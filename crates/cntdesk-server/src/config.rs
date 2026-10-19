use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub db_path: String,
    pub db_key: String,
    pub listen_addr: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
}

impl ServerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            db_path: env::var("CNTDESK_DB_PATH").unwrap_or_else(|_| "data/cntdesk.db".into()),
            db_key: env::var("CNTDESK_DB_KEY").unwrap_or_else(|_| "dev-key-change-in-prod".into()),
            listen_addr: env::var("CNTDESK_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:4433".into()),
            tls_cert_path: env::var("CNTDESK_TLS_CERT").unwrap_or_else(|_| "certs/server.crt".into()),
            tls_key_path: env::var("CNTDESK_TLS_KEY").unwrap_or_else(|_| "certs/server.key".into()),
        })
    }
}