//! Entry point utama untuk OMP Telegram Bot Bridge.

mod handlers;
mod omp_client;
mod types;
mod utils;

use handlers::{command_handler, message_handler, Command};
use log::{debug, error, info, warn};
use omp_client::OmpClient;
use std::collections::HashSet;
use std::sync::Arc;
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;
use types::{AppConfig, RpcEvent};
use utils::escape_html;
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
        warn!("Gagal mendaftarkan menu perintah ke Telegram API: {:#}", e);
    } else {
        info!("Menu perintah berhasil didaftarkan di Telegram.");
    }

    // 6. Notifikasi Startup ke Telegram Admin
    let startup_msg = format!(
        "🚀 <b>OMP Telegram Bridge Aktif</b>\n\
        📁 <b>Workspace:</b> <code>{}</code>\n\
        ⏳ <i>Menghubungkan ke OMP Core Engine...</i>",
        escape_html(&config.project_workspace.to_string_lossy())
    );
    notify_admins(&bot, &config.allowed_user_ids, &startup_msg).await;

    // 7. Background Event Watcher untuk memantau status OMP Engine (Up/Down)
    let watcher_bot = bot.clone();
    let watcher_users = config.allowed_user_ids.clone();
    let mut event_rx = client.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                RpcEvent::Ready { .. } => {
                    let msg = "🟢 <b>OMP Core Engine Terhubung (Ready)</b>\n<i>Siap menerima instruksi dan prompt koding!</i>";
                    notify_admins(&watcher_bot, &watcher_users, msg).await;
                }
                RpcEvent::Disconnected => {
                    let msg = "⚠️ <b>OMP Core Engine Terputus!</b>\n<i>Mencoba auto-respawn di background...</i>";
                    notify_admins(&watcher_bot, &watcher_users, msg).await;
                }
                _ => {}
            }
        }
    });

    // 8. Buat Handler Dispatcher Teloxide dengan dptree branching
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

    // 9. Jalankan Dispatcher dengan penanganan graceful shutdown bawaan Teloxide
    Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![client, shared_config])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    // 10. Notifikasi Shutdown ke Telegram Admin
    info!("Mengirim notifikasi shutdown ke admin...");
    let shutdown_msg = "🔴 <b>OMP Telegram Bridge Dimatikan (Offline)</b>\n<i>Aplikasi dan proses OMP telah dihentikan secara aman.</i>";
    notify_admins(&bot, &config.allowed_user_ids, shutdown_msg).await;

    info!("Aplikasi telah dimatikan dengan bersih.");
    Ok(())
}

/// Mengirimkan notifikasi broadcast ke seluruh User ID yang terdaftar dalam whitelist admin.
async fn notify_admins(bot: &Bot, allowed_user_ids: &HashSet<u64>, text: &str) {
    for &user_id in allowed_user_ids {
        let _ = bot.send_message(ChatId(user_id as i64), text)
            .parse_mode(ParseMode::Html)
            .await;
    }
}
