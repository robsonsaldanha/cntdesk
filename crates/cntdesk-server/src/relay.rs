//! Relay service: orchestrates QUIC transport, auth, and signaling for multi-viewer sessions.
//!
//! One host can serve multiple concurrent viewers. Host frames are fanned out
//! to all viewers; viewer input events are routed back to the host.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use cntdesk_core::crypto::KeyPair;
use cntdesk_core::protocol::Message;
use cntdesk_core::transport::{Transport};
use crate::auth::AuthService;
use crate::signaling::SignalingService;

/// Active peer connections tracked by device_id.
#[derive(Clone)]
struct PeerConnections {
    /// Map from device_id to their active connection state.
    connections: Arc<RwLock<HashMap<String, PeerState>>>,
}

struct PeerState {
    /// The QUIC connection endpoint for this peer.
    transport_addr: String,
    /// Session ID this peer belongs to.
    session_id: Option<String>,
}

impl PeerConnections {
    fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn register(&self, device_id: &str, addr: &str) {
        let mut conns = self.connections.write().await;
        conns.insert(
            device_id.to_string(),
            PeerState {
                transport_addr: addr.to_string(),
                session_id: None,
            },
        );
        info!(device_id = %device_id, addr = %addr, "Peer registered");
    }

    async fn set_session(&self, device_id: &str, session_id: &str) {
        let mut conns = self.connections.write().await;
        if let Some(state) = conns.get_mut(device_id) {
            state.session_id = Some(session_id.to_string());
        }
    }

    #[allow(dead_code)]
    async fn get_session(&self, device_id: &str) -> Option<String> {
        let conns = self.connections.read().await;
        conns.get(device_id).and_then(|s| s.session_id.clone())
    }

    async fn unregister(&self, device_id: &str) {
        let mut conns = self.connections.write().await;
        conns.remove(device_id);
        info!(device_id = %device_id, "Peer unregistered");
    }
}

/// Relay service managing the main event loop.
pub struct RelayService {
    transport: Transport,
    auth: AuthService,
    signaling: SignalingService,
    #[allow(dead_code)]
    server_keypair: KeyPair,
    peers: PeerConnections,
}

impl RelayService {
    pub fn new(
        transport: Transport,
        auth: AuthService,
        signaling: SignalingService,
        server_keypair: KeyPair,
    ) -> Self {
        Self {
            transport,
            auth,
            signaling,
            server_keypair,
            peers: PeerConnections::new(),
        }
    }

    /// Run the relay main loop: accept incoming QUIC connections and route messages.
    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Relay service started, accepting connections...");
        loop {
            match self.transport.accept().await {
                Ok(conn) => {
                    // Spawn handler for each incoming connection
                    let auth = self.auth.clone();
                    let signaling = self.signaling.clone();
                    let peers = self.peers.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_peer_connection(conn, auth, signaling, peers).await {
                            error!(error = %e, "Peer connection handler failed");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept QUIC connection");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

async fn handle_peer_connection(
    mut conn: cntdesk_core::transport::Connection,
    auth: AuthService,
    signaling: SignalingService,
    peers: PeerConnections,
) -> Result<(), String> {
    // First message should be AuthRequest
    let first_msg = conn.recv().await.map_err(|e| format!("Recv failed: {}", e))?;
    let (device_id, _session_token) = match first_msg {
        Message::AuthRequest { id, password_hash } => {
            // The `id` field carries either:
            //   - The client's own device ID (when registering as host, or first-time empty)
            //   - The target host's device ID (when connecting as viewer)
            // The `password_hash` field carries either:
            //   - The client's public key bytes (32 bytes, when registering)
            //   - The password string bytes (when connecting as viewer)
            //
            // Heuristic: if password_hash is exactly 32 bytes → it's a public key → host registration
            //            otherwise → it's a password → viewer attempting to connect to host `id`

            let is_host_registration = password_hash.len() == 32;

            if is_host_registration {
                // Host registration flow
                let my_id = if id.is_empty() {
                    // First-time: generate new ID
                    let new_id = auth.register_new_device(&password_hash)?;
                    new_id
                } else {
                    // Returning host: register with provided ID if not exists
                    if !auth.verify_device_exists(&id) {
                        auth.register_device_with_id(&id, &password_hash)?;
                    }
                    id
                };

                // Generate session token
                let token = auth.generate_session_token(&my_id)?;

                // Send auth response
                let response = Message::AuthResponse {
                    success: true,
                    session_token: Some(token.clone()),
                };
                conn.send(&response).await.map_err(|e| format!("Send failed: {}", e))?;

                (my_id, token)
            } else {
                // Viewer connection flow: `id` is the target host, `password_hash` is the password
                let target_host_id = id;
                let password = String::from_utf8_lossy(&password_hash).to_string();

                // Verify the target host exists and password matches
                match auth.verify_password(&target_host_id, password.as_bytes()) {
                    Ok(true) => {
                        // Generate a temporary device ID for this viewer session
                        let viewer_id = auth.register_new_device(&[0u8; 32])?;
                        let token = auth.generate_session_token(&viewer_id)?;

                        let response = Message::AuthResponse {
                            success: true,
                            session_token: Some(token.clone()),
                        };
                        conn.send(&response).await.map_err(|e| format!("Send failed: {}", e))?;

                        // Return viewer_id as device_id, and encode target_host in token for later use
                        // We'll use a special format: viewer_id + "|" + target_host_id
                        let combined_id = format!("{}|{}", viewer_id, target_host_id);
                        (combined_id, token)
                    }
                    Ok(false) => {
                        let response = Message::AuthResponse {
                            success: false,
                            session_token: None,
                        };
                        conn.send(&response).await.map_err(|e| format!("Send failed: {}", e))?;
                        return Err("Wrong password".to_string());
                    }
                    Err(e) => {
                        let response = Message::AuthResponse {
                            success: false,
                            session_token: None,
                        };
                        conn.send(&response).await.map_err(|e| format!("Send failed: {}", e))?;
                        return Err(format!("Auth error: {}", e));
                    }
                }
            }
        }
        _ => return Err("First message must be AuthRequest".to_string()),
    };

    // Register peer
    peers.register(&device_id, "quic").await;

    // Determine role based on device_id format
    if device_id.contains('|') {
        // Viewer: device_id format is "viewer_id|target_host_id"
        let parts: Vec<&str> = device_id.splitn(2, '|').collect();
        let viewer_id = parts[0].to_string();
        let target_host_id = parts[1].to_string();

        info!(viewer_id = %viewer_id, target = %target_host_id, "Viewer attempting to connect to host");

        if let Some(session_id) = signaling.pair_viewer_with_specific_host(&viewer_id, &target_host_id).await {
            peers.set_session(&device_id, &session_id).await;
            info!(viewer_id = %viewer_id, session_id = %session_id, "Viewer paired with host");
            run_viewer_loop(conn, viewer_id, session_id, signaling.clone()).await?;
        } else {
            return Err(format!("Host '{}' not available", target_host_id));
        }
    } else {
        // Host: register as waiting and run host loop
        signaling.register_waiting_host(&device_id).await;
        info!(device_id = %device_id, "Host registered and waiting for viewers");
        run_host_loop(conn, device_id.clone(), signaling.clone()).await?;
    }

    peers.unregister(&device_id).await;
    Ok(())
}

async fn run_host_loop(
    mut conn: cntdesk_core::transport::Connection,
    host_id: String,
    signaling: SignalingService,
) -> Result<(), String> {
    info!(host_id = %host_id, "Running host loop");
    loop {
        match conn.recv().await {
            Ok(Message::Disconnect { reason }) => {
                info!(reason = %reason, "Host disconnecting");
                break;
            }
            Ok(msg) => {
                // Get session for this host
                if let Some(session_id) = signaling.get_session_for_peer(&host_id).await {
                    // Fan-out message to all viewers in session
                    signaling.route_message_from(&session_id, &host_id, msg).await?;
                }
            }
            Err(e) => {
                warn!(error = %e, host_id = %host_id, "Host connection error");
                break;
            }
        }
    }
    Ok(())
}

async fn run_viewer_loop(
    mut conn: cntdesk_core::transport::Connection,
    viewer_id: String,
    session_id: String,
    signaling: SignalingService,
) -> Result<(), String> {
    info!(viewer_id = %viewer_id, session_id = %session_id, "Running viewer loop");
    loop {
        // Check for pending messages from host
        let pending = signaling.get_pending_messages_for(&session_id, &viewer_id).await;
        for msg in pending {
            conn.send(&msg).await.map_err(|e| format!("Send failed: {}", e))?;
        }

        // Receive input events from viewer UI
        match tokio::time::timeout(Duration::from_millis(50), conn.recv()).await {
            Ok(Ok(Message::InputEvent(event))) => {
                // Route input event to host
                signaling.route_message_from(&session_id, &viewer_id, Message::InputEvent(event)).await?;
            }
            Ok(Ok(Message::Disconnect { reason })) => {
                info!(reason = %reason, "Viewer disconnecting");
                break;
            }
            Ok(Ok(_)) => {
                // Ignore non-input messages from viewer
            }
            Ok(Err(e)) => {
                warn!(error = %e, viewer_id = %viewer_id, "Viewer connection error");
                break;
            }
            Err(_) => {
                // Timeout, continue loop
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Remove viewer from session
    signaling.remove_peer(&viewer_id).await;
    Ok(())
}