//! QUIC transport layer for CNT Desk.
//!
//! Provides `Transport` (endpoint management) and `Connection` (bidirectional
//! message stream) using quinn with TLS 1.3. Length-prefixed framing:
//! u32 big-endian prefix + bincode payload.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use quinn::{Endpoint, RecvStream, SendStream};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::error::CoreError;
use crate::protocol::Message;

/// Certificate verifier that accepts any server certificate (for self-signed dev certs).
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Connection establishment timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// TLS configuration for QUIC endpoints.
#[derive(Clone)]
pub struct TlsConfig {
    pub server_cert: Vec<u8>,
    pub server_key: Vec<u8>,
    pub client_ca: Option<Vec<u8>>,
}

/// QUIC transport endpoint.
pub struct Transport {
    endpoint: Endpoint,
}

/// Bidirectional QUIC connection with message framing.
pub struct Connection {
    send: SendStream,
    recv: RecvStream,
}

fn make_transport_config(idle_secs: u64) -> Arc<quinn::TransportConfig> {
    let mut cfg = quinn::TransportConfig::default();
    cfg.max_idle_timeout(Some(Duration::from_secs(idle_secs).try_into().unwrap()));
    Arc::new(cfg)
}

impl Transport {
    /// Bind a QUIC endpoint to the given address with TLS configuration.
    pub fn bind(addr: &str, tls_config: &TlsConfig) -> Result<Self, CoreError> {
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| CoreError::Protocol(format!("invalid address {}: {}", addr, e)))?;

        // Parse PEM-encoded cert and key to DER format for rustls
        let cert_pem = std::str::from_utf8(&tls_config.server_cert)
            .map_err(|e| CoreError::Crypto(format!("cert not valid UTF-8: {}", e)))?;
        let certs: Vec<rustls::pki_types::CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| CoreError::Crypto(format!("failed to parse cert PEM: {}", e)))?;
        let cert = certs.into_iter().next()
            .ok_or_else(|| CoreError::Crypto("no certificate found in PEM".into()))?;

        let key_pem = std::str::from_utf8(&tls_config.server_key)
            .map_err(|e| CoreError::Crypto(format!("key not valid UTF-8: {}", e)))?;
        let key = rustls_pemfile::private_key(&mut key_pem.as_bytes())
            .map_err(|e| CoreError::Crypto(format!("failed to parse key PEM: {}", e)))?
            .ok_or_else(|| CoreError::Crypto("no private key found in PEM".into()))?;

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .map_err(|e| CoreError::Crypto(format!("TLS server config error: {}", e)))?;
        server_crypto.alpn_protocols = vec![b"cntdesk".to_vec()];

        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| CoreError::Crypto(format!("QUIC server config error: {}", e)))?,
        ));
        server_config.transport_config(make_transport_config(60));

        let endpoint = Endpoint::server(server_config, socket_addr)
            .map_err(|e| CoreError::Protocol(format!("bind failed: {}", e)))?;

        Ok(Self { endpoint })
    }

    /// Bind a client-only QUIC endpoint (no server TLS config needed).
    pub fn bind_client(addr: &str) -> Result<Self, CoreError> {
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| CoreError::Protocol(format!("invalid address {}: {}", addr, e)))?;

        let endpoint = Endpoint::client(socket_addr)
            .map_err(|e| CoreError::Protocol(format!("client endpoint bind failed: {}", e)))?;

        Ok(Self { endpoint })
    }

    /// Connect to a remote QUIC endpoint.
    pub async fn connect(
        &self,
        addr: &str,
        _tls_config: &TlsConfig,
    ) -> Result<Connection, CoreError> {
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| CoreError::Protocol(format!("invalid address {}: {}", addr, e)))?;

        // Accept any server certificate (self-signed) for dev/testing
        let mut client_crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();
        client_crypto.alpn_protocols = vec![b"cntdesk".to_vec()];

        let mut client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| CoreError::Crypto(format!("QUIC client config error: {}", e)))?,
        ));
        client_config.transport_config(make_transport_config(30));

        let connecting = self
            .endpoint
            .connect_with(client_config, socket_addr, "localhost")
            .map_err(|e| CoreError::Protocol(format!("connect initiation failed: {}", e)))?;

        match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
            Ok(Ok(conn)) => {
                let (send, recv) = conn.open_bi().await.map_err(|e| {
                    CoreError::Protocol(format!("open bidirectional stream failed: {}", e))
                })?;
                Ok(Connection { send, recv })
            }
            Ok(Err(e)) => {
                warn!("QUIC connection failed: {}, TCP fallback not implemented in phase 0", e);
                Err(CoreError::Protocol(format!(
                    "QUIC connection failed (TCP fallback deferred): {}",
                    e
                )))
            }
            Err(_) => {
                warn!(
                    "QUIC connection timed out after {:?}, TCP fallback not implemented in phase 0",
                    CONNECT_TIMEOUT
                );
                Err(CoreError::Protocol(format!(
                    "connection timed out after {:?} (TCP fallback deferred)",
                    CONNECT_TIMEOUT
                )))
            }
        }
    }

    /// Accept an incoming QUIC connection.
    pub async fn accept(&self) -> Result<Connection, CoreError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| CoreError::Protocol("endpoint closed".into()))?;

        let conn = incoming
            .await
            .map_err(|e| CoreError::Protocol(format!("accept failed: {}", e)))?;

        let (send, recv) = conn.accept_bi().await.map_err(|e| {
            CoreError::Protocol(format!("accept bidirectional stream failed: {}", e))
        })?;

        Ok(Connection { send, recv })
    }

    /// Create a loopback pair for testing with self-signed certificates.
    ///
    /// Uses a single endpoint for both server and client (matching quinn's own
    /// test patterns). Returns (server_connection, client_connection).
    pub async fn loopback_pair() -> Result<(Connection, Connection), CoreError> {
        use rcgen::generate_simple_self_signed;

        // Ensure rustls crypto provider is installed
        let _ = rustls::crypto::ring::default_provider().install_default();

        let certified_key = generate_simple_self_signed(vec!["localhost".into()])
            .map_err(|e| CoreError::Crypto(format!("self-signed cert generation failed: {}", e)))?;

        let cert_der = certified_key.cert.der().to_vec();
        let key_der = certified_key.key_pair.serialize_der();

        // Build server TLS config
        let cert = rustls::pki_types::CertificateDer::from(cert_der.clone());
        let key = rustls::pki_types::PrivateKeyDer::try_from(key_der)
            .map_err(|e| CoreError::Crypto(format!("invalid key: {}", e)))?;

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key)
            .map_err(|e| CoreError::Crypto(format!("TLS server config error: {}", e)))?;
        server_crypto.alpn_protocols = vec![b"cntdesk".to_vec()];

        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| CoreError::Crypto(format!("QUIC server config error: {}", e)))?,
        ));
        server_config.transport_config(make_transport_config(60));

        // Single endpoint acts as both server and client (quinn pattern)
        let endpoint = Endpoint::server(
            server_config,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        )
        .map_err(|e| CoreError::Protocol(format!("endpoint bind failed: {}", e)))?;

        let addr = endpoint
            .local_addr()
            .map_err(|e| CoreError::Protocol(format!("local addr failed: {}", e)))?;

        // Build client config from same cert
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(cert)
            .map_err(|e| CoreError::Crypto(format!("failed to add cert: {}", e)))?;

        let mut client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_crypto.alpn_protocols = vec![b"cntdesk".to_vec()];

        let mut client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| CoreError::Crypto(format!("QUIC client config error: {}", e)))?,
        ));
        client_config.transport_config(make_transport_config(30));

        // Spawn server accept task (matches quinn's own test pattern exactly)
        let ep_server = endpoint.clone();
        let server_handle = tokio::spawn(async move {
            let incoming = ep_server.accept().await.unwrap();
            let conn = incoming.await.unwrap();
            let (send, recv) = conn.accept_bi().await.unwrap();
            Connection { send, recv }
        });

        // Yield to allow spawned task to start polling accept()
        tokio::task::yield_now().await;

        // Connect client
        let connecting = endpoint
            .connect_with(client_config, addr, "localhost")
            .map_err(|e| CoreError::Protocol(format!("loopback connect failed: {}", e)))?;

        let conn = tokio::time::timeout(CONNECT_TIMEOUT, connecting)
            .await
            .map_err(|_| CoreError::Protocol("loopback connection timed out".into()))?
            .map_err(|e| CoreError::Protocol(format!("loopback connection failed: {}", e)))?;

        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| CoreError::Protocol(format!("loopback open_bi failed: {}", e)))?;

        let client_conn = Connection { send, recv };
        let server_conn = server_handle
            .await
            .map_err(|e| CoreError::Protocol(format!("server task panicked: {}", e)))?;

        Ok((server_conn, client_conn))
    }
}

impl Connection {
    /// Send a message with length-prefixed framing.
    pub async fn send(&mut self, msg: &Message) -> Result<(), CoreError> {
        let payload = msg.serialize();
        let len = payload.len() as u32;

        self.send
            .write_all(&len.to_be_bytes())
            .await
            .map_err(|e| CoreError::Protocol(format!("send length prefix failed: {}", e)))?;
        self.send
            .write_all(&payload)
            .await
            .map_err(|e| CoreError::Protocol(format!("send payload failed: {}", e)))?;
        self.send
            .flush()
            .await
            .map_err(|e| CoreError::Protocol(format!("send flush failed: {}", e)))?;

        Ok(())
    }

    /// Receive a message with length-prefixed framing.
    pub async fn recv(&mut self) -> Result<Message, CoreError> {
        let mut len_buf = [0u8; 4];
        self.recv
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| CoreError::Protocol(format!("recv length prefix failed: {}", e)))?;

        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 16 * 1024 * 1024 {
            return Err(CoreError::Protocol(format!(
                "message too large: {} bytes",
                len
            )));
        }

        let mut payload = vec![0u8; len];
        self.recv
            .read_exact(&mut payload)
            .await
            .map_err(|e| CoreError::Protocol(format!("recv payload failed: {}", e)))?;

        Message::deserialize(&payload)
    }

    /// Close the connection gracefully.
    pub async fn close(mut self) {
        let _ = self.send.shutdown().await;
    }
}