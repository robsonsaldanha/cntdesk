//! Tauri IPC bridge — wraps backend services for frontend access.

use std::sync::Arc;

use tauri::{Builder, Emitter, Runtime};
use tokio::sync::Mutex;

use crate::auth::ClientAuth;
use crate::commands::AppState;
use crate::connection::{self, ConnectionManager};

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

    // Spawn frame-forwarding task: reads from FRAME_CHANNEL and emits Tauri events
    let _cm_for_frames = app_state.connection_manager.clone();
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
        .setup(move |app| {
            let handle = app.handle().clone();
            tokio::spawn(async move {
                // Take the receiver (only one consumer allowed)
                let mut rx = match connection::take_frame_receiver() {
                    Some(rx) => rx,
                    None => {
                        tracing::warn!("Frame receiver already taken; frame forwarding disabled");
                        return;
                    }
                };

                tracing::info!("Frame forwarding task started");
                while let Some(frame) = rx.recv().await {
                    // Emit as Tauri event "cntdesk-frame" with serializable payload
                    let payload = serde_json::json!({
                        "data": frame.data,
                        "width": frame.width,
                        "height": frame.height,
                        "isKeyframe": frame.is_keyframe,
                    });
                    if let Err(e) = handle.emit("cntdesk-frame", &payload) {
                        tracing::warn!(error = %e, "Failed to emit frame event");
                    }
                }
                tracing::info!("Frame forwarding task ended");
            });
            Ok(())
        })
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                tracing::info!("Window close requested, exiting application");
                std::process::exit(0);
            }
        })
}