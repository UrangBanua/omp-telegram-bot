//! Entry point utama untuk OMP Telegram Bot Bridge.

mod handlers;
mod omp_client;
mod types;
mod utils;

use handlers::{command_handler, message_handler, Command};
use log::{debug, error, info, warn};
use omp_client::OmpClient;
use std::sync::Arc;
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use types::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Inisialisasi Logger
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    // 2. Muat variabel lingkungan dari .env
    if let Err(e) = dotenvy::dotenv() {
        warn!("File .env tidak ditemukan atau gagal dimuat: {}. Menggunakan environment sistem.", e);
    } else {
        debug!("File .env berhasil dibaca.");
    }

    let config = match AppConfig::load_from_env() {
        Ok(c) => c,
        Err(e) => {
            error!("Kesalahan konfigurasi: {:#}", e);
            std::process::exit(1);
        }
    };

    info!("Konfigurasi berhasil dimuat.");
    info!("Target Project Workspace: {:?}", config.project_workspace);
    info!("Whitelist User ID: {:?}", config.allowed_user_ids);

    // 3. Inisialisasi OMP RPC Client
    debug!("Menginisialisasi OmpClient...");
    let client = OmpClient::start(config.clone());
    let shared_config = Arc::new(config.clone());

    // 4. Inisialisasi Bot Telegram
    debug!("Menginisialisasi Teloxide Bot instance...");
    let bot = Bot::new(&config.teloxide_token);
    // 5. Daftarkan menu perintah autocomplete ke Telegram API
    info!("Mendaftarkan menu autocomplete perintah ke Telegram...");
    if let Err(e) = bot.set_my_commands(Command::bot_commands()).await {
        log::warn!("Gagal mendaftarkan menu perintah ke Telegram API: {:#}", e);
    } else {
        info!("Menu perintah berhasil didaftarkan di Telegram.");
    }

    // 6. Buat Handler Dispatcher Teloxide dengan dptree branching
    debug!("Menyusun pohon handler (dptree)...");
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler),
        )
        .branch(
            Update::filter_message()
                .endpoint(message_handler),
        );
    info!("OMP Telegram Bot Bridge siap berjalan! Menunggu pesan masuk...");

    // 7. Jalankan Dispatcher dengan penanganan graceful shutdown bawaan Teloxide
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![client, shared_config])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    info!("Aplikasi telah dimatikan dengan bersih.");
    Ok(())
}
