//! cntdesk-client - CNT Desk remote desktop client application

pub mod auth;
pub mod commands;
pub mod connection;
pub mod ipc;

use tracing_subscriber::{fmt, EnvFilter};

fn main() {
    // Initialize tracing
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    tracing::info!("Starting CNT Desk client");

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("cntdesk");

    // Create separate ClientAuth instances since ConnectionManager takes ownership
    // and AppIpc needs its own for IPC commands like get_device_id
    let auth_for_ipc = auth::ClientAuth::new(config_dir.clone())
        .expect("Failed to initialize client auth for IPC");
    let auth_for_conn = auth::ClientAuth::new(config_dir)
        .expect("Failed to initialize client auth for connection");

    let server_addr = std::env::var("CNTDESK_SERVER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:4433".to_string());

    let connection_manager = connection::ConnectionManager::new(&server_addr, auth_for_conn)
        .expect("Failed to initialize connection manager");

    let app_ipc = ipc::AppIpc::new(connection_manager, auth_for_ipc);

    let builder = tauri::Builder::default();
    let builder = ipc::register_ipc(builder, app_ipc);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}