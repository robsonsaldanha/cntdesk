//! Tauri IPC bridge — wraps backend services for frontend access.

use std::sync::Arc;

use tauri::{Builder, Runtime};
use tokio::sync::Mutex;

use crate::auth::ClientAuth;
use crate::commands::AppState;
use crate::connection::ConnectionManager;

/// Application IPC state holding shared backend services.
pub struct AppIpc {
    pub connection_manager: Arc<Mutex<ConnectionManager>>,
    pub client_auth: Arc<Mutex<ClientAuth>>,
}

impl AppIpc {
    /// Create new IPC state from backend services.
    pub fn new(
        connection_manager: ConnectionManager,
        client_auth: ClientAuth,
    ) -> Self {
        Self {
            connection_manager: Arc::new(Mutex::new(connection_manager)),
            client_auth: Arc::new(Mutex::new(client_auth)),
        }
    }

    /// Convert to Tauri-managed AppState for command handlers.
    pub fn into_app_state(self) -> AppState {
        AppState {
            connection_manager: self.connection_manager,
            client_auth: self.client_auth,
        }
    }
}

/// Register all IPC commands and state with the Tauri builder.
pub fn register_ipc<R: Runtime>(builder: Builder<R>, ipc: AppIpc) -> Builder<R> {
    let app_state = ipc.into_app_state();
    builder
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_device_id,
            crate::commands::set_password,
            crate::commands::generate_one_time_password,
            crate::commands::connect,
            crate::commands::disconnect,
            crate::commands::send_input,
            crate::commands::get_connection_state,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                tracing::info!("Window close requested, initiating graceful shutdown");
                api.prevent_close();
                let _ = window.close();
            }
        })
}