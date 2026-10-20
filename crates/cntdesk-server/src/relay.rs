//! Relay service wrapping QUIC transport endpoint.
//! Accepts connections, authenticates via AuthService, registers in signaling,
//! spawns bidirectional forward tasks.

use std::sync::Arc;

use tokio::signal;
use tracing::{error, info, warn};

use cntdesk_core::crypto::KeyPair;
use cntdesk_core::protocol::Message;
use cntdesk_core::transport::{Connection, Transport};

use crate::auth::AuthService;
use crate::signaling::SignalingService;

/// Relay service managing incoming QUIC connections.
pub struct RelayService {
    transport: Arc<Transport>,
    auth: Arc<AuthService>,
    signaling: Arc<SignalingService>,
    server_keypair: Arc<KeyPair>,
}

impl RelayService {
    /// Create a new relay service.
    pub fn new(
        transport: Transport,
        auth: AuthService,
        signaling: SignalingService,
        server_keypair: KeyPair,
    ) -> Self {
        Self {
            transport: Arc::new(transport),
            auth: Arc::new(auth),
            signaling: Arc::new(signaling),
            server_keypair: Arc::new(server_keypair),
        }
    }

    /// Run the relay accept loop with graceful shutdown on SIGTERM/SIGINT.
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Relay service starting");

        let shutdown = async {
            let ctrl_c = signal::ctrl_c();
            #[cfg(unix)]
            {
                let mut sigterm =
                    signal::unix::signal(signal::unix::SignalKind::terminate())
                        .expect("failed to register SIGTERM handler");
                tokio::select! {
                    _ = ctrl_c => {},
                    _ = sigterm.recv() => {},
                }
            }
            #[cfg(not(unix))]
            {
                ctrl_c.await.ok();
            }
        };

        let accept_loop = async {
            loop {
                match self.transport.accept().await {
                    Ok(conn) => {
                        let auth = self.auth.clone();
                        let signaling = self.signaling.clone();
                        let server_kp = self.server_keypair.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(conn, &auth, &signaling, &server_kp).await {
                                warn!(error = %e, "Connection handling failed");
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "Accept failed");
                        // Brief backoff on repeated accept failures
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                }
            }
        };

        tokio::select! {
            _ = shutdown => {
                info!("Shutdown signal received, stopping relay service");
            }
            _ = accept_loop => {
                warn!("Accept loop exited unexpectedly");
            }
        }

        info!("Relay service stopped");
        Ok(())
    }

    /// Handle a single incoming connection: authenticate, register, forward.
    async fn handle_connection(
        mut conn: Connection,
        auth: &AuthService,
        signaling: &SignalingService,
        server_keypair: &KeyPair,
    ) -> Result<(), String> {
        // Wait for AuthRequest
        let msg = conn.recv().await.map_err(|e| format!("recv auth request failed: {}", e))?;

        let (device_id, password_hash) = match msg {
            Message::AuthRequest { id, password_hash } => (id, password_hash),
            other => {
                return Err(format!("Expected AuthRequest, got {:?}", other));
            }
        };

        // Verify device exists
        if !auth.verify_device_exists(&device_id) {
            let resp = Message::AuthResponse {
                success: false,
                session_token: None,
            };
            let _ = conn.send(&resp).await;
            return Err(format!("Device '{}' not found", device_id));
        }

        // Verify password
        match auth.verify_password(&device_id, &password_hash) {
            Ok(true) => {}
            Ok(false) => {
                let resp = Message::AuthResponse {
                    success: false,
                    session_token: None,
                };
                let _ = conn.send(&resp).await;
                return Err(format!("Password verification failed for '{}'", device_id));
            }
            Err(e) => {
                let resp = Message::AuthResponse {
                    success: false,
                    session_token: None,
                };
                let _ = conn.send(&resp).await;
                return Err(format!("Password check error for '{}': {}", device_id, e));
            }
        }

        // Authentication successful — generate real session token
        let session_token = auth.create_session_token(&device_id, server_keypair)
            .map_err(|e| format!("Failed to create session token: {}", e))?;

        let resp = Message::AuthResponse {
            success: true,
            session_token: Some(session_token),
        };
        conn.send(&resp)
            .await
            .map_err(|e| format!("send auth response failed: {}", e))?;

        info!(device_id = %device_id, "Device authenticated with real session token");

        // Try to pair with a waiting peer; if none, register as waiting
        let session_id = if let Some(sid) = signaling.pair_with_waiting(&device_id).await {
            info!(device_id = %device_id, session_id = %sid, "Paired with waiting peer");
            sid
        } else {
            signaling.register_waiting_peer(&device_id).await;
            info!(device_id = %device_id, "Registered as waiting peer");
            // Use device_id as placeholder session until paired
            device_id.clone()
        };

        // Enter relay forwarding loop
        Self::forward_loop(conn, &device_id, &session_id, signaling).await
    }

    /// Forward messages bidirectionally through signaling.
    async fn forward_loop(
        mut conn: Connection,
        device_id: &str,
        session_id: &str,
        signaling: &SignalingService,
    ) -> Result<(), String> {
        loop {
            match conn.recv().await {
                Ok(msg) => {
                    match &msg {
                        Message::Disconnect { reason } => {
                            info!(device_id = %device_id, reason = %reason, "Peer disconnected");
                            break;
                        }
                        Message::FrameData { .. } | Message::InputEvent(_) | Message::IceCandidate { .. } => {
                            // Route frame data, input events, and ICE candidates to paired peer
                            if let Err(e) = signaling.route_message(session_id, msg.clone()).await {
                                warn!(device_id = %device_id, error = %e, "Failed to route message");
                            }
                        }
                        _ => {
                            // Other messages logged but not forwarded
                            info!(device_id = %device_id, "Non-forwardable message received");
                        }
                    }

                    // Check for pending outbound messages from signaling
                    let pending = signaling.get_pending_messages(session_id).await;
                    for out_msg in pending {
                        if let Err(e) = conn.send(&out_msg).await {
                            warn!(device_id = %device_id, error = %e, "Failed to send pending message");
                            break;
                        }
                    }
                }
                Err(e) => {
                    info!(device_id = %device_id, error = %e, "Connection closed");
                    break;
                }
            }
        }
        Ok(())
    }
}