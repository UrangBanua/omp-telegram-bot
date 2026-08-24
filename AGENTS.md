# Panduan Agen OMP - omp-telegram-bot

## 1. Dokumentasi & Referensi Internal OMP
Untuk dokumentasi resmi, spesifikasi RPC, dan referensi internal OMP, agen WAJIB membaca file di:
`D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-guides/docs/omp/`

Khususnya:
- `docs/omp/05-programmatic/RPC mode.md` untuk spesifikasi protokol JSON-RPC stdio.
- `docs/omp/01-start/Compaction.md` & `Sessions.md` untuk manajemen sesi OMP.

## 2. Peran & Standar Kualitas Arsitektur Rust (Senior Architect)
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
- **Wajib Verifikasi & Dry-Run Sintaks:** Setiap kali melakukan penambahan atau modifikasi kode, agen WAJIB melakukan verifikasi sintaks/struktur penutup blok kurung kurawal (`{ ... }`) dan memastikan tidak ada *unclosed delimiter* serta menjalankan `cargo test` sebelum menyerahkan hasil ke pengguna.
- **Asynchronous & Concurrency:** Gunakan runtime `tokio` secara murni. DILARANG menjalankan blocking call (misal synchronous sleep atau blocking std::fs/std::io) di dalam async context thread pool.
- **Idiomatic Error Handling:** Gunakan `anyhow::Result` untuk level application/entry-point dan `thiserror` jika membuat custom domain error. Jangan menggunakan `unwrap()` atau `expect()` pada runtime path produksi.
- **Telegram Rate-Limiting & Flood Control:** Implementasikan *debounced buffer* (throttling ~1.0–1.5 detik) saat melakukan edit pesan teks hasil streaming dari OMP agar tidak terkena HTTP 429 Flood Control dari Telegram API.
- **Smart Markdown Chunker:** Telegram memiliki batas 4096 karakter per pesan. Gunakan `split_markdown_into_html_messages` yang memotong berdasarkan batas seksi/blok kode sehingga seluruh tag HTML (`<pre>`, `<code>`, `<b>`, `<i>`) selalu tertutup sempurna dan valid 100%.
- **Perataan Format Teks Telegram:** Format paragraf teks biasa, heading bold `<b>`, data tabel kartu terstruktur modern (`• <b>Item</b> ├─ / └─`), dan blok `<pre><code>` terisolasi hanya untuk script/kode & tool execution blockquote.
- **Keamanan Debug Logging:** Log level `debug!` hanya boleh mencatat nama tipe command/event struktural (`type_name`) dan DILARANG mencetak raw JSON/text input rahasia (API key, password).
- **Modular Design:**
  - `src/main.rs`: Dedicated 8MB runner, inisialisasi runtime, konfigurasi, logger, notifikasi lifecycle, dan dispatcher Teloxide.
  - `src/omp_client.rs`: Manajer child process OMP RPC, I/O streaming, handling event `ready`, respawn otomatis, dan mpsc channels.
  - `src/handlers.rs`: Router perintah Telegram, callback query confirmation, debounced live stream, dan otorisasi ID pengguna.
  - `src/types.rs`: Definisi struct/enum payload RPC dan event OMP.
  - `src/utils.rs`: Konverter Markdown ke Telegram HTML, session scanner, chunking, dan sanitasi.
