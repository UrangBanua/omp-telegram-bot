# Panduan Agen OMP - omp-telegram-bot

## 1. Dokumentasi & Referensi Internal OMP
Untuk dokumentasi resmi, spesifikasi RPC, dan referensi internal OMP, agen WAJIB membaca file di:
`D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-guides/docs/omp/`

Khususnya:
- `docs/omp/05-programmatic/RPC mode.md` untuk spesifikasi protokol JSON-RPC stdio.
- `docs/omp/01-start/Compaction.md` & `Sessions.md` untuk manajemen sesi OMP.

## 2. Peran & Standar Kualitas Arsitektur Rust (Senior Architect)
Seluruh implementasi kode pada proyek ini harus mematuhi standar rekayasa perangkat lunak berikut:

- **Wajib Verifikasi & Dry-Run Sintaks:** Setiap kali melakukan penambahan atau modifikasi kode, agen WAJIB melakukan verifikasi sintaks/struktur penutup blok kurung kurawal (`{ ... }`) dan memastikan tidak ada *unclosed delimiter* sebelum menyerahkan hasil ke pengguna.
- **Pencegahan Stack Overflow (Windows Environment):**
  - Seluruh endpoint handler Teloxide (`command_handler`, `message_handler`, `callback_handler`) WAJIB langsung mendelegasikan eksekusinya ke `tokio::spawn(async move { ... })` agar pohon eksekusi `dptree` tidak menumpuk *future state machine* di stack thread utama.
  - Runtime Tokio wajib dikonfigurasi dengan alokasi stack minimal 4 MB (`tokio::runtime::Builder::new_multi_thread().thread_stack_size(4 * 1024 * 1024)`).
- **Streaming I/O pada File Sesi:** DILARANG menggunakan `std::fs::read_to_string` pada file riwayat sesi `.jsonl` (karena ukurannya bisa mencapai beberapa MB). Wajib menggunakan `std::io::BufReader` dengan pembatasan baris (`take(N)`) dan dijalankan di dalam `tokio::task::spawn_blocking`.
- **Asynchronous & Concurrency:** Gunakan runtime `tokio` secara murni. DILARANG menjalankan blocking call (misal synchronous sleep atau blocking std::fs/std::io) di dalam async context thread pool.
- **Idiomatic Error Handling:** Gunakan `anyhow::Result` untuk level application/entry-point dan `thiserror` jika membuat custom domain error. Jangan menggunakan `unwrap()` atau `expect()` pada runtime path produksi.
- **Telegram Rate-Limiting & Flood Control:** Implementasikan *debounced buffer* (throttling ~1.0–1.5 detik) saat melakukan edit pesan teks hasil streaming dari OMP agar tidak terkena HTTP 429 Flood Control dari Telegram API.
- **Smart Markdown Chunker:** Telegram memiliki batas 4096 karakter per pesan. Gunakan `split_markdown_into_html_messages` yang memotong berdasarkan batas seksi/blok kode sehingga seluruh tag HTML (`<pre>`, `<code>`, `<b>`, `<i>`) selalu tertutup sempurna dan valid 100%.
- **Perataan Format Teks Telegram:** Format paragraf teks biasa, heading bold `<b>`, data tabel terstruktur (`📋`), dan blok `<pre><code>` terisolasi hanya untuk script/kode & tool execution blockquote.
- **Keamanan Debug Logging:** Log level `debug!` hanya boleh mencatat nama tipe command/event struktural (`type_name`) dan DILARANG mencetak raw JSON/text input rahasia (API key, password).
- **Modular Design:**
  - `src/main.rs`: Inisialisasi runtime 4 MB, konfigurasi, logger, notifikasi lifecycle, dan listener Teloxide.
  - `src/omp_client.rs`: Manajer child process OMP RPC, I/O streaming, handling event `ready`, respawn otomatis, dan mpsc channels.
  - `src/handlers.rs`: Router perintah Telegram, callback query confirmation, debounced live stream, dan otorisasi ID pengguna.
  - `src/types.rs`: Definisi struct/enum payload RPC dan event OMP.
  - `src/utils.rs`: Konverter Markdown ke Telegram HTML, session scanner, chunking, dan sanitasi.
