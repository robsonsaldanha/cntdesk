use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub id: String,
    pub public_key: Vec<u8>,
    pub password_hash: Option<Vec<u8>>,
    pub created_at: u64,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &str, _key: &str) -> anyhow::Result<Self> {
        // NOTE: SQLCipher encryption requires libsqlcipher native library.
        // Using bundled rusqlite for compilation; encryption to be enabled
        // when sqlcipher-sys is available in production builds.
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn open_in_memory(_key: &str) -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn init_schema(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                public_key BLOB NOT NULL,
                password_hash BLOB,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_tokens (
                device_id TEXT PRIMARY KEY,
                token BLOB NOT NULL,
                expires_at INTEGER NOT NULL,
                FOREIGN KEY (device_id) REFERENCES devices(id)
            );",
        )
    }

    pub fn register_device(&self, record: &DeviceRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO devices (id, public_key, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![record.id, record.public_key, record.password_hash, record.created_at],
        ).map_err(|e| {
            if let rusqlite::Error::SqliteFailure(err, _) = &e {
                if err.code == rusqlite::ErrorCode::ConstraintViolation {
                    return anyhow::anyhow!("Device ID '{}' already exists", record.id);
                }
            }
            anyhow::anyhow!(e)
        })?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_device(&self, id: &str) -> anyhow::Result<Option<DeviceRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, public_key, password_hash, created_at FROM devices WHERE id = ?1"
        )?;
        let result = stmt.query_row(params![id], |row| {
            Ok(DeviceRecord {
                id: row.get(0)?,
                public_key: row.get(1)?,
                password_hash: row.get(2)?,
                created_at: row.get(3)?,
            })
        });
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }

    pub fn update_password(&self, id: &str, hash: &[u8]) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE devices SET password_hash = ?1 WHERE id = ?2",
            params![hash, id],
        )?;
        if rows == 0 {
            anyhow::bail!("Device '{}' not found", id);
        }
        Ok(())
    }

    pub fn set_session_token(&self, id: &str, token: &[u8], expires_at: u64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO session_tokens (device_id, token, expires_at) VALUES (?1, ?2, ?3)",
            params![id, token, expires_at],
        )?;
        Ok(())
    }

    /// Update only the expiry timestamp of an existing session token (for testing).
    pub fn update_session_token_expiry(&self, id: &str, expires_at: u64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE session_tokens SET expires_at = ?1 WHERE device_id = ?2",
            params![expires_at, id],
        )?;
        Ok(())
    }

    /// Get the stored expiry timestamp for a device's session token.
    pub fn get_session_token_expiry(&self, id: &str) -> anyhow::Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT expires_at FROM session_tokens WHERE device_id = ?1",
            params![id],
            |row| row.get(0),
        ).optional()?;
        Ok(result)
    }
}