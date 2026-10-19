//! Tauri IPC commands for CNT Desk client.

use std::sync::Arc;

use cntdesk_core::protocol::InputEvent;
use tauri::State;
use tokio::sync::Mutex;

use crate::auth::ClientAuth;
use crate::connection::ConnectionManager;

/// Shared application state accessible from all IPC commands.
pub struct AppState {
    pub connection_manager: Arc<Mutex<ConnectionManager>>,
    pub client_auth: Arc<Mutex<ClientAuth>>,
}

/// Connection state exposed to frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { remote_id: String },
    Hosting { device_id: String },
}

#[tauri::command]
pub async fn get_device_id(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let _auth = state.client_auth.lock().await;
    // ClientAuth doesn't store device_id yet; return None until registration
    // Device ID comes from server via register_with_server()
    Ok(None)
}

#[tauri::command]
pub async fn set_password(
    _password: String,
    _state: State<'_, AppState>,
) -> Result<(), String> {
    // Phase 0: password management not yet implemented in ConnectionManager
    // Will be added when server-side password verification is wired
    Err("set_password not yet implemented".to_string())
}

#[tauri::command]
pub async fn generate_one_time_password(
    _state: State<'_, AppState>,
) -> Result<String, String> {
    // Phase 0: OTP generation not yet implemented in ConnectionManager
    Err("generate_one_time_password not yet implemented".to_string())
}

#[tauri::command]
pub async fn connect(
    remote_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cm = state.connection_manager.lock().await;
    // connect_to_peer returns PeerSession but we just verify it works
    let _session = cm
        .connect_to_peer(&remote_id, &password)
        .await
        .map_err(|e| e.to_string())?;
    // TODO: store session for later use (send_input, disconnect)
    Ok(())
}

#[tauri::command]
pub async fn disconnect(_state: State<'_, AppState>) -> Result<(), String> {
    // Phase 0: disconnect not yet implemented in ConnectionManager
    Err("disconnect not yet implemented".to_string())
}

#[tauri::command]
pub async fn send_input(
    _event: InputEvent,
    _state: State<'_, AppState>,
) -> Result<(), String> {
    // Phase 0: input forwarding not yet implemented
    Err("send_input not yet implemented".to_string())
}

#[tauri::command]
pub async fn get_connection_state(
    _state: State<'_, AppState>,
) -> Result<ConnectionState, String> {
    // Phase 0: no connection state tracking yet
    Ok(ConnectionState::Disconnected)
}