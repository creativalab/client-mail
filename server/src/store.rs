//! Server database (SQLite): devices and their sealed accounts.
//!
//! Device tokens are stored as SHA-256 hashes; account settings only as
//! AES-GCM ciphertext (see `seal`). Nothing here can be read without the
//! phones' keys.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::sync::{Mutex, MutexGuard};

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS devices (
    id         TEXT PRIMARY KEY,
    token_hash BLOB NOT NULL UNIQUE,
    fcm_token  TEXT NOT NULL,
    created    INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS accounts (
    id      TEXT PRIMARY KEY,
    device  TEXT NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    nonce   BLOB NOT NULL,
    sealed  BLOB NOT NULL,
    created INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS accounts_device ON accounts (device);
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub fcm_token: String,
}

#[derive(Debug, Clone)]
pub struct SealedAccount {
    pub id: String,
    pub nonce: Vec<u8>,
    pub sealed: Vec<u8>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Store {
    pub fn open(path: &str) -> Result<Store> {
        let conn = Connection::open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Self::with(conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Store> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(conn: Connection) -> Result<Store> {
        conn.execute_batch(SCHEMA)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Registers a phone; `token` is its secret (only the hash is kept).
    pub fn add_device(&self, id: &str, token: &str, fcm_token: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO devices (id, token_hash, fcm_token, created) VALUES (?1, ?2, ?3, ?4)",
            params![id, hash(token), fcm_token, now()],
        )?;
        Ok(())
    }

    pub fn device_by_token(&self, token: &str) -> Result<Option<Device>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT id, fcm_token FROM devices WHERE token_hash = ?1",
                params![hash(token)],
                |r| {
                    Ok(Device {
                        id: r.get(0)?,
                        fcm_token: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn fcm_token(&self, device: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT fcm_token FROM devices WHERE id = ?1",
                params![device],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_fcm_token(&self, device: &str, fcm_token: &str) -> Result<()> {
        self.lock().execute(
            "UPDATE devices SET fcm_token = ?2 WHERE id = ?1",
            params![device, fcm_token],
        )?;
        Ok(())
    }

    /// Deletes the device and all its accounts.
    pub fn delete_device(&self, device: &str) -> Result<()> {
        self.lock()
            .execute("DELETE FROM devices WHERE id = ?1", params![device])?;
        Ok(())
    }

    pub fn add_account(&self, device: &str, account: &SealedAccount) -> Result<()> {
        self.lock().execute(
            "INSERT INTO accounts (id, device, nonce, sealed, created) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![account.id, device, account.nonce, account.sealed, now()],
        )?;
        Ok(())
    }

    pub fn accounts(&self, device: &str) -> Result<Vec<SealedAccount>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare_cached("SELECT id, nonce, sealed FROM accounts WHERE device = ?1 ORDER BY created")?;
        let rows = stmt.query_map(params![device], |r| {
            Ok(SealedAccount {
                id: r.get(0)?,
                nonce: r.get(1)?,
                sealed: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// True if the account existed and belonged to the device.
    pub fn delete_account(&self, device: &str, account: &str) -> Result<bool> {
        Ok(self.lock().execute(
            "DELETE FROM accounts WHERE id = ?1 AND device = ?2",
            params![account, device],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_and_accounts() {
        let s = Store::in_memory().unwrap();
        s.add_device("d1", "secret-token", "fcm-1").unwrap();
        let d = s.device_by_token("secret-token").unwrap().unwrap();
        assert_eq!(d.id, "d1");
        assert!(s.device_by_token("other").unwrap().is_none());

        s.set_fcm_token("d1", "fcm-2").unwrap();
        assert_eq!(s.fcm_token("d1").unwrap().as_deref(), Some("fcm-2"));

        let acc = SealedAccount {
            id: "a1".into(),
            nonce: vec![1; 12],
            sealed: vec![2; 40],
        };
        s.add_account("d1", &acc).unwrap();
        assert_eq!(s.accounts("d1").unwrap().len(), 1);
        assert!(
            !s.delete_account("d2", "a1").unwrap(),
            "other device cannot delete"
        );

        // Deleting the device removes its accounts.
        s.delete_device("d1").unwrap();
        assert!(s.accounts("d1").unwrap().is_empty());
    }
}
