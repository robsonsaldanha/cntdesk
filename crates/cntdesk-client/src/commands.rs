//! Tauri IPC commands for CNT Desk client.

use std::sync::Arc;

use cntdesk_core::protocol::{InputEvent, Message};
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
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { remote_id: String },
    Hosting { device_id: String },
}

#[tauri::command]
pub async fn get_device_id(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let cm = state.connection_manager.lock().await;
    Ok(cm.device_id().map(|s| s.to_string()))
}

#[tauri::command]
pub async fn set_password(
    _password: String,
    _state: State<'_, AppState>,
) -> Result<(), String> {
    // TODO(phase2): requires active server connection to send SetPassword RPC
    Err("set_password requires active server connection (phase 2)".to_string())
}

#[tauri::command]
pub async fn generate_one_time_password(
    _state: State<'_, AppState>,
) -> Result<String, String> {
    // TODO(phase2): requires active server connection to request OTP generation
    Err("generate_one_time_password requires active server connection (phase 2)".to_string())
}

#[tauri::command]
pub async fn connect(
    remote_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cm = state.connection_manager.lock().await;
    cm.connect_to_peer(&remote_id, &password)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    let mut cm = state.connection_manager.lock().await;
    cm.disconnect()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn send_input(
    event: InputEvent,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cm = state.connection_manager.lock().await;
    let session = cm.active_session()
        .ok_or_else(|| "Not connected: no active session".to_string())?;
    session.send(&Message::InputEvent(event))
        .await
        .map_err(|e| format!("Failed to send input event: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn get_connection_state(
    state: State<'_, AppState>,
) -> Result<ConnectionState, String> {
    let cm = state.connection_manager.lock().await;
    Ok(cm.state().clone())
}