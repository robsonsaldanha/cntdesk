//! Signaling service for managing active sessions and message routing.
//!
//! Supports multi-viewer fan-out: one host can serve multiple concurrent viewers.
//! Messages from the host are broadcast to all viewers in the session;
//! messages from any viewer are forwarded only to the host.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use cntdesk_core::protocol::Message;

/// Session expiration time: 24 hours in seconds.
const SESSION_TTL_SECS: u64 = 86400;

/// Role of a peer within a session.
#[derive(Debug, Clone, PartialEq)]
pub enum PeerRole {
    Host,
    Viewer,
}

/// State for an active signaling session supporting multi-viewer fan-out.
#[derive(Clone)]
struct SessionState {
    /// The host device ID (exactly one per session).
    host_id: String,
    /// Set of viewer device IDs currently connected.
    viewers: HashSet<String>,
    /// Per-peer pending outbound message queues.
    pending_messages: HashMap<String, VecDeque<Message>>,
    created_at: u64,
    /// Bandwidth tracking: bytes transferred in this session.
    bytes_transferred: u64,
}

/// Signaling service managing active sessions behind RwLock.
pub struct SignalingService {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    /// Peers waiting to be paired as hosts (device_id -> timestamp registered).
    waiting_hosts: Arc<RwLock<HashMap<String, u64>>>,
    /// Map from device_id to the session_id they belong to (for quick lookup).
    peer_to_session: Arc<RwLock<HashMap<String, String>>>,
}

impl SignalingService {
    /// Create a new signaling service.
    pub fn new() -> Self {
        let svc = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            waiting_hosts: Arc::new(RwLock::new(HashMap::new())),
            peer_to_session: Arc::new(RwLock::new(HashMap::new())),
        };
        // Spawn background cleanup task for expired sessions
        let sessions = svc.sessions.clone();
        let peer_map = svc.peer_to_session.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                Self::cleanup_expired_sessions(&sessions, &peer_map).await;
            }
        });
        svc
    }

    /// Create a test instance without background cleanup.
    pub fn new_test() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            waiting_hosts: Arc::new(RwLock::new(HashMap::new())),
            peer_to_session: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a host as waiting to be paired with viewers.
    pub async fn register_waiting_host(&self, device_id: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut hosts = self.waiting_hosts.write().await;
        hosts.insert(device_id.to_string(), now);
        info!(device_id = %device_id, "Host registered as waiting");
    }

    /// Backwards-compatible alias for register_waiting_host.
    pub async fn register_waiting_peer(&self, device_id: &str) {
        self.register_waiting_host(device_id).await;
    }

    /// Check if a host is currently waiting.
    pub async fn has_waiting_host(&self, device_id: &str) -> bool {
        let hosts = self.waiting_hosts.read().await;
        hosts.contains_key(device_id)
    }

    /// Backwards-compatible alias.
    pub async fn has_waiting_peer(&self, device_id: &str) -> bool {
        self.has_waiting_host(device_id).await
    }

    /// Try to pair a newly arrived viewer with an existing waiting host.
    /// Returns Some(session_id) if pairing succeeded, None if no waiting host available.
    pub async fn pair_viewer_with_host(&self, viewer_id: &str) -> Option<String> {
        let hosts = self.waiting_hosts.write().await;
        // Find first waiting host that isn't ourselves
        let host_id = hosts.keys()
            .find(|id| id.as_str() != viewer_id)
            .cloned()?;
        // Don't remove the host — it stays registered so more viewers can join
        drop(hosts);

        // Check if a session already exists for this host
        let peer_map = self.peer_to_session.read().await;
        if let Some(existing_session) = peer_map.get(&host_id) {
            let session_id = existing_session.clone();
            drop(peer_map);
            // Add viewer to existing session
            let mut sessions = self.sessions.write().await;
            if let Some(state) = sessions.get_mut(&session_id) {
                state.viewers.insert(viewer_id.to_string());
                state.pending_messages.insert(viewer_id.to_string(), VecDeque::new());
                info!(session_id = %session_id, viewer = %viewer_id, "Viewer added to existing session");
            }
            // Update peer-to-session map
            drop(sessions);
            let mut peer_map = self.peer_to_session.write().await;
            peer_map.insert(viewer_id.to_string(), session_id.clone());
            return Some(session_id);
        }
        drop(peer_map);

        // Create new session between host and first viewer
        match self.create_session_multi(&host_id, &[viewer_id]).await {
            Ok(session_id) => {
                info!(session_id = %session_id, "Host paired with first viewer");
                Some(session_id)
            }
            Err(e) => {
                warn!(error = %e, "Failed to create session during pairing");
                None
            }
        }
    }

    /// Backwards-compatible alias for pair_viewer_with_host.
    pub async fn pair_with_waiting(&self, new_peer_id: &str) -> Option<String> {
        self.pair_viewer_with_host(new_peer_id).await
    }

    /// Create a new multi-viewer session. Returns session ID.
    pub async fn create_session_multi(&self, host_id: &str, viewer_ids: &[&str]) -> Result<String, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let session_id = format!("session-{}", host_id);

        let mut pending_messages = HashMap::new();
        pending_messages.insert(host_id.to_string(), VecDeque::new());

        let mut viewers = HashSet::new();
        for vid in viewer_ids {
            viewers.insert(vid.to_string());
            pending_messages.insert(vid.to_string(), VecDeque::new());
        }

        let state = SessionState {
            host_id: host_id.to_string(),
            viewers,
            pending_messages,
            created_at: now,
            bytes_transferred: 0,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), state);
        drop(sessions);

        // Update peer-to-session map
        let mut peer_map = self.peer_to_session.write().await;
        peer_map.insert(host_id.to_string(), session_id.clone());
        for vid in viewer_ids {
            peer_map.insert(vid.to_string(), session_id.clone());
        }

        info!(session_id = %session_id, host = %host_id, viewers = ?viewer_ids, "Multi-viewer session created");
        Ok(session_id)
    }

    /// Backwards-compatible: create a session between two peers.
    pub async fn create_session(&self, peer_a: &str, peer_b: &str) -> Result<String, String> {
        // Treat peer_a as host, peer_b as viewer for backwards compat
        self.create_session_multi(peer_a, &[peer_b]).await
    }

    /// Route a message with fan-out semantics.
    ///
    /// If `sender_id` is the host, the message is queued for ALL viewers (fan-out).
    /// If `sender_id` is a viewer, the message is queued only for the host.
    pub async fn route_message_from(&self, session_id: &str, sender_id: &str, msg: Message) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            warn!(session_id = %session_id, "Route message failed: session not found");
            format!("Session '{}' not found", session_id)
        })?;

        // Track bandwidth
        let msg_size = match &msg {
            Message::IceCandidate { candidate } => candidate.len() as u64,
            Message::Disconnect { reason } => reason.len() as u64,
            _ => 64,
        };
        state.bytes_transferred += msg_size;

        if sender_id == state.host_id {
            // Host → fan-out to all viewers
            let viewer_ids: Vec<String> = state.viewers.iter().cloned().collect();
            for vid in viewer_ids {
                if let Some(queue) = state.pending_messages.get_mut(&vid) {
                    queue.push_back(msg.clone());
                }
            }
            debug!(session_id = %session_id, sender = %sender_id, viewers = state.viewers.len(), "Host message fanned out to viewers");
        } else if state.viewers.contains(sender_id) {
            // Viewer → host only
            if let Some(queue) = state.pending_messages.get_mut(&state.host_id) {
                queue.push_back(msg);
            }
            debug!(session_id = %session_id, sender = %sender_id, "Viewer message routed to host");
        } else {
            return Err(format!("Sender '{}' not part of session '{}'", sender_id, session_id));
        }

        Ok(())
    }

    /// Backwards-compatible route_message (routes to generic pending queue).
    pub async fn route_message(&self, session_id: &str, msg: Message) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        let state = sessions.get_mut(session_id).ok_or_else(|| {
            warn!(session_id = %session_id, "Route message failed: session not found");
            format!("Session '{}' not found", session_id)
        })?;

        let msg_size = match &msg {
            Message::IceCandidate { candidate } => candidate.len() as u64,
            Message::Disconnect { reason } => reason.len() as u64,
            _ => 64,
        };
        state.bytes_transferred += msg_size;

        // Legacy path: push to all pending queues
        for queue in state.pending_messages.values_mut() {
            queue.push_back(msg.clone());
        }
        debug!(session_id = %session_id, "Message routed (legacy)");
        Ok(())
    }

    /// Get and drain pending messages for a specific peer in a session.
    pub async fn get_pending_messages_for(&self, session_id: &str, peer_id: &str) -> Vec<Message> {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            if let Some(queue) = state.pending_messages.get_mut(peer_id) {
                return queue.drain(..).collect();
            }
        }
        Vec::new()
    }

    /// Backwards-compatible: get and drain all pending messages for a session.
    pub async fn get_pending_messages(&self, session_id: &str) -> Vec<Message> {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(session_id) {
            // Drain all queues into one vec (legacy behavior)
            let mut all = Vec::new();
            for queue in state.pending_messages.values_mut() {
                all.extend(queue.drain(..));
            }
            all
        } else {
            Vec::new()
        }
    }

    /// Remove a peer from its session. If the host leaves, close the entire session.
    /// If a viewer leaves, just remove it from the viewer set.
    pub async fn remove_peer(&self, device_id: &str) {
        let session_id = {
            let mut peer_map = self.peer_to_session.write().await;
            peer_map.remove(device_id)
        };

        if let Some(sid) = session_id {
            let mut sessions = self.sessions.write().await;
            if let Some(state) = sessions.get_mut(&sid) {
                if state.host_id == device_id {
                    // Host left — close entire session
                    info!(session_id = %sid, host = %device_id, "Host left, closing session");
                    sessions.remove(&sid);
                } else {
                    state.viewers.remove(device_id);
                    state.pending_messages.remove(device_id);
                    info!(session_id = %sid, viewer = %device_id, remaining_viewers = state.viewers.len(), "Viewer removed from session");
                }
            }
        }
    }

    /// Close a session entirely.
    pub async fn close_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.remove(session_id) {
            // Clean up peer-to-session map
            drop(sessions);
            let mut peer_map = self.peer_to_session.write().await;
            peer_map.remove(&state.host_id);
            for vid in &state.viewers {
                peer_map.remove(vid);
            }
            info!(session_id = %session_id, "Session closed");
        }
    }

    /// Get bandwidth stats for a session.
    pub async fn get_bandwidth(&self, session_id: &str) -> Option<u64> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).map(|s| s.bytes_transferred)
    }

    /// Get the number of active viewers in a session.
    pub async fn get_viewer_count(&self, session_id: &str) -> Option<usize> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).map(|s| s.viewers.len())
    }

    /// Look up the session_id for a given device.
    pub async fn get_session_for_peer(&self, device_id: &str) -> Option<String> {
        let peer_map = self.peer_to_session.read().await;
        peer_map.get(device_id).cloned()
    }

    /// Remove sessions older than SESSION_TTL_SECS.
    async fn cleanup_expired_sessions(
        sessions: &Arc<RwLock<HashMap<String, SessionState>>>,
        peer_map: &Arc<RwLock<HashMap<String, String>>>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut lock = sessions.write().await;
        let expired: Vec<(String, SessionState)> = lock
            .iter()
            .filter(|(_, state)| now.saturating_sub(state.created_at) > SESSION_TTL_SECS)
            .map(|(id, state)| (id.clone(), state.clone()))
            .collect();

        let mut pm = peer_map.write().await;
        for (id, state) in &expired {
            lock.remove(id);
            pm.remove(&state.host_id);
            for vid in &state.viewers {
                pm.remove(vid);
            }
            info!(session_id = %id, "Expired session cleaned up");
        }
    }
}