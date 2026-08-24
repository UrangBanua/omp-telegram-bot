# Panduan Agen OMP - omp-telegram-bot

## 1. Dokumentasi & Referensi Internal OMP
Untuk dokumentasi resmi, spesifikasi RPC, dan referensi internal OMP, agen WAJIB membaca file di:
`D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-guides/docs/omp/`

Khususnya:
- `docs/omp/05-programmatic/RPC mode.md` untuk spesifikasi protokol JSON-RPC stdio.
- `docs/omp/01-start/Compaction.md` & `Sessions.md` untuk manajemen sesi OMP.
- `docs/omp/07-internals/RPC Protocol Reference.md` untuk kontrak payload kanonikal (`prompt`, `steer`, `abort`, `set_session_name`, `switch_session`, `get_state`, `compact`).

---

## 2. Standar Kualitas Arsitektur Rust (Senior Architect)
Seluruh implementasi kode pada proyek ini harus mematuhi standar rekayasa perangkat lunak berikut:

- **Kompatibilitas Lintas Platform (Windows & Linux Server):**
  - Seluruh kode harus 100% portable antara Windows 11 dan Linux Server (Debian/Ubuntu/Alpine).
  - Konfigurasi linker stack size di `.cargo/config.toml` wajib di-scope per target (`x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`).
  - Path workspace harus dinormalisasi menggunakan `std::path::PathBuf` agar aman dari perbedaan path separator (`/` vs `\`).
- **Pencegahan Stack Overflow & Thread Isolation:**
  - `main()` wajib men-spawn dedicated thread runner dengan alokasi stack 8 MB (`std::thread::Builder::new().stack_size(8 * 1024 * 1024)`).
  - Runtime Tokio wajib dikonfigurasi dengan alokasi stack minimal 8 MB (`tokio::runtime::Builder::new_multi_thread().thread_stack_size(8 * 1024 * 1024)`).
  - Seluruh endpoint handler Teloxide (`command_handler`, `message_handler`, `callback_handler`) WAJIB langsung mendelegasikan eksekusinya ke `tokio::spawn(async move { ... })` agar pohon eksekusi `dptree` tidak menumpuk *future state machine* di stack dispatcher.
- **Streaming I/O pada File Sesi:** DILARANG menggunakan `std::fs::read_to_string` pada file riwayat sesi `.jsonl` (karena ukurannya bisa mencapai beberapa MB). Wajib menggunakan `std::io::BufReader` dengan pembatasan baris (`take(N)`) dan dijalankan di dalam `tokio::task::spawn_blocking`.
- **Manajemen Sesi Otomatis & Proteksi Reset:**
  - Bot secara default menjalankan `omp` dengan flag `-c` (`--continue`) agar otomatis me-resume sesi aktif terakhir.
  - Perintah `/new` wajib meminta konfirmasi pengguna via *Telegram Inline Keyboard* sebelum mereset sesi.
  - Perintah `/resume` memindai sesi disk via `list_workspace_sessions` dan berpindah sesi via RPC `switch_session`.
  - Perintah `/rename <nama>` memperbarui judul sesi pada file riwayat disk via RPC `set_session_name`.
- **Asynchronous & Concurrency:** Gunakan runtime `tokio` secara murni. DILARANG menjalankan blocking call (misal synchronous sleep atau blocking std::fs/std::io) di dalam async context thread pool.
- **Idiomatic Error Handling:** Gunakan `anyhow::Result` untuk level application/entry-point dan `thiserror` jika membuat custom domain error. Jangan menggunakan `unwrap()` atau `expect()` pada runtime path produksi.
- **Telegram Rate-Limiting & Flood Control:** Implementasikan *debounced buffer* (throttling ~1.0–1.5 detik) saat melakukan edit pesan teks hasil streaming dari OMP agar tidak terkena HTTP 429 Flood Control dari Telegram API.
- **Smart Markdown Chunker:** Telegram memiliki batas 4096 karakter per pesan. Gunakan `split_markdown_into_html_messages` yang memotong berdasarkan batas seksi/blok kode sehingga seluruh tag HTML (`<pre>`, `<code>`, `<b>`, `<i>`) selalu tertutup sempurna dan valid 100%.
- **Perataan Format Teks Telegram:** Format paragraf teks biasa, heading bold `<b>`, data tabel pohon kartu terstruktur (`• <b>Item</b> ├─ / └─`), dan blok `<pre><code>` terisolasi hanya untuk script/kode & tool execution blockquote.
- **Keamanan Debug Logging:** Log level `debug!` hanya boleh mencatat nama tipe command/event struktural (`type_name`) dan DILARANG mencetak raw JSON/text input rahasia (API key, password).
- **Wajib Verifikasi Sintaks & Dry-Run Test:** Setiap kali ada modifikasi kode pada `src/`, jalankan `cargo test` untuk memverifikasi keutuhan fungsi sebelum menyerahkan hasil ke pengguna.

---

## 3. Pemetaan Command Telegram ke JSON-RPC OMP

| Command Telegram | RPC Command | Payload / Perilaku |
| :--- | :--- | :--- |
| **`/start`** | — | Menampilkan status engine, workspace aktif, dan panduan menu. |
| **`/new`** | `new_session` | Mengirim konfirmasi Inline Keyboard sebelum membuat sesi baru. |
| **`/resume`** | `switch_session` | Memindai sesi lokal disk dan menampilkan tombol picker sesi. |
| **`/rename <nama>`** | `set_session_name` | Mengubah judul sesi aktif saat ini di file `.jsonl`. |
| **`/status`** | `get_state` | Mengambil snapshot kapasitas context window (`%`), model, dan status. |
| **`/abort`** | `abort` | Menghentikan paksa aksi AI seketika (*Emergency Stop*). |
| **`/steer <pesan>`** | `steer` | Menyisipkan arahan di tengah eksekusi turn aktif (*in-flight*). |
| **`/model <nama>`** | `set_model` | Mengganti model AI aktif (misal: `gemini-3.7-flash`). |
| **`/thinking <lvl>`** | `set_thinking_level` | Mengatur level reasoning (`off` s.d. `max`). |
| **`/compact`** | `compact` | Meringkas memori percakapan aktif untuk hemat token. |
| **Teks Biasa** | `prompt` | Dikirim sebagai instruksi koding / tugas ke OMP. |
| **Kirim Gambar** | `prompt` | Dikirim sebagai multimodal base64 bersama caption pertanyaan. |

---

## 4. Struktur Modul & Tanggung Jawab Kode

- `src/main.rs`: Dedicated 8MB thread runner, inisialisasi runtime, konfigurasi, logger, notifikasi lifecycle (Startup 3-baris, Ready, Disconnected, Shutdown), dan dispatcher Teloxide.
- `src/omp_client.rs`: Manajer child process OMP RPC, I/O stdio streaming, handling event `ready`, auto-respawn, dan channel MPSC/Broadcast.
- `src/handlers.rs`: Router perintah Telegram, callback query confirmation, debounced live stream, and otorisasi ID pengguna.
- `src/types.rs`: Definisi struct/enum payload RPC dan event OMP.
- `src/utils.rs`: Smart Markdown chunker, structured card table formatter, session scanner, HTML sanitizer, dan unit test suite.
