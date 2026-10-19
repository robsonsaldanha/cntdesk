use cntdesk_client::auth::ClientAuth;

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