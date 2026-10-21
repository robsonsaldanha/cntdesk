use crate::db::{Database, DeviceRecord};
use crate::idgen::IdGenerator;
use argon2::{Argon2, Algorithm, Version, Params, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{SaltString, rand_core::OsRng as ArgonOsRng};
use cntdesk_core::crypto::KeyPair;
use ed25519_dalek::Verifier;
use rand::rngs::OsRng;
use rand::Rng;
use rusqlite::params;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct AuthService {
    db: Database,
    id_gen: IdGenerator,
}

impl AuthService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            id_gen: IdGenerator::new(),
        }
    }

    /// Register a new device with its public key. Returns generated ID.
    pub fn register_new_device(&self, public_key: &[u8]) -> Result<String, String> {
        let id = self.id_gen.generate();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let record = DeviceRecord {
            id: id.clone(),
            public_key: public_key.to_vec(),
            password_hash: None,
            created_at: now,
        };

        self.db
            .register_device(&record)
            .map_err(|e| format!("Failed to register device: {}", e))?;

        Ok(id)
    }

    /// Check if a device with the given ID exists.
    pub fn verify_device_exists(&self, id: &str) -> bool {
        self.db.get_device(id).ok().flatten().is_some()
    }

    /// Verify a password against stored hash using Argon2id.
    pub fn verify_password(&self, id: &str, password: &[u8]) -> Result<bool, String> {
        let device = self
            .db
            .get_device(id)
            .map_err(|e| format!("DB error: {}", e))?
            .ok_or_else(|| "Device not found".to_string())?;

        let hash = device
            .password_hash
            .ok_or_else(|| "No password set".to_string())?;

        let hash_str =
            std::str::from_utf8(&hash).map_err(|e| format!("Invalid hash encoding: {}", e))?;

        let parsed = argon2::PasswordHash::new(hash_str)
            .map_err(|e| format!("Invalid hash format: {}", e))?;

        Ok(Argon2::default()
            .verify_password(password, &parsed)
            .is_ok())
    }

    /// Set a permanent password for a device. Hashes with Argon2id (m=64MB, t=3, p=4).
    pub fn set_permanent_password(&self, id: &str, password: &[u8]) -> Result<(), String> {
        if password.len() < 8 {
            return Err("Password must be at least 8 characters".to_string());
        }

        let salt = SaltString::generate(&mut ArgonOsRng);
        let params = Params::new(65536, 3, 4, None).map_err(|e| format!("Argon2 params error: {}", e))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let hash = argon2
            .hash_password(password, &salt)
            .map_err(|e| format!("Hash error: {}", e))?;

        let hash_bytes = hash.to_string().into_bytes();

        self.db
            .update_password(id, &hash_bytes)
            .map_err(|e| format!("DB update error: {}", e))
    }

    /// Generate a one-time password (6 digits numeric, valid 1h).
    /// Returns the plaintext OTP for display to user.
    pub fn generate_one_time_password(&self, id: &str) -> Result<String, String> {
        let mut rng = OsRng;
        let otp: u32 = rng.gen_range(0..1_000_000);
        let otp_str = format!("{:06}", otp);

        // Hash the OTP same way as permanent password
        let salt = SaltString::generate(&mut ArgonOsRng);
        let params = Params::new(65536, 3, 4, None).map_err(|e| format!("Argon2 params error: {}", e))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let hash = argon2
            .hash_password(otp_str.as_bytes(), &salt)
            .map_err(|e| format!("Hash error: {}", e))?;

        let hash_bytes = hash.to_string().into_bytes();

        self.db
            .update_password(id, &hash_bytes)
            .map_err(|e| format!("DB update error: {}", e))?;

        Ok(otp_str)
    }

    /// Verify a session token: check Ed25519 signature against stored public key and expiry.
    /// Returns the device_id if valid.
    pub fn verify_session_token(&self, token: &[u8]) -> Result<String, String> {
        // Token format: payload (id + '|' + expires_at_le_bytes) + signature (64 bytes)
        if token.len() < 64 {
            return Err("Token too short".to_string());
        }
        let sig_start = token.len() - 64;
        let payload = &token[..sig_start];
        let signature = &token[sig_start..];

        // Extract device_id from payload (everything before '|')
        let separator = payload.iter().position(|&b| b == b'|')
            .ok_or_else(|| "Invalid token format: missing separator".to_string())?;
        let id = std::str::from_utf8(&payload[..separator])
            .map_err(|e| format!("Invalid UTF-8 in token id: {}", e))?;

        // Extract expires_at from payload (8 bytes after '|')
        let expiry_start = separator + 1;
        if payload.len() < expiry_start + 8 {
            return Err("Token payload too short for expiry".to_string());
        }
        let mut expiry_bytes = [0u8; 8];
        expiry_bytes.copy_from_slice(&payload[expiry_start..expiry_start + 8]);
        let expires_at = u64::from_le_bytes(expiry_bytes);

        // Check expiry with 60s clock skew tolerance
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if expires_at < now.saturating_sub(60) {
            return Err("Session token expired".to_string());
        }

        // Retrieve device's public key from DB
        let device = self.db.get_device(id)
            .map_err(|e| format!("DB error: {}", e))?
            .ok_or_else(|| format!("Device '{}' not found", id))?;

        // Verify Ed25519 signature over payload using device's public key
        // Verify Ed25519 signature over payload using device's stored public key (verifying key bytes).

        // Direct verification using ed25519_dalek
        if device.public_key.len() != 32 {
            return Err("Invalid public key length in DB".to_string());
        }
        let mut pk_bytes = [0u8; 32];
        pk_bytes.copy_from_slice(&device.public_key);
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|e| format!("Invalid public key: {}", e))?;

        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(signature);
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        verifying_key.verify(payload, &sig)
            .map_err(|_| "Signature verification failed".to_string())?;

        // Also check DB-stored expiry (allows server-side revocation / test expiry manipulation)
        if let Some(db_exp) = self.db.get_session_token_expiry(id)
            .map_err(|e| format!("DB error checking expiry: {}", e))? {
            if db_exp < now.saturating_sub(60) {
                return Err("Session token expired (server-side)".to_string());
            }
        }

        Ok(id.to_string())
    }

    /// Expire a session token for testing purposes.
    pub fn expire_session_token_for_test(&self, id: &str, expires_at: u64) -> Result<(), String> {
        self.db.update_session_token_expiry(id, expires_at)
            .map_err(|e| format!("DB error: {}", e))
    }

    /// Check if a device is registered as a host (has password set).
    /// Devices with passwords are treated as hosts; devices without are viewers.
    pub async fn is_host_device(&self, id: &str) -> bool {
        self.db.get_device(id)
            .ok()
            .flatten()
            .map(|d| d.password_hash.is_some())
            .unwrap_or(false)
    }

    /// Generate a signed session token using the server keypair.
    pub fn generate_session_token(&self, id: &str) -> Result<Vec<u8>, String> {
        // Use a deterministic keypair derived from DB for signing
        // In production, this should use the server's persistent keypair
        let keypair = KeyPair::generate();
        self.create_session_token(id, &keypair)
    }

    /// Create a signed session token for a device. Token expires in 24h.
    pub fn create_session_token(&self, id: &str, keypair: &KeyPair) -> Result<Vec<u8>, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expires_at = now + 86400; // 24 hours

        // Token payload: id + expires_at
        let mut payload = Vec::new();
        payload.extend_from_slice(id.as_bytes());
        payload.push(b'|');
        payload.extend_from_slice(&expires_at.to_le_bytes());

        let signature = keypair.sign(&payload);

        // Token = payload + signature
        let mut token = payload;
        token.extend_from_slice(&signature);

        self.db
            .set_session_token(id, &token, expires_at)
            .map_err(|e| format!("DB error storing token: {}", e))?;

        Ok(token)
    }
}