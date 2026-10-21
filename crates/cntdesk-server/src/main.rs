pub mod auth;
pub mod config;
pub mod db;
pub mod idgen;
pub mod relay;
pub mod signaling;

use tracing::info;
use tracing_subscriber::EnvFilter;

use cntdesk_core::transport::{TlsConfig, Transport};

use crate::auth::AuthService;
use crate::config::ServerConfig;
use crate::db::Database;
use crate::relay::RelayService;
use crate::signaling::SignalingService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install rustls CryptoProvider (ring) before any TLS usage
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    // Setup structured JSON logging
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!("CNT Desk Server starting");

    // Load configuration
    let config = ServerConfig::from_env()?;
    info!(listen_addr = %config.listen_addr, "Configuration loaded");

    // Open database
    let db = Database::open(&config.db_path, &config.db_key)?;
    info!("Database opened");

    // Create auth service
    let auth = AuthService::new(db);

    // Load TLS certificates
    let server_cert = std::fs::read(&config.tls_cert_path).unwrap_or_else(|_| {
        info!("TLS cert not found at {}, using placeholder for dev", config.tls_cert_path);
        vec![0u8; 32] // Placeholder — real deployment needs valid certs
    });
    let server_key = std::fs::read(&config.tls_key_path).unwrap_or_else(|_| {
        info!("TLS key not found at {}, using placeholder for dev", config.tls_key_path);
        vec![0u8; 32]
    });

    let tls_config = TlsConfig {
        server_cert,
        server_key,
        client_ca: None,
    };

    // Bind QUIC transport
    let transport = Transport::bind(&config.listen_addr, &tls_config)?;
    info!(addr = %config.listen_addr, "QUIC transport bound");

    // Generate or load server keypair for session token signing
    let server_keypair = cntdesk_core::crypto::KeyPair::generate();
    info!("Server keypair generated for session token signing");

    // Create signaling and relay services
    let signaling = SignalingService::new();
    let relay = RelayService::new(transport, auth, signaling, server_keypair);

    // Run relay (blocks until shutdown signal)
    relay.run().await?;

    info!("CNT Desk Server shut down cleanly");
    Ok(())
}