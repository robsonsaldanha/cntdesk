//! Client connection manager with retry logic and session management.
//!
//! Spawns background data-plane tasks after successful auth:
//! - Host: capture → encode → send FrameMetadata + FrameData
//! - Viewer: recv FrameData → push to FRAME_CHANNEL for frontend rendering

use std::time::Duration;

use anyhow::{Context, Result};
use cntdesk_core::protocol::Message;
use cntdesk_core::transport::{Connection, TlsConfig, Transport};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::auth::ClientAuth;
use crate::commands::ConnectionState;

/// Global frame channel: viewer receive loop → frontend renderer.
/// Bounded at 8 frames to apply backpressure on slow renders.
static FRAME_CHANNEL: once_cell::sync::Lazy<(
    mpsc::Sender<FrameEvent>,
    parking_lot::Mutex<Option<mpsc::Receiver<FrameEvent>>>,
)> = once_cell::sync::Lazy::new(|| {
    let (tx, rx) = mpsc::channel(8);
    (tx, parking_lot::Mutex::new(Some(rx)))
});

/// Take the receiver end of the frame channel (once only).
pub fn take_frame_receiver() -> Option<mpsc::Receiver<FrameEvent>> {
    FRAME_CHANNEL.1.lock().take()
}

/// A decoded/raw frame ready for display.
#[derive(Debug, Clone)]
pub struct FrameEvent {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub is_keyframe: bool,
}

/// Typed wrapper for a QUIC session (used by both host and viewer).
pub struct PeerSession {
    conn: Connection,
}

impl PeerSession {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn send(&mut self, msg: &Message) -> Result<()> {
        self.conn.send(msg).await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    pub async fn recv(&mut self) -> Result<Message> {
        self.conn.recv().await.map_err(|e| anyhow::anyhow!("{}", e))
    }
}

/// Manages client connections to the CNT Desk server and peers.
pub struct ConnectionManager {
    transport: Option<Transport>,
    auth: ClientAuth,
    server_addr: String,
    device_id: Option<String>,
    state: ConnectionState,
    /// Active QUIC connection — taken (moved) into background task on spawn.
    active_conn: Option<Connection>,
    /// Handle to cancel background data-plane tasks on disconnect.
    cancel_token: Option<tokio_util::sync::CancellationToken>,
}

impl ConnectionManager {
    pub fn new(server_addr: &str, auth: ClientAuth) -> Result<Self> {
        let device_id = auth.load_device_id();
        Ok(Self {
            transport: None,
            auth,
            server_addr: server_addr.to_string(),
            device_id,
            state: ConnectionState::Disconnected,
            active_conn: None,
            cancel_token: None,
        })
    }

    pub fn new_test(server_addr: &str, auth: ClientAuth) -> Self {
        let device_id = auth.load_device_id();
        Self {
            transport: None,
            auth,
            server_addr: server_addr.to_string(),
            device_id,
            state: ConnectionState::Disconnected,
            active_conn: None,
            cancel_token: None,
        }
    }

    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    pub fn state(&self) -> &ConnectionState {
        &self.state
    }

    /// Send an input event through the active connection (viewer → host).
    pub async fn send_input(&mut self, event: cntdesk_core::protocol::InputEvent) -> Result<()> {
        let conn = self.active_conn.as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected: no active session"))?;
        conn.send(&Message::InputEvent(event)).await
            .map_err(|e| anyhow::anyhow!("Failed to send input: {}", e))
    }

    /// Connect to server: empty id → host registration; non-empty → viewer connection.
    pub async fn connect_to_peer(&mut self, id: &str, password: &str) -> Result<()> {
        // Cancel any previous data-plane tasks
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        self.state = ConnectionState::Connecting;
        let addr = self.server_addr.clone();
        let mut conn = self.connect_with_retry(&addr).await?;

        if id.is_empty() {
            // ── Host registration ──────────────────────────────────────
            let keypair = self.auth.ensure_identity()?;
            let my_id = self.device_id.clone().unwrap_or_default();
            conn.send(&Message::AuthRequest {
                id: my_id,
                password_hash: keypair.public_key_bytes(),
            }).await.context("Failed to send host registration")?;

            match conn.recv().await.context("Failed to receive auth response")? {
                Message::AuthResponse { success: true, .. } => {
                    let device_id = self.device_id.clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    self.state = ConnectionState::Hosting { device_id };
                    tracing::info!("Registered as host, spawning capture loop");

                    // Move connection into background capture task
                    let cancel = tokio_util::sync::CancellationToken::new();
                    self.cancel_token = Some(cancel.clone());
                    self.active_conn = None; // moved into task
                    spawn_host_capture_loop(conn, cancel);
                    Ok(())
                }
                Message::AuthResponse { success: false, .. } => {
                    self.state = ConnectionState::Disconnected;
                    anyhow::bail!("Server rejected host registration")
                }
                other => {
                    self.state = ConnectionState::Disconnected;
                    anyhow::bail!("Unexpected message during host registration: {:?}", other)
                }
            }
        } else {
            // ── Viewer connection ──────────────────────────────────────
            conn.send(&Message::AuthRequest {
                id: id.to_string(),
                password_hash: password.as_bytes().to_vec(),
            }).await.context("Failed to send auth request")?;

            match conn.recv().await.context("Failed to receive auth response")? {
                Message::AuthResponse { success: true, .. } => {
                    self.state = ConnectionState::Connected { remote_id: id.to_string() };
                    tracing::info!(remote_id = %id, "Connected as viewer, spawning receive loop");

                    // Keep connection in active_conn for send_input() AND
                    // spawn a separate receive task that reads from it.
                    // Problem: Connection isn't Clone. Solution: move conn
                    // into receive task, use a separate channel for outbound
                    // input events. For now, keep conn for input sending
                    // and spawn receive on a second connection... but that
                    // requires two QUIC streams.
                    //
                    // Better approach: move conn into a combined task that
                    // both receives frames AND accepts input via mpsc channel.
                    let cancel = tokio_util::sync::CancellationToken::new();
                    self.cancel_token = Some(cancel.clone());
                    self.active_conn = None; // moved into task
                    spawn_viewer_combined_loop(conn, cancel);
                    Ok(())
                }
                Message::AuthResponse { success: false, .. } => {
                    self.state = ConnectionState::Disconnected;
                    anyhow::bail!("Authentication failed for peer {}", id)
                }
                other => {
                    self.state = ConnectionState::Disconnected;
                    anyhow::bail!("Unexpected message during peer auth: {:?}", other)
                }
            }
        }
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
        if let Some(mut conn) = self.active_conn.take() {
            let msg = Message::Disconnect { reason: "user_requested".to_string() };
            let _ = conn.send(&msg).await;
        }
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn ensure_transport(&mut self) -> Result<()> {
        if self.transport.is_none() {
            let transport = Transport::bind_client("0.0.0.0:0")
                .context("Failed to create QUIC transport endpoint")?;
            self.transport = Some(transport);
            tracing::info!("QUIC transport endpoint initialized for client");
        }
        Ok(())
    }

    async fn connect_with_retry(&mut self, addr: &str) -> Result<Connection> {
        self.ensure_transport()?;
        let transport = self.transport.as_ref().unwrap();
        let mut delay = Duration::from_millis(500);
        let max_delay = Duration::from_secs(30);
        let max_attempts = 10;

        let tls_config = TlsConfig {
            server_cert: vec![],
            server_key: vec![],
            client_ca: None,
        };

        for attempt in 1..=max_attempts {
            match transport.connect(addr, &tls_config).await {
                Ok(conn) => return Ok(conn),
                Err(e) => {
                    tracing::warn!(
                        "Connection attempt {}/{} to {} failed: {}. Retrying in {:?}",
                        attempt, max_attempts, addr, e, delay
                    );
                    if attempt == max_attempts {
                        anyhow::bail!(
                            "Failed to connect to {} after {} attempts: {}",
                            addr, max_attempts, e
                        );
                    }
                    sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
        }
        unreachable!()
    }
}

// ─── Background data-plane tasks ────────────────────────────────────────────

/// Host: capture screen → encode → send FrameMetadata + FrameData over QUIC.
fn spawn_host_capture_loop(
    mut conn: Connection,
    cancel: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        tracing::info!("Host capture loop started");

        let mut capture = match cntdesk_capture::capture::create_backend() {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create capture backend");
                return;
            }
        };

        let config = cntdesk_capture::capture::CaptureConfig::default();
        if let Err(e) = capture.start(&config) {
            tracing::error!(error = %e, "Failed to start capture");
            return;
        }

        let encoder_config = cntdesk_core::encoder::EncoderConfig {
            width: config.max_width,
            height: config.max_height,
            fps: config.target_fps,
            bitrate_kbps: 2000,
            keyframe_interval_ms: 2000,
        };

        let mut encoder = match cntdesk_core::encoder::create_encoder(&encoder_config) {
            Ok(enc) => enc,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create encoder");
                return;
            }
        };

        let frame_interval = Duration::from_millis(1000 / config.target_fps as u64);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Host capture loop cancelled");
                    break;
                }
                _ = tokio::time::sleep(frame_interval) => {
                    match capture.next_frame() {
                        Ok(Some(frame)) => {
                            let width = frame.width;
                            let height = frame.height;

                            match encoder.encode(&frame) {
                                Ok(packet) => {
                                    let meta = Message::FrameMetadata {
                                        seq: packet.seq,
                                        timestamp_us: packet.timestamp_us,
                                        width: width as u16,
                                        height: height as u16,
                                        keyframe: packet.is_keyframe,
                                    };
                                    if let Err(e) = conn.send(&meta).await {
                                        tracing::warn!(error = %e, "Failed to send frame metadata");
                                        break;
                                    }

                                    let data_msg = Message::FrameData {
                                        seq: packet.seq,
                                        data: packet.data,
                                        is_keyframe: packet.is_keyframe,
                                    };
                                    if let Err(e) = conn.send(&data_msg).await {
                                        tracing::warn!(error = %e, "Failed to send frame data");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "Encode failed");
                                }
                            }
                        }
                        Ok(None) => {} // screen unchanged
                        Err(e) => {
                            tracing::warn!(error = %e, "Capture error");
                        }
                    }
                }
            }
        }

        let _ = capture.stop();
        tracing::info!("Host capture loop stopped");
    });
}

/// Viewer: receive frames from QUIC → push to FRAME_CHANNEL; also handle
/// input injection messages from host (for when this node is acting as host
/// receiving input from viewers — future use).
fn spawn_viewer_combined_loop(
    mut conn: Connection,
    cancel: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        tracing::info!("Viewer receive loop started");
        let tx = &FRAME_CHANNEL.0;
        let mut pending_meta: Option<(u16, u16, bool)> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Viewer receive loop cancelled");
                    break;
                }
                result = conn.recv() => {
                    match result {
                        Ok(Message::FrameMetadata { width, height, keyframe, .. }) => {
                            pending_meta = Some((width, height, keyframe));
                        }
                        Ok(Message::FrameData { data, is_keyframe, .. }) => {
                            let (width, height, _) = pending_meta
                                .unwrap_or((1920, 1080, is_keyframe));
                            // Push to channel; drop frame if frontend is too slow
                            let _ = tx.try_send(FrameEvent {
                                data,
                                width: width as u32,
                                height: height as u32,
                                is_keyframe,
                            });
                        }
                        Ok(Message::InputEvent(event)) => {
                            // Host receiving input from viewer — inject locally
                            if let Err(e) = crate::input_injection::inject(&event) {
                                tracing::warn!(error = %e, "Failed to inject input");
                            }
                        }
                        Ok(Message::Disconnect { reason }) => {
                            tracing::info!(reason = %reason, "Remote disconnected");
                            break;
                        }
                        Ok(_) => {} // ignore other messages
                        Err(e) => {
                            tracing::warn!(error = %e, "Viewer receive error");
                            break;
                        }
                    }
                }
            }
        }
        tracing::info!("Viewer receive loop stopped");
    });
}