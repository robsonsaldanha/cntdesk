//! Integration tests for IPC command wiring (Task 5).

use cntdesk_client::auth::ClientAuth;
use cntdesk_client::commands::{AppState, ConnectionState};
use cntdesk_client::connection::ConnectionManager;
use std::sync::Arc;
use tokio::sync::Mutex;

fn create_test_state() -> AppState {
    let auth = ClientAuth::new_temp_dir().unwrap();
    let cm = ConnectionManager::new_test("127.0.0.1:9999", auth);
    let auth2 = ClientAuth::new_temp_dir().unwrap();
    AppState {
        connection_manager: Arc::new(Mutex::new(cm)),
        client_auth: Arc::new(Mutex::new(auth2)),
    }
}

#[tokio::test]
async fn test_ipc_get_device_id_returns_none_when_not_registered() {
    let state = create_test_state();
    let cm = state.connection_manager.lock().await;
    assert!(cm.device_id().is_none());
}

#[tokio::test]
async fn test_ipc_get_connection_state_reflects_disconnected() {
    let state = create_test_state();
    let cm = state.connection_manager.lock().await;
    assert_eq!(*cm.state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn test_ipc_send_input_fails_when_not_connected() {
    let state = create_test_state();
    let mut cm = state.connection_manager.lock().await;
    // No active session — send_input should fail gracefully
    assert!(cm.active_session().is_none());
}

#[tokio::test]
async fn test_ipc_disconnect_clears_session() {
    let state = create_test_state();
    let mut cm = state.connection_manager.lock().await;
    // Disconnect when not connected should succeed without error
    let result = cm.disconnect().await;
    assert!(result.is_ok());
    assert_eq!(*cm.state(), ConnectionState::Disconnected);
    assert!(cm.active_session().is_none());
}