use cntdesk_server::auth::AuthService;
use cntdesk_server::db::Database;
use cntdesk_server::idgen::IdGenerator;

#[test]
fn test_id_format_and_uniqueness() {
    let gen = IdGenerator::new();
    let id = gen.generate();
    assert!(regex::Regex::new(r"^\d{3}-\d{3}-\d{3}$").unwrap().is_match(&id));
    let id2 = gen.generate();
    assert_ne!(id, id2);
}

#[test]
fn test_auth_service_register_and_verify() {
    let db = Database::open_in_memory("key").unwrap();
    let service = AuthService::new(db);
    let pubkey = vec![1u8; 32];
    let id = service.register_new_device(&pubkey).unwrap();
    assert!(service.verify_device_exists(&id));
}

#[test]
fn test_permanent_password_set_and_verify() {
    let db = Database::open_in_memory("key").unwrap();
    let service = AuthService::new(db);
    let pubkey = vec![2u8; 32];
    let id = service.register_new_device(&pubkey).unwrap();

    service.set_permanent_password(&id, b"securepass123").unwrap();
    assert!(service.verify_password(&id, b"securepass123").unwrap());
    assert!(!service.verify_password(&id, b"wrongpassword").unwrap());
}

#[test]
fn test_short_password_rejected() {
    let db = Database::open_in_memory("key").unwrap();
    let service = AuthService::new(db);
    let pubkey = vec![3u8; 32];
    let id = service.register_new_device(&pubkey).unwrap();

    let result = service.set_permanent_password(&id, b"short");
    assert!(result.is_err());
}

#[test]
fn test_one_time_password_generation() {
    let db = Database::open_in_memory("key").unwrap();
    let service = AuthService::new(db);
    let pubkey = vec![4u8; 32];
    let id = service.register_new_device(&pubkey).unwrap();

    let otp = service.generate_one_time_password(&id).unwrap();
    assert_eq!(otp.len(), 6);
    assert!(otp.chars().all(|c| c.is_ascii_digit()));
    assert!(service.verify_password(&id, otp.as_bytes()).unwrap());
}

#[test]
fn test_session_token_creation() {
    use cntdesk_core::crypto::KeyPair;

    let db = Database::open_in_memory("key").unwrap();
    let service = AuthService::new(db);
    let pubkey = vec![5u8; 32];
    let id = service.register_new_device(&pubkey).unwrap();

    let keypair = KeyPair::generate();
    let token = service.create_session_token(&id, &keypair).unwrap();
    assert!(!token.is_empty());
}

#[test]
fn test_verify_valid_session_token() {
    use cntdesk_core::crypto::KeyPair;

    let db = Database::open_in_memory("test").unwrap();
    let auth = AuthService::new(db);
    let keypair = KeyPair::generate();
    let id = auth.register_new_device(&keypair.public_key_bytes()).unwrap();
    auth.set_permanent_password(&id, b"testpassword123").unwrap();

    let token = auth.create_session_token(&id, &keypair).unwrap();
    let verified_id = auth.verify_session_token(&token).unwrap();
    assert_eq!(verified_id, id);
}

#[test]
fn test_verify_expired_token_rejected() {
    use cntdesk_core::crypto::KeyPair;

    let db = Database::open_in_memory("test").unwrap();
    let auth = AuthService::new(db);
    let keypair = KeyPair::generate();
    let id = auth.register_new_device(&keypair.public_key_bytes()).unwrap();

    let token = auth.create_session_token(&id, &keypair).unwrap();
    // Expire the token by setting expires_at to 0
    auth.expire_session_token_for_test(&id, 0).unwrap();

    let result = auth.verify_session_token(&token);
    assert!(result.is_err());
}

#[test]
fn test_verify_tampered_token_rejected() {
    use cntdesk_core::crypto::KeyPair;

    let db = Database::open_in_memory("test").unwrap();
    let auth = AuthService::new(db);
    let keypair = KeyPair::generate();
    let id = auth.register_new_device(&keypair.public_key_bytes()).unwrap();

    let mut token = auth.create_session_token(&id, &keypair).unwrap();
    // Tamper with signature (last 64 bytes)
    if let Some(last) = token.last_mut() {
        *last ^= 0xFF;
    }
    let result = auth.verify_session_token(&token);
    assert!(result.is_err());
}