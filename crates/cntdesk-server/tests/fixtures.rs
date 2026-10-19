use cntdesk_server::db::Database;
use cntdesk_server::auth::AuthService;
use cntdesk_core::crypto::KeyPair;

pub struct TestServer {
    pub auth: AuthService,
}

impl TestServer {
    pub fn start() -> Self {
        let db = Database::open_in_memory("test-key").expect("Failed to open in-memory DB");
        let auth = AuthService::new(db);
        Self { auth }
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
}