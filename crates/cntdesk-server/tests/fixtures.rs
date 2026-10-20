use cntdesk_server::db::Database;
use cntdesk_server::auth::AuthService;
use cntdesk_server::signaling::SignalingService;
use cntdesk_core::crypto::KeyPair;
use cntdesk_core::protocol::Message;

pub struct TestServer {
    pub auth: AuthService,
    pub signaling: SignalingService,
}

impl TestServer {
    pub fn start() -> Self {
        let db = Database::open_in_memory("test-key").expect("Failed to open in-memory DB");
        let auth = AuthService::new(db);
        let signaling = SignalingService::new_test();
        Self { auth, signaling }
    }
}

pub struct TestClient {
    pub keypair: KeyPair,
    pub device_id: Option<String>,
}

impl TestClient {
    pub fn new() -> Self {
        Self {
            keypair: KeyPair::generate(),
            device_id: None,
        }
    }

    pub fn register(&mut self, server: &TestServer) -> Result<String, String> {
        let id = server.auth.register_new_device(&self.keypair.public_key_bytes())?;
        self.device_id = Some(id.clone());
        Ok(id)
    }

    pub fn set_permanent_password(&self, server: &TestServer, password: &str) -> Result<(), String> {
        let id = self.device_id.as_ref().ok_or("Not registered")?;
        server.auth.set_permanent_password(id, password.as_bytes())
    }

    pub fn verify_password(&self, server: &TestServer, password: &str) -> Result<bool, String> {
        let id = self.device_id.as_ref().ok_or("Not registered")?;
        server.auth.verify_password(id, password.as_bytes())
    }

    /// Authenticate and obtain a session token (service-layer, no network).
    pub fn authenticate(&self, server: &TestServer, password: &str) -> Result<Vec<u8>, String> {
        let id = self.device_id.as_ref().ok_or("Not registered")?;
        let valid = server.auth.verify_password(id, password.as_bytes())?;
        if !valid {
            return Err("Password verification failed".to_string());
        }
        server.auth.create_session_token(id, &self.keypair)
    }
}

/// TestHarness sets up two authenticated peers with a signaling session between them,
/// enabling service-layer e2e tests without real QUIC transport.
pub struct TestHarness {
    pub server: TestServer,
    pub host_id: String,
    pub viewer_id: String,
    pub session_id: String,
}

impl TestHarness {
    /// Create a harness with host and viewer registered, passwords set, session created.
    pub async fn setup(host_password: &str) -> Self {
        let server = TestServer::start();

        let mut host = TestClient::new();
        let host_id = host.register(&server).expect("Host registration failed");
        host.set_permanent_password(&server, host_password).expect("Host password set failed");

        let mut viewer = TestClient::new();
        let viewer_id = viewer.register(&server).expect("Viewer registration failed");

        // Create signaling session between host and viewer
        let session_id = server.signaling.create_session(&host_id, &viewer_id)
            .await
            .expect("Session creation failed");

        Self {
            server,
            host_id,
            viewer_id,
            session_id,
        }
    }

    /// Simulate host sending a message through the relay to the viewer.
    pub async fn host_send_to_viewer(&self, msg: Message) -> Result<(), String> {
        self.server.signaling.route_message(&self.session_id, msg).await
    }

    /// Simulate viewer sending a message through the relay to the host.
    pub async fn viewer_send_to_host(&self, msg: Message) -> Result<(), String> {
        // For bidirectional, we use the same session_id; the signaling service
        // routes messages to the pending queue which either peer can drain.
        self.server.signaling.route_message(&self.session_id, msg).await
    }

    /// Drain pending messages (simulates what forward_loop does for a peer).
    pub async fn drain_pending(&self) -> Vec<Message> {
        self.server.signaling.get_pending_messages(&self.session_id).await
    }
}