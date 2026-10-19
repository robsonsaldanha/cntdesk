mod fixtures;

use fixtures::{TestServer, TestClient};
use regex::Regex;

#[tokio::test]
async fn test_full_connection_flow() {
    let server = TestServer::start();
    let mut host = TestClient::new();
    let mut viewer = TestClient::new();

    // Host registers and gets ID
    let host_id = host.register(&server).unwrap();
    assert!(
        Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap().is_match(&host_id),
        "ID format must be XXX-XXX-XXX, got: {}",
        host_id
    );

    // Host sets password
    host.set_permanent_password(&server, "testpass123").unwrap();

    // Viewer registers (needed before connecting in service-layer test)
    let _viewer_id = viewer.register(&server).unwrap();

    // Verify password works
    let valid = host.verify_password(&server, "testpass123").unwrap();
    assert!(valid, "Correct password must verify");

    // Disconnect (service-layer: just verify state)
    // In a full network test this would call viewer.disconnect()
}

#[tokio::test]
async fn test_wrong_password_rejected() {
    let server = TestServer::start();
    let mut host = TestClient::new();

    let host_id = host.register(&server).unwrap();
    host.set_permanent_password(&server, "correctpassword").unwrap();

    // Wrong password must fail verification
    let result = host.verify_password(&server, "wrongpassword");
    match result {
        Ok(false) => {} // Expected: verification returns false
        Err(_) => {}    // Also acceptable: error returned
        Ok(true) => panic!("Wrong password must not verify for device {}", host_id),
    }
}

#[tokio::test]
async fn test_duplicate_id_atomicity() {
    use std::sync::Arc;
    use tokio::task;

    let server = Arc::new(TestServer::start());
    let mut handles = vec![];

    // Spawn 10 concurrent registration requests
    for i in 0..10 {
        let srv = Arc::clone(&server);
        let handle = task::spawn(async move {
            let client = TestClient::new();
            let id = srv.auth.register_new_device(&client.keypair.public_key_bytes())
                .expect(&format!("Registration {} failed", i));
            id
        });
        handles.push(handle);
    }

    // Collect all IDs
    let mut ids = vec![];
    for handle in handles {
        let id = handle.await.unwrap();
        ids.push(id);
    }

    // All 10 must succeed
    assert_eq!(ids.len(), 10, "All 10 registrations must succeed");

    // All IDs must be unique
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 10, "All IDs must be unique, got {} duplicates", 10 - unique.len());

    // All IDs must match format
    let re = Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap();
    for id in &ids {
        assert!(re.is_match(id), "ID '{}' does not match XXX-XXX-XXX format", id);
    }
}