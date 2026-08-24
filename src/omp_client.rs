//! Manajer child process OMP RPC over asynchronous stdio streams (JSON-RPC).

use crate::types::{AppConfig, RpcCommand, RpcEvent};
use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc};

/// Handle untuk berinteraksi dengan OMP RPC dari handler Telegram.
#[derive(Clone)]
pub struct OmpClient {
    command_tx: mpsc::Sender<RpcCommand>,
    event_tx: broadcast::Sender<RpcEvent>,
    is_ready: Arc<AtomicBool>,
}

impl OmpClient {
    /// Membuat instance OMP client baru dan memulai background worker loop.
    pub fn start(config: AppConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel::<RpcCommand>(100);
        let (event_tx, _) = broadcast::channel::<RpcEvent>(500);
        let is_ready = Arc::new(AtomicBool::new(false));

        let client = Self {
            command_tx,
            event_tx: event_tx.clone(),
            is_ready: is_ready.clone(),
        };

        // Jalankan supervisor loop di background task
        tokio::spawn(async move {
            run_omp_supervisor(config, command_rx, event_tx, is_ready).await;
        });

        client
    }

    /// Mengirimkan perintah JSON-RPC ke OMP Core Engine.
    pub async fn send_command(&self, cmd: RpcCommand) -> Result<()> {
        debug!("Memasukkan RpcCommand '{}' ke antrean pengirim mpsc...", cmd.type_name());
        self.command_tx
            .send(cmd)
            .await
            .map_err(|_| anyhow::anyhow!("Gagal mengirim perintah ke channel OMP (channel closed)"))
    }

    /// Berlangganan event stream keluaran dari OMP stdout.
    pub fn subscribe(&self) -> broadcast::Receiver<RpcEvent> {
        self.event_tx.subscribe()
    }

    /// Memeriksa apakah handshake `ready` dari OMP telah diterima.
    pub fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::SeqCst)
    }
}

/// Supervisor loop yang mengelola siklus hidup child process OMP dan auto-respawn jika crash.
async fn run_omp_supervisor(
    config: AppConfig,
    mut command_rx: mpsc::Receiver<RpcCommand>,
    event_tx: broadcast::Sender<RpcEvent>,
    is_ready: Arc<AtomicBool>,
) {
    loop {
        info!(
            "Memulai proses OMP RPC di workspace: {:?}",
            config.project_workspace
        );

        is_ready.store(false, Ordering::SeqCst);

        let mut child = match spawn_omp_process(&config) {
            Ok(c) => c,
            Err(e) => {
                error!("Gagal men-spawn subprocess OMP: {:#}. Mencoba lagi dalam 5 detik...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let stdin = match child.stdin.take() {
            Some(s) => s,
            None => {
                error!("Tidak dapat mengambil stdin child process OMP.");
                let _ = child.kill().await;
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                error!("Tidak dapat mengambil stdout child process OMP.");
                let _ = child.kill().await;
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        };

        // Task untuk menulis perintah ke stdin OMP
        let mut stdin_writer = stdin;
        let is_ready_clone = is_ready.clone();
        let event_tx_clone = event_tx.clone();

        let mut stdout_reader = BufReader::new(stdout).lines();

        let mut process_active = true;

        while process_active {
            tokio::select! {
                // Menerima baris output dari stdout OMP
                line_result = stdout_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<RpcEvent>(trimmed) {
                                Ok(event) => {
                                    debug!("Event OMP diterima dari stdout: '{}'", event.type_name());
                                    if let RpcEvent::Ready { .. } = &event {
                                        info!("Handshake OMP RPC Ready diterima!");
                                        is_ready_clone.store(true, Ordering::SeqCst);
                                    }
                                    let _ = event_tx_clone.send(event);
                                }
                                Err(err) => {
                                    // Abaikan log non-JSON biasa atau parse sebagai pesan log
                                    warn!("Baris non-JSON RPC dari stdout OMP: {} (err: {})", trimmed, err);
                                }
                            }
                        }
                        Ok(None) => {
                            warn!("Subprocess OMP stdout EOF terdeteksi (proses tertutup).");
                            process_active = false;
                        }
                        Err(e) => {
                            error!("Error membaca stdout OMP: {:#}", e);
                            process_active = false;
                        }
                    }
                }

                // Menerima perintah dari Telegram handler untuk diteruskan ke stdin OMP
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(command) => {
                            debug!("Meneruskan RpcCommand '{}' ke stdin OMP...", command.type_name());
                            match serde_json::to_string(&command) {
                                Ok(mut json_str) => {
                                    json_str.push('\n');
                                    if let Err(e) = stdin_writer.write_all(json_str.as_bytes()).await {
                                        error!("Gagal menulis ke stdin OMP: {:#}", e);
                                        process_active = false;
                                    }
                                    let _ = stdin_writer.flush().await;
                                    debug!("Berhasil flush RpcCommand ke stdin OMP.");
                                }
                                Err(e) => {
                                    error!("Gagal men-serialize RpcCommand ke JSON: {:#}", e);
                                }
                            }
                        }
                        None => {
                            info!("Command channel ditutup. Menghentikan OMP supervisor loop.");
                            let _ = child.kill().await;
                            return;
                        }
                    }
                }
            }
        }

        // Pastikan child process dimatikan sebelum respawn
        is_ready.store(false, Ordering::SeqCst);
        let _ = child.kill().await;
        warn!("Subprocess OMP terhenti. Men-respawn dalam 2 detik...");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

/// Men-spawn subprocess OMP CLI dengan mode RPC persisten.
fn spawn_omp_process(config: &AppConfig) -> Result<Child> {
    debug!("Menjalankan spawn process: '{} --mode rpc' di direktori {:?}", config.omp_bin_path, config.project_workspace);
    let mut cmd = Command::new(&config.omp_bin_path);
    cmd.arg("--mode")
        .arg("rpc")
        .current_dir(&config.project_workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let child = cmd
        .spawn()
        .with_context(|| format!("Gagal menjalankan binary: '{}'", config.omp_bin_path))?;

    Ok(child)
}
