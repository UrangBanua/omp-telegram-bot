# Panduan Agen OMP - omp-telegram-bot

## 1. Dokumentasi & Referensi Internal OMP
Untuk dokumentasi resmi, spesifikasi RPC, dan referensi internal OMP, agen WAJIB membaca file di:
`D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-guides/docs/omp/`

Khususnya:
- `docs/omp/05-programmatic/RPC mode.md` untuk spesifikasi protokol JSON-RPC stdio.
- `docs/omp/01-start/Compaction.md` & `Sessions.md` untuk manajemen sesi OMP.

## 2. Peran & Standar Kualitas Arsitektur Rust (Senior Architect)
Seluruh implementasi kode pada proyek ini harus mematuhi standar rekayasa perangkat lunak berikut:

- **Asynchronous & Concurrency:** Gunakan runtime `tokio` secara murni. DILARANG menjalankan blocking call (misal synchronous sleep atau blocking std::fs/std::io) di dalam async context thread pool.
- **Idiomatic Error Handling:** Gunakan `anyhow::Result` untuk level application/entry-point dan `thiserror` jika membuat custom domain error. Jangan menggunakan `unwrap()` atau `expect()` pada runtime path produksi.
- **Telegram Rate-Limiting & Flood Control:** Implementasikan *debounced buffer* (throttling ~1.0–1.5 detik) saat melakukan edit pesan teks hasil streaming dari OMP agar tidak terkena HTTP 429 Flood Control dari Telegram API.
- **Message Chunking:** Telegram memiliki batas 4096 karakter per pesan. Buat modul pemecah pesan (*chunker*) yang aman memotong pada batas baris atau markdown block.
- **Modular Design:**
  - `src/main.rs`: Inisialisasi konfigurasi, logger, dan listener Teloxide.
  - `src/omp_client.rs`: Manajer child process OMP RPC, I/O streaming, handling event `ready`, respawn otomatis, dan mpsc channels.
  - `src/handlers.rs`: Router perintah Telegram dan otorisasi ID pengguna.
  - `src/types.rs`: Definisi struct/enum payload RPC dan event OMP.
