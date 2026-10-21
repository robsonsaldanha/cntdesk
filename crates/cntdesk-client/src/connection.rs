//! Client connection manager with retry logic and session management.

use std::time::Duration;

use anyhow::{Context, Result};
use cntdesk_core::crypto::KeyPair;
use cntdesk_core::protocol::Message;
use cntdesk_core::transport::{Connection, TlsConfig, Transport};
use tokio::time::sleep;

use crate::auth::ClientAuth;
use crate::commands::ConnectionState;

/// Typed wrapper for a viewer (peer) session.
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

/// Typed wrapper for a host session.
pub struct HostSession {
    conn: Connection,
}

impl HostSession {
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
    active_session: Option<PeerSession>,
}

impl ConnectionManager {
    /// Create a new ConnectionManager.
    /// Transport is initialized lazily on first connect to avoid startup crash
    /// from empty TLS config (client-only, no server cert needed at init).
    pub fn new(server_addr: &str, auth: ClientAuth) -> Result<Self> {
        let device_id = auth.load_device_id();
        Ok(Self {
            transport: None,
            auth,
            server_addr: server_addr.to_string(),
            device_id,
            state: ConnectionState::Disconnected,
            active_session: None,
        })
    }

    /// Create a ConnectionManager for testing without requiring valid TLS certificates.
    /// Transport is None; calling connect/accept methods will return an error.
    /// Suitable for state-tracking and device-ID persistence tests only.
    pub fn new_test(server_addr: &str, auth: ClientAuth) -> Self {
        let device_id = auth.load_device_id();
        Self {
            transport: None,
            auth,
            server_addr: server_addr.to_string(),
            device_id,
            state: ConnectionState::Disconnected,
            active_session: None,
        }
    }

    /// Get the persisted device ID, if any.
    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    /// Get the current connection state.
    pub fn state(&self) -> &ConnectionState {
        &self.state
    }

    /// Get mutable access to the active peer session, if connected.
    pub fn active_session(&mut self) -> Option<&mut PeerSession> {
        self.active_session.as_mut()
    }

    /// Register this client with the server, returning the assigned device ID.
    pub async fn register_with_server(&mut self) -> Result<String> {
        let keypair = self.auth.ensure_identity()?;
        let mut conn = self.connect_with_retry(&self.server_addr).await?;

        let msg = Message::AuthRequest {
            id: String::new(), // empty = new registration
            password_hash: keypair.public_key_bytes(),
        };
        conn.send(&msg).await.context("Failed to send registration request")?;

        match conn.recv().await.context("Failed to receive registration response")? {
            Message::AuthResponse { success: true, session_token: Some(token) } => {
                // Token contains the assigned ID as UTF-8 bytes for phase 0
                let id = String::from_utf8(token)
                    .context("Invalid ID in server response")?;
                // Persist device ID and update state
                self.auth.save_device_id(&id).context("Failed to persist device ID")?;
                self.device_id = Some(id.clone());
                Ok(id)
            }
            Message::AuthResponse { success: false, .. } => {
                anyhow::bail!("Server rejected registration")
            }
            other => {
                anyhow::bail!("Unexpected message during registration: {:?}", other)
            }
        }
    }

    /// Connect to a remote peer as a viewer.
    pub async fn connect_to_peer(&mut self, id: &str, password: &str) -> Result<()> {
        self.state = ConnectionState::Connecting;
        let mut conn = self.connect_with_retry(&self.server_addr).await?;

        let msg = Message::AuthRequest {
            id: id.to_string(),
            password_hash: password.as_bytes().to_vec(),
        };
        conn.send(&msg).await.context("Failed to send auth request")?;

        match conn.recv().await.context("Failed to receive auth response")? {
            Message::AuthResponse { success: true, .. } => {
                self.active_session = Some(PeerSession::new(conn));
                self.state = ConnectionState::Connected { remote_id: id.to_string() };
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

    /// Disconnect from the current peer session.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(mut session) = self.active_session.take() {
            let msg = Message::Disconnect { reason: "user_requested".to_string() };
            let _ = session.send(&msg).await;
        }
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    /// Start hosting a session (wait for incoming viewer connection).
    pub async fn host_session(&self) -> Result<HostSession> {
        let transport = self.transport.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Transport not available (test mode)"))?;
        let conn = transport.accept().await
            .context("Failed to accept incoming connection")?;
        Ok(HostSession::new(conn))
    }

    /// Run the host receive loop: injects incoming InputEvents into the local OS.
    pub async fn run_host_input_loop(session: &mut HostSession) -> Result<()> {
        loop {
            match session.recv().await {
                Ok(Message::InputEvent(event)) => {
                    if let Err(e) = crate::input_injection::inject(&event) {
                        tracing::warn!(error = %e, "Failed to inject input event");
                    }
                }
                Ok(Message::Disconnect { reason }) => {
                    tracing::info!(reason = %reason, "Viewer disconnected");
                    break;
                }
                Ok(other) => {
                    tracing::debug!(msg = ?other, "Non-input message received by host");
                }
                Err(e) => {
                    tracing::info!(error = %e, "Host connection closed");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Connect with exponential backoff retry (max 30s).
    async fn connect_with_retry(&self, addr: &str) -> Result<Connection> {
        let transport = self.transport.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Transport not available (test mode)"))?;
        let mut delay = Duration::from_millis(500);
        let max_delay = Duration::from_secs(30);
        let max_attempts = 10;

        // Phase 0: placeholder TLS config for outbound connections.
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