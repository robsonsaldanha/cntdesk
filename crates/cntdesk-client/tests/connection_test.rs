use cntdesk_client::auth::ClientAuth;
use cntdesk_client::connection::ConnectionManager;
use cntdesk_client::commands::ConnectionState;

#[tokio::test]
async fn test_client_first_time_registration() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    assert!(!auth.has_identity());
    let kp = auth.ensure_identity().unwrap();
    assert!(auth.has_identity());
    assert_eq!(kp.public_key_bytes().len(), 32);
}

#[tokio::test]
async fn test_client_load_existing_identity() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    let kp1 = auth.ensure_identity().unwrap();
    let kp2 = auth.load_identity().unwrap();
    assert_eq!(kp1.public_key_bytes(), kp2.public_key_bytes());
}

#[tokio::test]
async fn test_client_ensure_identity_idempotent() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    let kp1 = auth.ensure_identity().unwrap();
    let kp2 = auth.ensure_identity().unwrap();
    assert_eq!(kp1.public_key_bytes(), kp2.public_key_bytes());
}

#[tokio::test]
async fn test_device_id_persisted_after_save() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    // Initially no device ID
    assert!(auth.load_device_id().is_none());
    // Save a device ID
    auth.save_device_id("123-456-789").unwrap();
    // Load it back
    let loaded = auth.load_device_id();
    assert_eq!(loaded, Some("123-456-789".to_string()));
}

#[tokio::test]
async fn test_connection_state_initial_is_disconnected() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    let cm = ConnectionManager::new_test("127.0.0.1:0", auth);
    assert_eq!(*cm.state(), ConnectionState::Disconnected);
    assert!(cm.device_id().is_none());
}

#[tokio::test]
async fn test_active_session_initially_none() {
    let auth = ClientAuth::new_temp_dir().unwrap();
    let mut cm = ConnectionManager::new_test("127.0.0.1:0", auth);
    assert!(cm.active_session().is_none());
}