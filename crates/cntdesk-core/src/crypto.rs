//! Cryptographic primitives for CNT Desk (Ed25519)

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

use crate::error::CoreError;

/// Ed25519 keypair wrapper
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    /// Generate a new random keypair using OS CSPRNG
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Sign arbitrary bytes, returning the signature as Vec<u8>
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let sig = self.signing_key.sign(message);
        sig.to_bytes().to_vec()
    }

    /// Verify a signature against a message using this keypair's public key
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        if signature.len() != 64 {
            return false;
        }
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(signature);
        let sig = Signature::from_bytes(&sig_bytes);
        let verifying_key = self.signing_key.verifying_key();
        verifying_key.verify(message, &sig).is_ok()
    }

    /// Return the public key as raw bytes (32 bytes)
    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.signing_key.verifying_key().as_bytes().to_vec()
    }

    /// Return the secret (private) key as raw bytes (32 bytes)
    pub fn secret_bytes(&self) -> Vec<u8> {
        self.signing_key.to_bytes().to_vec()
    }

    /// Reconstruct a KeyPair from raw secret key bytes (32 bytes)
    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        if bytes.len() != 32 {
            return Err(CoreError::Crypto(format!(
                "Invalid secret key length: expected 32, got {}",
                bytes.len()
            )));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        Ok(Self { signing_key })
    }
}

/// Session token with expiration
pub struct SessionToken {
    pub token: Vec<u8>,
    pub expires_at: u64,
}

impl SessionToken {
    /// Sign the token contents (token bytes + expiry) with the given keypair
    pub fn sign(&self, keypair: &KeyPair) -> Vec<u8> {
        let mut data = self.token.clone();
        data.extend_from_slice(&self.expires_at.to_le_bytes());
        keypair.sign(&data)
    }

    /// Verify a signed session token against a public key
    pub fn verify(signed: &[u8], public_key: &[u8]) -> Result<Self, CoreError> {
        if public_key.len() != 32 {
            return Err(CoreError::Crypto("Invalid public key length".into()));
        }
        if signed.len() < 72 {
            // minimum: 8 bytes expiry + 64 bytes signature
            return Err(CoreError::Crypto("Signed token too short".into()));
        }
        let pk_bytes: [u8; 32] = public_key.try_into().map_err(|_| {
            CoreError::Crypto("Failed to convert public key".into())
        })?;
        let verifying_key = VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        // The signed payload is everything except the last 64 bytes (signature)
        let payload_end = signed.len() - 64;
        let payload = &signed[..payload_end];
        let sig_bytes: [u8; 64] = signed[payload_end..].try_into().map_err(|_| {
            CoreError::Crypto("Failed to extract signature".into())
        })?;
        let signature = Signature::from_bytes(&sig_bytes);
        verifying_key
            .verify(payload, &signature)
            .map_err(|e| CoreError::Crypto(e.to_string()))?;

        // Extract expires_at from last 8 bytes of payload
        if payload.len() < 8 {
            return Err(CoreError::Crypto("Payload too short for expiry".into()));
        }
        let expiry_start = payload.len() - 8;
        let token = payload[..expiry_start].to_vec();
        let expires_at = u64::from_le_bytes(
            payload[expiry_start..]
                .try_into()
                .map_err(|_| CoreError::Crypto("Failed to read expiry".into()))?,
        );

        Ok(SessionToken { token, expires_at })
    }
}