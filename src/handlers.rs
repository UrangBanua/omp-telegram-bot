//! Router perintah Telegram, otorisasi pengguna, debounced live stream, dan multimodal handler.

use crate::omp_client::OmpClient;
use crate::types::{AppConfig, ImageContent, RpcCommand, RpcEvent};
use crate::utils::{chunk_message, escape_html, format_tool_status, markdown_to_telegram_html};
use base64::Engine;
use log::{debug, error, warn};
use std::sync::Arc;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::{ChatAction, MessageId, ParseMode};
use teloxide::utils::command::BotCommands;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};

/// Definisi perintah bot Telegram.
#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Daftar perintah yang didukung:")]
pub enum Command {
    #[command(description = "Menampilkan petunjuk penggunaan dan status koneksi bot")]
    Start,
    #[command(description = "Memulai sesi percakapan koding baru (/new atau /new <instruksi>)")]
    New(String),
    #[command(description = "Menghentikan paksa aksi/pemikiran AI (Emergency Stop)")]
    Abort,
    #[command(description = "Menyisipkan koreksi/arahan di tengah proses agen")]
    Steer(String),
    #[command(description = "Mengganti model AI aktif (misal: gemini-3.7-flash, claude-3-7-sonnet)")]
    Model(String),
    #[command(description = "Mengatur level reasoning thinking (off|minimal|low|medium|high|max)")]
    Thinking(String),
    #[command(description = "Meringkas memori percakapan untuk menghemat token")]
    Compact,
    #[command(description = "Memeriksa status OMP, model aktif, dan konsumsi token")]
    Status,
}

/// Helper untuk memeriksa apakah user yang mengirim pesan berada dalam whitelist.
fn is_authorized(msg: &Message, config: &AppConfig) -> bool {
    if config.allowed_user_ids.is_empty() {
        return true;
    }
    match msg.from.as_ref() {
        Some(user) => config.allowed_user_ids.contains(&user.id.0),
        None => false,
    }
}

/// Handler khusus untuk perintah Telegram (Command).
pub async fn command_handler(
    bot: Bot,
    msg: Message,
    cmd: Command,
    client: OmpClient,
    config: Arc<AppConfig>,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
    debug!("Menerima command dari user_id: {}", user_id);

    if !is_authorized(&msg, &config) {
        warn!("Akses command ditolak untuk user ID: {}", user_id);
        bot.send_message(
            msg.chat.id,
            "⛔ <b>Akses Ditolak</b>\nID akun Anda tidak terdaftar dalam whitelist bot.",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    debug!("Mengeksekusi command: {:?}", cmd);
    let chat_id = msg.chat.id;
    match cmd {
        Command::Start => {
            let ready_status = if client.is_ready() {
                "🟢 <b>Siap (Terkoneksi ke OMP RPC)</b>"
            } else {
                "🟡 <i>Menghubungkan ke OMP RPC...</i>"
            };

            let help_text = format!(
                "🤖 <b>OMP Telegram Bridge</b>\n\n\
                Status Engine: {}\n\n\
                <b>Menu Perintah:</b>\n\
                /new &lt;prompt&gt; - Mulai sesi koding baru\n\
                /abort - Hentikan paksa proses AI\n\
                /steer &lt;pesan&gt; - Beri arahan koreksi di tengah proses\n\
                /model &lt;nama&gt; - Ganti model AI aktif\n\
                /thinking &lt;level&gt; - Atur level thinking\n\
                /compact - Ringkas riwayat chat\n\
                /status - Cek status token dan model\n\n\
                <i>Kirim pesan teks atau foto langsung untuk mulai koding!</i>",
                ready_status
            );

            bot.send_message(chat_id, help_text)
                .parse_mode(ParseMode::Html)
                .await?;
        }

        Command::New(prompt) => {
            if let Err(e) = client.send_command(RpcCommand::NewSession { id: None, parent_session: None }).await {
                bot.send_message(chat_id, format!("❌ Gagal memulai sesi baru: {:#}", e)).await?;
                return Ok(());
            }

            if prompt.trim().is_empty() {
                bot.send_message(chat_id, "✨ <b>Sesi koding baru telah dimulai!</b>\nRiwayat lama telah diarsipkan.")
                    .parse_mode(ParseMode::Html)
                    .await?;
            } else {
                bot.send_message(chat_id, "✨ <b>Sesi baru dimulai dengan instruksi...</b>")
                    .parse_mode(ParseMode::Html)
                    .await?;
                tokio::spawn(async move {
                    let _ = execute_prompt_and_stream(bot, chat_id, prompt, None, client).await;
                });
            }
        }

        Command::Abort => {
            if let Err(e) = client.send_command(RpcCommand::Abort { id: None }).await {
                bot.send_message(chat_id, format!("❌ Gagal mengirim sinyal abort: {:#}", e)).await?;
            } else {
                bot.send_message(chat_id, "🛑 <b>Sinyal Abort (Stop) dikirimkan ke OMP.</b>")
                    .parse_mode(ParseMode::Html)
                    .await?;
            }
        }

        Command::Steer(message) => {
            if message.trim().is_empty() {
                bot.send_message(chat_id, "⚠️ Berikan pesan arahan: <code>/steer ubah ke TypeScript</code>")
                    .parse_mode(ParseMode::Html)
                    .await?;
                return Ok(());
            }

            if let Err(e) = client.send_command(RpcCommand::Steer { id: None, message: message.clone(), images: None }).await {
                bot.send_message(chat_id, format!("❌ Gagal mengirim arahan: {:#}", e)).await?;
            } else {
                bot.send_message(chat_id, format!("🧭 <b>Arahan disisipkan:</b> <i>{}</i>", escape_html(&message)))
                    .parse_mode(ParseMode::Html)
                    .await?;
            }
        }

        Command::Model(model_id) => {
            if model_id.trim().is_empty() {
                bot.send_message(chat_id, "⚠️ Berikan nama model: <code>/model gemini-3.7-flash</code> atau <code>/model claude-3-7-sonnet</code>")
                    .parse_mode(ParseMode::Html)
                    .await?;
                return Ok(());
            }

            if let Err(e) = client.send_command(RpcCommand::SetModel { id: None, provider: None, model_id: model_id.clone() }).await {
                bot.send_message(chat_id, format!("❌ Gagal mengganti model: {:#}", e)).await?;
            } else {
                bot.send_message(chat_id, format!("🔄 <b>Model AI diubah ke:</b> <code>{}</code>", escape_html(&model_id)))
                    .parse_mode(ParseMode::Html)
                    .await?;
            }
        }

        Command::Thinking(level) => {
            let valid_levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
            let trimmed = level.trim().to_lowercase();
            if !valid_levels.contains(&trimmed.as_str()) {
                bot.send_message(chat_id, "⚠️ Level tidak valid. Pilihan: <code>off, minimal, low, medium, high, max</code>")
                    .parse_mode(ParseMode::Html)
                    .await?;
                return Ok(());
            }

            if let Err(e) = client.send_command(RpcCommand::SetThinkingLevel { id: None, level: trimmed.clone() }).await {
                bot.send_message(chat_id, format!("❌ Gagal mengatur level thinking: {:#}", e)).await?;
            } else {
                bot.send_message(chat_id, format!("🧠 <b>Level Thinking diatur ke:</b> <code>{}</code>", trimmed))
                    .parse_mode(ParseMode::Html)
                    .await?;
            }
        }

        Command::Compact => {
            let mut event_rx = client.subscribe();
            let sent_msg = bot.send_message(chat_id, "🧹 <i>Memicu peringkasan memori sesi (compaction)...</i>")
                .parse_mode(ParseMode::Html)
                .await?;

            if let Err(e) = client.send_command(RpcCommand::Compact { id: None, custom_instructions: None }).await {
                let _ = bot.edit_message_text(chat_id, sent_msg.id, format!("❌ Gagal menjalankan compaction: {:#}", e)).await;
                return Ok(());
            }

            let bot_clone = bot.clone();
            tokio::spawn(async move {
                let timeout_dur = Duration::from_secs(15);
                let start_time = tokio::time::Instant::now();
                while start_time.elapsed() < timeout_dur {
                    if let Ok(event) = event_rx.recv().await {
                        if let RpcEvent::Response { command, success, error, .. } = event {
                            if command.as_deref() == Some("compact") {
                                if success {
                                    let _ = bot_clone.edit_message_text(chat_id, sent_msg.id, "✅ <b>Compaction Berhasil!</b>\nMemori sesi telah diringkas untuk menghemat token.")
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                } else {
                                    let err_msg = error.unwrap_or_else(|| "Gagal compaction".to_string());
                                    let _ = bot_clone.edit_message_text(chat_id, sent_msg.id, format!("❌ <b>Compaction Gagal:</b> {}", escape_html(&err_msg)))
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                                return;
                            }
                        }
                    }
                }
            });
        }

        Command::Status => {
            let mut event_rx = client.subscribe();
            let sent_msg = bot.send_message(chat_id, "📊 <i>Mengambil snapshot status dari OMP Core Engine...</i>")
                .parse_mode(ParseMode::Html)
                .await?;

            if let Err(e) = client.send_command(RpcCommand::GetState { id: None }).await {
                let _ = bot.edit_message_text(chat_id, sent_msg.id, format!("❌ Gagal mengambil status: {:#}", e)).await;
                return Ok(());
            }

            let bot_clone = bot.clone();
            let workspace_path = config.project_workspace.clone();
            tokio::spawn(async move {
                let timeout_dur = Duration::from_secs(5);
                let start_time = tokio::time::Instant::now();
                while start_time.elapsed() < timeout_dur {
                    if let Ok(event) = event_rx.recv().await {
                        if let RpcEvent::Response { command, success, data, error, .. } = event {
                            if command.as_deref() == Some("get_state") {
                                if success {
                                    if let Some(d) = data {
                                        let card = format_state_card(&d, &workspace_path);
                                        let _ = bot_clone.edit_message_text(chat_id, sent_msg.id, card)
                                            .parse_mode(ParseMode::Html)
                                            .await;
                                    }
                                } else {
                                    let err_msg = error.unwrap_or_else(|| "Gagal mendapatkan status".to_string());
                                    let _ = bot_clone.edit_message_text(chat_id, sent_msg.id, format!("❌ Error: {}", escape_html(&err_msg)))
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                                return;
                            }
                        }
                    }
                }

                let _ = bot_clone.edit_message_text(chat_id, sent_msg.id, "⏱️ <i>Waktu tunggu status habis (timeout). OMP sedang sibuk.</i>")
                    .parse_mode(ParseMode::Html)
                    .await;
            });
        }
    }

    Ok(())
}

/// Format tampilan snapshot state OMP menjadi Telegram card yang rapi dan informatif.
fn format_state_card(data: &serde_json::Value, workspace: &std::path::Path) -> String {
    let provider = data.get("model")
        .and_then(|m| m.get("provider"))
        .and_then(|p| p.as_str())
        .unwrap_or("unknown");
    let model_id = data.get("model")
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .unwrap_or("unknown");

    let thinking = data.get("thinkingLevel")
        .and_then(|t| t.as_str())
        .unwrap_or("off");

    let is_streaming = data.get("isStreaming")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    let is_compacting = data.get("isCompacting")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);

    let message_count = data.get("messageCount")
        .and_then(|m| m.as_u64())
        .unwrap_or(0);

    let session_name = data.get("sessionName")
        .and_then(|s| s.as_str())
        .unwrap_or("-");

    let context_tokens = data.get("contextUsage")
        .and_then(|u| u.get("tokens"))
        .and_then(|t| t.as_u64());

    let max_tokens = data.get("contextUsage")
        .and_then(|u| u.get("maxTokens"))
        .and_then(|m| m.as_u64());

    let token_display = match (context_tokens, max_tokens) {
        (Some(tok), Some(max)) => {
            let pct = if max > 0 { (tok as f64 / max as f64) * 100.0 } else { 0.0 };
            format!("{} / {} tokens ({:.2}%)", tok, max, pct)
        }
        (Some(tok), None) => format!("{} tokens", tok),
        _ => "-".to_string(),
    };

    let status_str = if is_streaming {
        "⚡ <b>Sedang Berpikir / Streaming</b>"
    } else if is_compacting {
        "🧹 <b>Sedang Compacting Memori</b>"
    } else {
        "🟢 <b>Idle (Siap)</b>"
    };

    format!(
        "📊 <b>Status OMP Core Engine</b>\n\n\
        • <b>Status:</b> {}\n\
        • <b>Model Aktif:</b> <code>{}/{}</code>\n\
        • <b>Thinking Level:</b> <code>{}</code>\n\
        • <b>Sesi Aktif:</b> <code>{}</code> ({} pesan)\n\
        • <b>Konsumsi Token:</b> {}\n\
        • <b>Workspace:</b> <code>{}</code>",
        status_str,
        escape_html(provider),
        escape_html(model_id),
        escape_html(thinking),
        escape_html(session_name),
        message_count,
        token_display,
        escape_html(&workspace.to_string_lossy())
    )
}

pub async fn message_handler(
    bot: Bot,
    msg: Message,
    client: OmpClient,
    config: Arc<AppConfig>,
) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
    debug!("Menerima pesan non-command dari user_id: {}", user_id);

    if !is_authorized(&msg, &config) {
        warn!("Akses pesan ditolak untuk user ID: {}", user_id);
        bot.send_message(
            msg.chat.id,
            "⛔ <b>Akses Ditolak</b>\nID akun Anda tidak terdaftar dalam whitelist bot.",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let chat_id = msg.chat.id;
    if msg.photo().is_some() {
        let best_photo = match msg.photo().and_then(|photos| photos.last()) {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        bot.send_chat_action(chat_id, ChatAction::UploadPhoto).await?;

        let file = bot.get_file(&best_photo.file.id).await?;
        let mut buffer = Vec::new();

        if let Err(e) = bot.download_file(&file.path, &mut buffer).await {
            error!("Gagal mengunduh foto dari Telegram: {:#}", e);
            bot.send_message(chat_id, "❌ Gagal mengunduh gambar.").await?;
            return Ok(());
        }

        let b64_str = base64::engine::general_purpose::STANDARD.encode(&buffer);
        let image_content = ImageContent {
            url: format!("data:image/jpeg;base64,{}", b64_str),
        };

        let caption = msg.caption().unwrap_or("Jelaskan atau perbaiki kode pada gambar ini.").to_string();

        tokio::spawn(async move {
            let _ = execute_prompt_and_stream(bot, chat_id, caption, Some(vec![image_content]), client).await;
        });

        return Ok(());
    }

    // 2. Jika pesan teks biasa (Prompt langsung)
    if let Some(text) = msg.text() {
        let prompt_text = text.to_string();
        tokio::spawn(async move {
            let _ = execute_prompt_and_stream(bot, chat_id, prompt_text, None, client).await;
        });
    }

    Ok(())
}

/// Menjalankan prompt dan melakukan debounced live stream balasan ke Telegram.
async fn execute_prompt_and_stream(
    bot: Bot,
    chat_id: ChatId,
    prompt: String,
    images: Option<Vec<ImageContent>>,
    client: OmpClient,
) -> ResponseResult<()> {
    debug!("Memulai execute_prompt_and_stream untuk chat_id: {}", chat_id);
    let mut event_rx = client.subscribe();

    // Kirim prompt ke OMP RPC
    if let Err(e) = client.send_command(RpcCommand::Prompt {
        id: None,
        message: prompt,
        images,
    }).await {
        bot.send_message(chat_id, format!("❌ Gagal mengirim prompt ke OMP: {:#}", e)).await?;
        return Ok(());
    }

    // Kirim balon pesan awal untuk di-update selama streaming
    let sent_msg = bot.send_message(chat_id, "▌").await?;
    let message_id = sent_msg.id;
    debug!("Balon pesan awal dibuat dengan message_id: {}", message_id);

    // Jalankan background typing indicator loop
    let typing_bot = bot.clone();
    let (typing_stop_tx, mut typing_stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let mut interval_timer = interval(Duration::from_secs(4));
        loop {
            tokio::select! {
                _ = interval_timer.tick() => {
                    let _ = typing_bot.send_chat_action(chat_id, ChatAction::Typing).await;
                }
                _ = &mut typing_stop_rx => {
                    break;
                }
            }
        }
    });

    let mut accumulated_text = String::new();
    let mut current_tool_status = String::new();
    let mut last_rendered_text = String::new();
    let mut debounce_timer = interval(Duration::from_millis(1200));

    let mut is_turn_active = true;

    while is_turn_active {
        tokio::select! {
            _ = debounce_timer.tick() => {
                let display_text = format_live_text(&accumulated_text, &current_tool_status, true);
                if display_text != last_rendered_text && !display_text.trim().is_empty() {
                    last_rendered_text = display_text.clone();
                    update_message_safe(&bot, chat_id, message_id, &display_text).await;
                }
            }

            event_res = event_rx.recv() => {
                match event_res {
                    Ok(event) => {
                        debug!("Event OMP diterima di stream listener: '{}'", event.type_name());
                        match event {
                            RpcEvent::MessageUpdate { assistant_message_event } => {
                                if let Some(ev) = assistant_message_event {
                                    if ev.event_type == "text_delta" {
                                        if let Some(delta) = ev.delta {
                                            debug!("Menerima text_delta (panjang: {} chars)", delta.len());
                                            accumulated_text.push_str(&delta);
                                        }
                                    }
                                }
                            }
                            RpcEvent::ToolExecutionStart { tool_name, intent } => {
                                if let Some(tn) = tool_name {
                                    current_tool_status = format_tool_status(&tn, intent.as_deref());
                                }
                            }
                            RpcEvent::ToolExecutionEnd { .. } => {
                                current_tool_status.clear();
                            }
                            RpcEvent::Response { command, success, data, error, .. } => {
                                if let Some(cmd) = command {
                                    if cmd == "get_state" && success {
                                        if let Some(d) = data {
                                            let pretty_json = serde_json::to_string_pretty(&d).unwrap_or_default();
                                            let chunks = chunk_message(&pretty_json, 3800);
                                            for chunk in chunks {
                                                let msg = format!("<pre><code class=\"language-json\">{}</code></pre>", escape_html(&chunk));
                                                let _ = bot.send_message(chat_id, msg).parse_mode(ParseMode::Html).await;
                                            }
                                        }
                                    }
                                }
                                if !success {
                                    if let Some(err) = error {
                                        accumulated_text.push_str(&format!("\n\n⚠️ <i>Error: {}</i>", escape_html(&err)));
                                    }
                                }
                            }
                            RpcEvent::AgentEnd => {
                                debug!("Siklus agent_end tercapai. Menghentikan stream listener.");
                                is_turn_active = false;
                            }
                            _ => {}
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Event subscriber lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        is_turn_active = false;
                    }
                }
            }
        }
    }

    // Matikan background typing indicator
    let _ = typing_stop_tx.send(());

    // Finalisasi pesan (hapus kursor animasi)
    let final_display = format_live_text(&accumulated_text, "", false);
    if !final_display.trim().is_empty() {
        let chunks = chunk_message(&final_display, 4000);
        if let Some(first_chunk) = chunks.first() {
            update_message_safe(&bot, chat_id, message_id, first_chunk).await;
        }

        // Kirim sisa chunk jika teks melebihi 4000 karakter
        for extra_chunk in chunks.iter().skip(1) {
            let res = bot.send_message(chat_id, extra_chunk.clone())
                .parse_mode(ParseMode::Html)
                .await;
            if res.is_err() {
                let _ = bot.send_message(chat_id, extra_chunk.clone()).await;
            }
        }
    } else {
        update_message_safe(&bot, chat_id, message_id, "✅ Selesai.").await;
    }

    Ok(())
}

/// Format teks gabungan antara tool status dan teks stream yang dikonversi ke Telegram HTML.
fn format_live_text(accumulated: &str, tool_status: &str, with_cursor: bool) -> String {
    let mut out = String::new();
    if !tool_status.is_empty() {
        out.push_str(tool_status);
        out.push_str("\n\n");
    }

    if !accumulated.is_empty() {
        let parsed_html = markdown_to_telegram_html(accumulated);
        out.push_str(&parsed_html);
    }

    if with_cursor {
        out.push_str(" ▌");
    }
    out
}

/// Update pesan Telegram dengan HTML ParseMode dan safe fallback ke Plain Text jika parsing gagal.
async fn update_message_safe(bot: &Bot, chat_id: ChatId, message_id: MessageId, html_text: &str) {
    if html_text.trim().is_empty() {
        return;
    }

    debug!("Mencoba update pesan Telegram message_id: {} (panjang teks: {} chars)", message_id, html_text.len());
    // Coba edit dengan format HTML terlebih dahulu
    let res = bot.edit_message_text(chat_id, message_id, html_text)
        .parse_mode(ParseMode::Html)
        .await;
    if let Err(e) = res {
        let err_str = e.to_string();
        if !err_str.contains("message is not modified") {
            // Fallback ke Plain Text jika Telegram gagal mem-parse tag HTML
            let _ = bot.edit_message_text(chat_id, message_id, html_text).await;
        }
    }
}
