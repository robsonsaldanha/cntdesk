//! Relay service wrapping QUIC transport endpoint.
//! Accepts connections, authenticates via AuthService, registers in signaling,
//! spawns bidirectional forward tasks.

use std::sync::Arc;

use tokio::signal;
use tracing::{error, info, warn};

use cntdesk_core::protocol::Message;
use cntdesk_core::transport::{Connection, Transport};

use crate::auth::AuthService;
use crate::signaling::SignalingService;

/// Relay service managing incoming QUIC connections.
pub struct RelayService {
    transport: Arc<Transport>,
    auth: Arc<AuthService>,
    signaling: Arc<SignalingService>,
}

impl RelayService {
    /// Create a new relay service.
    pub fn new(
        transport: Transport,
        auth: AuthService,
        signaling: SignalingService,
    ) -> Self {
        Self {
            transport: Arc::new(transport),
            auth: Arc::new(auth),
            signaling: Arc::new(signaling),
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
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(conn, &auth, &signaling).await {
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

        // Authentication successful — send response
        let resp = Message::AuthResponse {
            success: true,
            session_token: Some(vec![0u8; 32]), // Placeholder token for phase 0
        };
        conn.send(&resp)
            .await
            .map_err(|e| format!("send auth response failed: {}", e))?;

        info!(device_id = %device_id, "Device authenticated");

        // Enter relay forwarding loop
        Self::forward_loop(conn, &device_id, signaling).await
    }

    /// Forward messages bidirectionally through signaling or direct relay.
    async fn forward_loop(
        mut conn: Connection,
        device_id: &str,
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
                        Message::IceCandidate { .. } => {
                            // Route through signaling for ICE exchange
                            // In phase 0, we just log it
                            info!(device_id = %device_id, "ICE candidate received");
                        }
                        _ => {
                            // Forward other messages (frame data, input events)
                            // In full implementation, these go to the paired peer
                        }
                    }

                    // Check for pending outbound messages from signaling
                    let pending = signaling.get_pending_messages(device_id).await;
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