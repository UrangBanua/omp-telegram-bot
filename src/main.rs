//! Entry point utama untuk OMP Telegram Bot Bridge.

mod handlers;
mod omp_client;
mod types;
mod utils;

use handlers::{callback_handler, command_handler, message_handler, Command};
use log::{debug, error, info, warn};
use omp_client::OmpClient;
use std::collections::HashSet;
use std::sync::Arc;
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;
use types::{AppConfig, RpcCommand, RpcEvent};
use utils::escape_html;
fn main() -> anyhow::Result<()> {
    // Jalankan seluruh runtime dan dispatcher pada thread dengan stack size 8 MB (kebal stack overflow limit 1 MB OS Windows)
    let builder = std::thread::Builder::new()
        .name("omp-bot-runner".into())
        .stack_size(8 * 1024 * 1024);

    let handler = builder.spawn(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_stack_size(8 * 1024 * 1024)
            .build()
            .expect("Gagal membuat multi-thread Tokio runtime");

        runtime.block_on(async_main())
    })?;

    handler.join().map_err(|_| anyhow::anyhow!("Thread runner mengalami error"))?
}
async fn async_main() -> anyhow::Result<()> {
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
    // 6. Background Event Watcher untuk memantau status OMP Engine (Up/Down) & Notifikasi Startup 3 Baris
    let watcher_bot = bot.clone();
    let watcher_users = config.allowed_user_ids.clone();
    let watcher_workspace = config.project_workspace.clone();
    let watcher_client = client.clone();
    let mut event_rx = client.subscribe();

    tokio::spawn(async move {
        let mut has_notified_startup = false;
        while let Ok(event) = event_rx.recv().await {
            match event {
                RpcEvent::Ready { .. } => {
                    debug!("Menerima Ready event di watcher, meminta GetState...");
                    let _ = watcher_client.send_command(RpcCommand::GetState { id: None }).await;
                }
                RpcEvent::Response { command, success, data, .. } => {
                    if command.as_deref() == Some("get_state") && success {
                        if !has_notified_startup {
                            has_notified_startup = true;
                            let session_title_disk = utils::get_active_session_title(&watcher_workspace);
                            let session_title_state = data.as_ref()
                                .and_then(|d| d.get("sessionName"))
                                .and_then(|s| s.as_str())
                                .filter(|s| !s.trim().is_empty() && *s != "Sesi Utama");

                            let final_session_title = session_title_state.unwrap_or(&session_title_disk);

                            // Notifikasi Startup Tepat 3 Baris
                            let startup_msg = format!(
                                "🚀 <b>OMP Telegram Bot Aktif</b>\n\
                                📄 <b>Nama Sesi:</b> <code>{}</code>\n\
                                📁 <b>Workspace:</b> <code>{}</code>",
                                escape_html(final_session_title),
                                escape_html(&watcher_workspace.to_string_lossy())
                            );
                            notify_admins(&watcher_bot, &watcher_users, &startup_msg).await;
                        }
                    }
                }
                RpcEvent::Disconnected => {
                    has_notified_startup = false; // Reset agar saat reconnect bisa re-notify jika perlu
                    let msg = "⚠️ <b>OMP Core Engine Terputus!</b>\n<i>Mencoba auto-respawn di background...</i>";
                    notify_admins(&watcher_bot, &watcher_users, msg).await;
                }
                _ => {}
            }
        }
    });

    // 7. Buat Handler Dispatcher Teloxide dengan dptree branching
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
        )
        .branch(
            Update::filter_callback_query()
                .endpoint(callback_handler),
        );

    info!("OMP Telegram Bot Bridge siap berjalan! Menunggu pesan masuk...");

    // 8. Jalankan Dispatcher dengan penanganan graceful shutdown bawaan Teloxide
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
