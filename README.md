# OMP Telegram Bot Bridge (Rust)

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Tokio](https://img.shields.io/badge/Async-Tokio-blue.svg?logo=tokio)](https://tokio.rs/)
[![Teloxide](https://img.shields.io/badge/Telegram-Teloxide-2CA5E0.svg?logo=telegram)](https://github.com/teloxide/teloxide)
[![OMP](https://img.shields.io/badge/Engine-Oh%20My%20Pi%20RPC-8A2BE2.svg)](https://omp.sh)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20Server-green.svg)](https://github.com/)

> **High-Performance Asynchronous Bridge** berbasis bahasa Rust yang menghubungkan bot Telegram langsung ke **OMP Core Engine (`omp --mode rpc`)** via protokol *Newline-Delimited JSON-RPC stdio streams*. Memungkinkan kontrol penuh agen koding OMP secara remote melalui Telegram dengan perlindungan otorisasi, debounced live-streaming, dan isolasi sesi otomatis.

---

## 🏛️ Arsitektur Sistem

```text
┌──────────────────────┐                        ┌────────────────────────────────────────────────────────┐                          ┌──────────────────────────────────┐
│                      │                        │              Rust Telegram Bridge (Teloxide)           │                          │                                  │
│ Telegram API (Cloud) ◄──Long-Polling / Http──►│  - Dedicated 8 MB Thread Runner                        │◄──Async Tokio Stdio I/O─►│ OMP Core Engine (omp --mode rpc) │
│                      │                        │  - Debounced Message Updater (Throttling ~1.2s)        │   (Newline-Delimited     │                                  │
│                      │                        │  - Smart Semantic Markdown Chunker                     │    JSON-RPC Protocol)    │                                  │
│                      │                        │  - Interactive Session Switcher (/resume, /new)        │                          │                                  │
└──────────────────────┘                        └────────────────────────────────────────────────────────┘                          └──────────────────────────────────┘
```

---

## ✨ Fitur Utama

- ⚡ **Asynchronous Stdio JSON-RPC Stream**: Mengendalikan subprocess `omp --mode rpc` secara native menggunakan non-blocking I/O Tokio.
- 🔄 **Auto-Resume & Session Switcher (`/resume`)**: Otomatis melanjutkan sesi terakhir saat startup (`-c`) dan menyediakan antarmuka *Telegram Inline Keyboard* untuk berpindah sesi (*Hot Switch*).
- 🛡️ **Whitelist Otorisasi Akun (`ALLOWED_USER_IDS`)**: Membatasi eksekusi perintah dan shell hanya untuk akun Telegram yang terdaftar.
- 💬 **Debounced Live Streaming (Anti-Flood 429)**: Mengalirkan respons teks secara berkala (~1.2 detik) dengan animasi kursor dinamis tanpa melanggar *rate limit* Telegram.
- 📋 **Smart Markdown & Code Isolation**: Memotong pesan berdasarkan batas heading/paragraf dan mengisolasi script kode ke dalam balon pesan tersendiri sehingga tombol **"Copy"** Telegram berfungsi sempurna.
- 📸 **Dukungan Multimodal (Foto / Screenshot)**: Menerima foto/screenshot error dari Telegram, mengonversinya ke Base64, dan menyuapkannya langsung ke OMP RPC.
- 🚀 **Notifikasi Siklus Hidup (Lifecycle Broadcast)**: Mengirimkan pesan status otomatis ke admin saat bot *Startup*, *Ready*, *Disconnected (Auto-Restart)*, dan *Shutdown*.
- 🪟 **Kompatibilitas Penuh Windows 11 & Linux Server**: Dilengkapi konfigurasi alokasi 8 MB stack per thread untuk performa stabil di Windows maupun server produksi Linux.

---

## 📋 Daftar Perintah Telegram (Bot Commands)

| Perintah | Deskripsi & Perilaku |
| :--- | :--- |
| **`/start`** | Menampilkan status koneksi OMP, workspace aktif, dan panduan menu. |
| **`/new`** | Menampilkan tombol konfirmasi interaktif untuk membuat sesi baru (mengarsipkan sesi lama). |
| **`/resume`** | Memindai riwayat sesi lokal di disk dan menampilkan menu tombol untuk beralih sesi. |
| **`/status`** | Mengambil snapshot status *real-time* (model aktif, level thinking, konsumsi token, pesan). |
| **`/abort`** | Menghentikan paksa aksi/proses koding AI seketika (*Emergency Stop*). |
| **`/steer <pesan>`** | Menyisipkan arahan/koreksi di tengah-tengah proses agen yang sedang berjalan. |
| **`/model <nama>`** | Mengganti model AI aktif (misal: `gemini-3.7-flash`, `claude-3-7-sonnet`). |
| **`/thinking <level>`** | Mengatur kedalaman reasoning (`off`, `minimal`, `low`, `medium`, `high`, `max`). |
| **`/compact`** | Memerintahkan OMP meringkas riwayat chat di memori untuk menghemat token. |
| **Pesan Teks Biasa** | Dikirimkan langsung sebagai prompt tugas/koding ke agen OMP. |
| **Kirim Gambar/Foto** | Dikirimkan sebagai prompt multimodal bersama caption pertanyaan Anda. |

---

## 🚀 Panduan Instalasi & Penggunaan

### 1. Prasyarat Sistem
- **Rust Toolchain**: Rust 1.75+ (`rustup`, `cargo`).
- **OMP CLI**: Binary `omp` sudah terpasang dan dapat dipanggil dari terminal.
- **Bot Telegram**: Token bot dari [@BotFather](https://t.me/BotFather).
- **User ID Telegram**: ID akun Anda dari [@userinfobot](https://t.me/userinfobot).

---

### 2. Konfigurasi Lingkungan (`.env`)
Salin file template `.env.example` menjadi `.env`:

```bash
# Di Windows
copy .env.example .env

# Di Linux / macOS
cp .env.example .env
```

Sesuaikan nilai variabel di dalam `.env`:
```ini
# Token API Telegram dari @BotFather
TELOXIDE_TOKEN=123456789:ABCdefGHIjklMNOpqrSTUvwxYZ

# Whitelist ID Telegram yang diizinkan (pisahkan koma jika > 1 ID)
ALLOWED_USER_IDS=923843583

# Direktori proyek/workspace target yang ingin dikelola OMP
PROJECT_WORKSPACE=D:/PROJECTS/AfniAfdillah/RepositoryGitHub/omp-guides

# Path executable OMP CLI (default "omp")
OMP_BIN_PATH=omp

# Level pencatatan log (info / debug / trace)
RUST_LOG=info
```

---

### 3. Menjalankan Bot di Lokal
```bash
cargo run
```

Untuk menjalankan unit test *dry-run*:
```bash
cargo test
```

---

## 🐧 Panduan Deploy ke Linux Server (Production)

### 1. Build Binary Release
```bash
cargo build --release
```
Binary hasil kompilasi akan berada di `target/release/omp-telegram-bot`.

### 2. Jalankan sebagai Service Systemd (Rekomendasi Linux)
Buat file service systemd di `/etc/systemd/system/omp-telegram-bot.service`:

```ini
[Unit]
Description=OMP Telegram Bot Bridge Service
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/omp-telegram-bot
ExecStart=/home/ubuntu/omp-telegram-bot/target/release/omp-telegram-bot
Restart=always
RestartSec=5
EnvironmentFile=/home/ubuntu/omp-telegram-bot/.env

[Install]
WantedBy=multi-user.target
```

Aktifkan dan jalankan service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable omp-telegram-bot
sudo systemctl start omp-telegram-bot
sudo systemctl status omp-telegram-bot
```

---

## 📂 Struktur Modul Proyek

```text
omp-telegram-bot/
├── Cargo.toml          # Dependensi crate (Teloxide, Tokio, Serde, Reqwest, Base64)
├── .cargo/
│   └── config.toml     # Konfigurasi linker stack 8 MB cross-platform (Windows & Linux)
├── .env.example        # Template konfigurasi environment
├── AGENTS.md           # Standar kualitas arsitektur & aturan persistensi agen
├── plans/              # Blueprint teknis canonical
└── src/
    ├── main.rs         # Entry point, runner 8 MB, notifikasi lifecycle, dispatcher
    ├── omp_client.rs   # Manajer subprocess OMP RPC, I/O stdio stream, auto-respawn
    ├── handlers.rs     # Router command Telegram, callback confirmation, live stream
    ├── types.rs        # Data contracts JSON-RPC (Commands, Events, AppConfig)
    └── utils.rs        # Smart Markdown chunker, session scanner, HTML sanitizer
```

---

## 📄 Lisensi
Didistribusikan di bawah lisensi MIT.
