# Plan: OMP Telegram Bot Bridge (Rust)

## Context
Pengguna ingin membangun jembatan (bridge) `omp-telegram-bot` berbasis bahasa Rust yang menghubungkan antarmuka Telegram (framework `teloxide`) secara langsung ke OMP Core Engine (`omp --mode rpc`) melalui asynchronous stdio streams (JSON-RPC). Bot ini memungkinkan kontrol penuh agen koding OMP secara remote melalui Telegram dengan perlindungan otorisasi ID, manajemen streaming berkecepatan tinggi tanpa melanggar rate-limit Telegram, dan pemulihan otomatis jika proses OMP terputus.

---

## Blueprint Arsitektur

```text
┌──────────────────────┐                        ┌────────────────────────────────────────────────────────┐                          ┌──────────────────────────────────┐
│                      │                        │                   Rust Telegram Bridge                 │                          │                                  │
│ Telegram API (Cloud) ◄──Long-Polling / Http──►│  - Teloxide Dispatcher                                 │◄──Async Tokio Stdio I/O─►│ OMP Core Engine (omp --mode rpc) │
│                      │                        │  - Debounced Message Updater (Throttling ~1.2s)        │   (Newline-Delimited     │                                  │
│                      │                        │  - Message Chunker (Max 4096 Chars)                    │    JSON-RPC Protocol)    │                                  │
└──────────────────────┘                        └────────────────────────────────────────────────────────┘                          └──────────────────────────────────┘
```

---

## Approach

### 1. Inisialisasi Proyek & Manajemen Dependensi (`Cargo.toml`)
- Inisialisasi konfigurasi `Cargo.toml` dengan dependensi:
  - `teloxide` (v0.13, fitur `macros`, `ctrlc_handler`) untuk kerangka kerja Telegram bot.
  - `tokio` (v1.35+, fitur `full`, `process`, `sync`, `time`, `signal`) untuk runtime asinkron, timer debouncing, dan graceful shutdown.
  - `serde` (v1.0, fitur `derive`) & `serde_json` (v1.0) untuk serialisasi protokol RPC.
  - `base64` (v0.22) untuk encoding gambar multimodal ke JSON-RPC OMP.
  - `dotenvy` (v0.15) untuk membaca file konfigurasi lingkungan `.env`.
  - `anyhow` (v1.0) untuk penanganan error idiomatik.
  - `log` (v0.4) & `pretty_env_logger` (v0.5) untuk observabilitas log.


### 2. Setup Konfigurasi & Keamanan (`.env.example` & `.gitignore`)
- Buat file `.env.example`:
  ```ini
  # 1. Kredensial Bot Telegram (dari @BotFather)
  TELOXIDE_TOKEN=123456789:ABCdefGHIjklMNOpqrSTUvwxYZ

  # 2. Whitelist ID Telegram Pengguna yang Diizinkan (dipisahkan koma jika lebih dari satu)
  # Dapatkan ID Anda via bot @userinfobot di Telegram
  ALLOWED_USER_IDS=123456789,987654321

  # 3. Direktori Workspace Proyek yang Ingin Dikelola oleh OMP
  # Contoh: D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-guides atau path proyek lainnya
  PROJECT_WORKSPACE=D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-telegram-bot

  # 4. Path Binary Eksekusi CLI OMP (default "omp" jika sudah ada di PATH sistem)
  OMP_BIN_PATH=omp

  # 5. Level Pencatatan Log Sistem Rust
  RUST_LOG=info
  ```
- **Filter Otorisasi:** Hanya akun Telegram yang ID-nya terdaftar di `ALLOWED_USER_IDS` yang dapat mengeksekusi bot. Akses selain itu akan langsung ditolak atau diabaikan demi keamanan sistem lokal.
### 3. Pemodelan Data & Tipe Protokol RPC (`src/types.rs`)
Tambahkan definisi schema sesuai spesifikasi canonical OMP RPC (`docs/omp/07-internals/RPC Protocol Reference.md`):
- **Inbound RPC Commands (stdin):**
  - `prompt`: `{"id": "...", "type": "prompt", "message": String, "images": Option<Vec<String>>}`
  - `steer`: `{"id": "...", "type": "steer", "message": String}`
  - `abort`: `{"id": "...", "type": "abort"}`
  - `follow_up`: `{"id": "...", "type": "follow_up", "message": String}`
  - `new_session`: `{"id": "...", "type": "new_session"}`
  - `set_model`: `{"id": "...", "type": "set_model", "provider": Option<String>, "modelId": String}`
  - `set_thinking_level`: `{"id": "...", "type": "set_thinking_level", "level": String}`
  - `compact`: `{"id": "...", "type": "compact"}`
  - `get_state`: `{"id": "...", "type": "get_state"}`
- **Outbound RPC Events (stdout):**
  - `ready`: Membaca handshake awal OMP.
  - `agent_start` & `agent_end`: Penanda siklus eksekusi agen.
  - `message_update`: Streaming token teks (`text_delta`).
  - `tool_execution_start` & `tool_execution_end`: Memantau pemanggilan tool (misal `read`, `bash`, `edit`).

### 4. Modul OMP Client, Resilience & Graceful Shutdown (`src/omp_client.rs` & `src/main.rs`)
- **Spawn Subprocess (Persistent Session):** Memulai `omp --mode rpc` (TANPA flag `--no-session`) pada direktori `PROJECT_WORKSPACE` agar riwayat sesi tersimpan di `~/.omp/agent/sessions/`.
- **Handshake Ready:** Menunggu pembacaan event `{"type": "ready"}` pertama sebelum menandai client siap mengirim perintah.
- **Bi-Directional Channels:**
  - `tokio::sync::mpsc::Sender<RpcCommand>` untuk mengirim perintah dari handler Telegram ke stdin OMP.
  - `tokio::sync::broadcast::Sender<RpcEvent>` untuk menyiarkan event dari stdout OMP ke active task Telegram.
- **Auto-Respawn:** Jika subprocess OMP mati tak terduga (EOF pada stdout), modul otomatis men-spawn ulang proses baru dan mencatat peringatan di log.
- **Graceful Shutdown:** `tokio::signal::ctrl_c()` di `src/main.rs` memastikan saat bot dimatikan, subprocess OMP dihentikan secara bersih (menutup stdin & kill child process) sehingga tidak ada proses orphan di Task Manager Windows.

### 5. Debounced Streaming, Typing Status, Tool Indicator & Chunker (`src/handlers.rs` / `src/utils.rs`)
- **Background Typing Loop:** Mengirim `sendChatAction(ChatAction::Typing)` setiap ~4 detik selama fase thinking/tool berjalan agar status header Telegram menampilkan "typing...".
- **Tool Activity Indicator:** Saat event `tool_execution_start` diterima (misal `toolName: "bash"`), bot menyematkan status singkat pada pesan (misal `🛠️ [Tool: bash] ...`).
- **Debounced Live Streaming:** Mengumpulkan akumulasi `text_delta` dan memperbarui pesan Telegram setiap ~1.2 detik dengan kursor animasi (`▌`), menghindari rate limit HTTP 429.
- **Message Chunker & Safe Markdown:** Memotong pesan otomatis pada batas baris jika melebihi 4000 karakter dan membungkus tabel/markdown ke format yang aman dari error parsing Telegram.

### 6. Pemetaan Perintah Telegram & Multimodal Photo (`src/handlers.rs`)
- **Autocomplete Menu:** Saat bot startup, memanggil `bot.set_my_commands(Command::bot_commands()).await` agar daftar command otomatis muncul di menu popup Telegram `[/]`.
- `/start`: Menampilkan pesan selamat datang, status koneksi OMP, dan daftar perintah.
- `/new`: Mengirim command `new_session` untuk membersihkan riwayat dan memulai task baru.
- `/abort`: Mengirim command `abort` untuk menghentikan paksa aksi agen secara instan.
- `/steer <koreksi>`: Menyisipkan instruksi arahan di tengah eksekusi agen yang sedang berjalan.
- `/model <nama>`: Mengganti model aktif (misal `gemini-3.7-flash`, `claude-3-7-sonnet`).
- `/thinking <level>`: Mengubah level thinking model (`off`, `minimal`, `low`, `medium`, `high`, `max`).
- `/compact`: Memicu proses peringkasan sesi untuk menghemat token.
- `/status`: Mengambil status terkini (token usage, model aktif, tools aktif) via `get_state`.
- *Pesan teks biasa*: Otomatis dikirim sebagai `prompt` (atau `follow_up` jika agen sedang sibuk).
- *Pesan Gambar / Foto (Multimodal)*: Bot mengunduh file foto dari Telegram, mengonversi ke base64, dan mengirimkannya ke OMP RPC di dalam array `images` bersama caption teks pertanyaan pengguna.

## Critical Files & Anchors
- `AGENTS.md`: Menegakkan standar arsitektur Rust async dan referensi dokumentasi OMP.
- `Cargo.toml`: Konfigurasi dependensi dan optimasi build.
- `src/omp_client.rs`: Inti komunikasi inter-process asinkron, streaming I/O, dan auto-respawn.
- `src/handlers.rs`: Otentikasi pengguna, debounced updater, dan parsing perintah.
- `src/types.rs`: Kontrak data schema JSON-RPC OMP.

---

## Verification Plan

1. **Kompilasi & Analisis Kode:**
   - Jalankan `cargo check` & `cargo clippy` untuk memastikan nol error atau warning kritis.
2. **Uji Handshake & Status (End-to-End):**
   - Jalankan `cargo run`.
   - Pastikan log menampilkan `OMP RPC Ready`.
   - Kirim `/status` dari Telegram, pastikan bot merespons dengan snapshot JSON/teks status OMP.
3. **Uji Streaming & Debouncing:**
   - Kirim `/new buatkan fungsi quicksort di python lengkap dengan contoh`.
   - Amati pesan di Telegram yang ter-update secara berkala dan halus tanpa terkena flood limit.
4. **Uji Interupsi (`/steer` & `/abort`):**
   - Jalankan task panjang, kirim `/steer ubah ke bahasa rust` lalu `/abort`.
   - Pastikan proses agen merespons perubahan secara instan.
5. **Uji Keamanan Whitelist:**
   - Kirim pesan dari ID yang tidak ada di `ALLOWED_USER_IDS`.
   - Pastikan bot tidak memproses instruksi tersebut.

---

## Assumptions & Contingencies
- **Lingkungan OS:** `cargo` dan `rustc` akan terpasang sebelum tahap implementasi dimulai.
- **Fallback OMP:** Jika binary `omp` tidak berada di root PATH, path absolut dapat dikonfigurasi melalui variabel `OMP_BIN_PATH` di `.env`.
