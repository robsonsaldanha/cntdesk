//! Client identity management with Ed25519 keypair persistence.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cntdesk_core::crypto::KeyPair;

/// Manages local client identity (Ed25519 keypair) with file-based persistence.
pub struct ClientAuth {
    config_dir: PathBuf,
}

impl ClientAuth {
    /// Create a new ClientAuth with the given config directory.
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&config_dir)
            .with_context(|| format!("Failed to create config dir: {:?}", config_dir))?;
        Ok(Self { config_dir })
    }

    /// Create a ClientAuth using a temporary directory (for tests).
    pub fn new_temp_dir() -> Result<Self> {
        let tmp = tempfile::tempdir().context("Failed to create temp dir")?;
        // keep() returns Result<PathBuf, PersistError> in newer tempfile versions,
        // but older versions return PathBuf directly. Handle both via into_path() fallback.
        #[allow(deprecated)]
        let path = tmp.into_path();
        Self::new(path)
    }

    /// Check if an identity keypair already exists on disk.
    pub fn has_identity(&self) -> bool {
        self.identity_path().exists()
    }

    /// Ensure an identity exists: load existing or generate new one.
    pub fn ensure_identity(&self) -> Result<KeyPair> {
        if self.has_identity() {
            self.load_identity()
        } else {
            let kp = KeyPair::generate();
            self.save_identity(&kp)?;
            Ok(kp)
        }
    }

    /// Load identity from disk.
    pub fn load_identity(&self) -> Result<KeyPair> {
        let bytes = fs::read(self.identity_path()).context("Failed to read identity key")?;
        KeyPair::from_secret_bytes(&bytes).context("Failed to parse identity key")
    }

    fn save_identity(&self, kp: &KeyPair) -> Result<()> {
        let path = self.identity_path();
        fs::write(&path, kp.secret_bytes()).context("Failed to write identity key")?;
        // Set 0600 permissions on Unix
        #[cfg(unix)]
        {
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&path, perms).context("Failed to set identity key permissions")?;
        }
        Ok(())
    }

    fn identity_path(&self) -> PathBuf {
        self.config_dir.join("identity.key")
    }

    /// Persist device ID to disk with 0600 permissions.
    pub fn save_device_id(&self, id: &str) -> Result<()> {
        let path = self.device_id_path();
        fs::write(&path, id.as_bytes()).context("Failed to write device ID")?;
        #[cfg(unix)]
        {
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&path, perms).context("Failed to set device ID permissions")?;
        }
        Ok(())
    }

    /// Load persisted device ID from disk, returning None if not present.
    pub fn load_device_id(&self) -> Option<String> {
        let path = self.device_id_path();
        fs::read_to_string(&path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    fn device_id_path(&self) -> PathBuf {
        self.config_dir.join("device_id")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let subdir = tmp.path().join("subdir");
        let _auth = ClientAuth::new(subdir.clone()).unwrap();
        assert!(subdir.exists());
    }
}