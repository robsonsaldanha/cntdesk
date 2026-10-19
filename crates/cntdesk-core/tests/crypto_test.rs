use cntdesk_core::crypto::KeyPair;

#[test]
fn test_keypair_sign_verify() {
    let kp = KeyPair::generate();
    let msg = b"test message";
    let sig = kp.sign(msg);
    assert!(kp.verify(msg, &sig));
    assert!(!kp.verify(b"wrong", &sig));
}

#[test]
fn test_keypair_public_key_length() {
    let kp = KeyPair::generate();
    assert_eq!(kp.public_key_bytes().len(), 32);
}

#[test]
fn test_different_keypairs_produce_different_keys() {
    let kp1 = KeyPair::generate();
    let kp2 = KeyPair::generate();
    assert_ne!(kp1.public_key_bytes(), kp2.public_key_bytes());
}

#[test]
fn test_signature_wrong_key_fails() {
    let kp1 = KeyPair::generate();
    let kp2 = KeyPair::generate();
    let msg = b"test message";
    let sig = kp1.sign(msg);
    assert!(!kp2.verify(msg, &sig));
}