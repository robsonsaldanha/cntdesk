use cntdesk_server::db::{Database, DeviceRecord};

// Note: Cargo converts hyphens to underscores in crate names for Rust imports.
// The crate is named "cntdesk-server" in Cargo.toml but imported as "cntdesk_server".

#[test]
fn test_register_and_lookup_device() {
    let db = Database::open_in_memory("test-key").unwrap();
    let record = DeviceRecord {
        id: "123-456-789".into(),
        public_key: vec![0u8; 32],
        password_hash: None,
        created_at: 1000,
    };
    db.register_device(&record).unwrap();
    let found = db.get_device("123-456-789").unwrap().unwrap();
    assert_eq!(found.id, record.id);
    assert_eq!(found.public_key, record.public_key);
}

#[test]
fn test_duplicate_id_rejected() {
    let db = Database::open_in_memory("test-key").unwrap();
    let record = DeviceRecord {
        id: "123-456-789".into(),
        public_key: vec![0u8; 32],
        password_hash: None,
        created_at: 1000,
    };
    db.register_device(&record).unwrap();
    let result = db.register_device(&record);
    assert!(result.is_err());
}