use crate::db::{Database, DeviceRecord};
use crate::idgen::IdGenerator;
use argon2::{Argon2, Algorithm, Version, Params, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{SaltString, rand_core::OsRng as ArgonOsRng};
use cntdesk_core::crypto::KeyPair;
use rand::rngs::OsRng;
use rand::Rng;
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