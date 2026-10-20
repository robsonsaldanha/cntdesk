//! Signaling service for managing active sessions and message routing.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use cntdesk_core::protocol::Message;

/// Session expiration time: 24 hours in seconds.
const SESSION_TTL_SECS: u64 = 86400;

/// State for an active signaling session.
struct SessionState {
    peer_a: String,
    peer_b: String,
    pending_messages: VecDeque<Message>,
    created_at: u64,
    /// Bandwidth tracking: bytes transferred in this session.
    bytes_transferred: u64,
}

/// Signaling service managing active sessions behind RwLock.
pub struct SignalingService {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    /// Peers waiting to be paired (device_id -> timestamp registered).
    waiting_peers: Arc<RwLock<HashMap<String, u64>>>,
}

impl SignalingService {
    /// Create a new signaling service.
    pub fn new() -> Self {
        let svc = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            waiting_peers: Arc::new(RwLock::new(HashMap::new())),
        };
        // Spawn background cleanup task for expired sessions
        let sessions = svc.sessions.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                Self::cleanup_expired_sessions(&sessions).await;
            }
        });
        svc
    }

    /// Create a test instance without background cleanup.
    pub fn new_test() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            waiting_peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a peer as waiting to be paired.
    pub async fn register_waiting_peer(&self, device_id: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut peers = self.waiting_peers.write().await;
        peers.insert(device_id.to_string(), now);
        info!(device_id = %device_id, "Peer registered as waiting");
    }

    /// Check if a peer is currently waiting to be paired.
    pub async fn has_waiting_peer(&self, device_id: &str) -> bool {
        let peers = self.waiting_peers.read().await;
        peers.contains_key(device_id)
    }

    /// Try to pair a newly arrived peer with an existing waiting peer.
    /// Returns Some(session_id) if pairing succeeded, None if no waiting peer available.
    /// On success, removes the waiting peer and creates a session.
    pub async fn pair_with_waiting(&self, new_peer_id: &str) -> Option<String> {
        let mut peers = self.waiting_peers.write().await;
        // Find first waiting peer that isn't ourselves
        let waiting_id = peers.keys()
            .find(|id| id.as_str() != new_peer_id)
            .cloned()?;
        peers.remove(&waiting_id);
        drop(peers);

        // Create session between the two peers
        match self.create_session(&waiting_id, new_peer_id).await {
            Ok(session_id) => {
                info!(session_id = %session_id, "Peers paired successfully");
                Some(session_id)
            }
            Err(e) => {
                warn!(error = %e, "Failed to create session during pairing");
                None
            }
        }
    }

    /// Create a new session between two peers. Returns session ID.
    pub async fn create_session(&self, peer_a: &str, peer_b: &str) -> Result<String, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let session_id = format!("{}-{}", peer_a, peer_b);

        let state = SessionState {
            peer_a: peer_a.to_string(),
            peer_b: peer_b.to_string(),
            pending_messages: VecDeque::new(),
            created_at: now,
            bytes_transferred: 0,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), state);

        info!(session_id = %session_id, peer_a = %peer_a, peer_b = %peer_b, "Session created");
        Ok(session_id)
    }

    /// Route a message to a session's pending queue.
    pub async fn route_message(&self, session_id: &str, msg: Message) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            warn!(session_id = %session_id, "Route message failed: session not found");
            format!("Session '{}' not found", session_id)
        })?;

        // Track bandwidth (approximate via serialized size)
        let msg_size = match &msg {
            Message::IceCandidate { candidate } => candidate.len() as u64,
            Message::Disconnect { reason } => reason.len() as u64,
            _ => 64, // approximate for other message types
        };
        state.bytes_transferred += msg_size;

        state.pending_messages.push_back(msg);
        debug!(session_id = %session_id, "Message routed");
        Ok(())
    }

    /// Get and drain pending messages for a session.
    pub async fn get_pending_messages(&self, session_id: &str) -> Vec<Message> {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            state.pending_messages.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    /// Close a session.
    pub async fn close_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            info!(session_id = %session_id, "Session closed");
        }
    }

    /// Get bandwidth stats for a session.
    pub async fn get_bandwidth(&self, session_id: &str) -> Option<u64> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).map(|s| s.bytes_transferred)
    }

    /// Remove sessions older than SESSION_TTL_SECS.
    async fn cleanup_expired_sessions(sessions: &Arc<RwLock<HashMap<String, SessionState>>>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut lock = sessions.write().await;
        let expired: Vec<String> = lock
            .iter()
            .filter(|(_, state)| now.saturating_sub(state.created_at) > SESSION_TTL_SECS)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired {
            lock.remove(id);
            info!(session_id = %id, "Expired session cleaned up");
        }
    }
}