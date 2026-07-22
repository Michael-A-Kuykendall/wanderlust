//! # Backup Mutual Exclusion
//!
//! Two Wanderlust heal cycles must never write the backup file and Registry
//! simultaneously. [`BackupLock`] provides a cooperative, file-based mutex in
//! `%LOCALAPPDATA%\wanderlust` so that overlapping runs serialize their writes.
//!
//! Rather than relying on OS advisory locks (which behave differently across
//! platforms and are hard to test deterministically), the lock records the
//! acquiring process id and an acquisition timestamp. A lock whose timestamp is
//! older than `stale_timeout` is considered abandoned and is taken over. This
//! guarantees progress even if a previous process crashed while holding the lock.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LOCK_FILE_NAME: &str = "backup.lock";

#[derive(Debug, Serialize, Deserialize)]
struct LockRecord {
    pid: u32,
    acquired_at: u64,
}

/// A cooperative file lock guarding backup/Registry writes.
#[derive(Debug, Clone)]
pub struct BackupLock {
    lock_path: PathBuf,
    pid: u32,
    stale_timeout: Duration,
}

impl BackupLock {
    /// Creates a lock rooted at `app_dir` (typically `%LOCALAPPDATA%\wanderlust`).
    pub fn new(app_dir: &Path, stale_timeout: Duration) -> Self {
        Self {
            lock_path: app_dir.join(LOCK_FILE_NAME),
            pid: std::process::id(),
            stale_timeout,
        }
    }

    /// Attempts to acquire the lock.
    ///
    /// Returns `Ok(())` if the lock was acquired (either it was free, or the
    /// existing holder was deemed stale). Returns an error if a live holder
    /// currently owns the lock.
    pub fn acquire(&self) -> Result<()> {
        if let Some(existing) = self.read_record()? {
            let now = now_secs();
            let age = now.saturating_sub(existing.acquired_at);
            if age <= self.stale_timeout.as_secs() {
                bail!(
                    "backup lock held by live process {} (age {}s < {}s stale timeout)",
                    existing.pid,
                    age,
                    self.stale_timeout.as_secs()
                );
            }
            // Stale: take over.
        }
        self.write_record()
    }

    /// Releases the lock if this process owns it. Errors are ignored on drop.
    pub fn release(&self) -> Result<()> {
        if self.lock_path.exists() {
            std::fs::remove_file(&self.lock_path)?;
        }
        Ok(())
    }

    /// Returns true if the lock currently exists and is not yet stale.
    pub fn is_held(&self) -> Result<bool> {
        match self.read_record()? {
            None => Ok(false),
            Some(rec) => {
                let age = now_secs().saturating_sub(rec.acquired_at);
                Ok(age <= self.stale_timeout.as_secs())
            }
        }
    }

    /// Acquires the lock and returns a [`LockGuard`] that releases it on drop.
    pub fn guard(self) -> Result<LockGuard> {
        self.acquire()?;
        Ok(LockGuard { lock: self })
    }

    fn read_record(&self) -> Result<Option<LockRecord>> {
        if !self.lock_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&self.lock_path)?;
        match serde_json::from_str::<LockRecord>(&content) {
            Ok(rec) => Ok(Some(rec)),
            // A malformed lock file is treated as absent (safe to overwrite).
            Err(_) => Ok(None),
        }
    }

    fn write_record(&self) -> Result<()> {
        if let Some(parent) = self.lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let rec = LockRecord {
            pid: self.pid,
            acquired_at: now_secs(),
        };
        let json = serde_json::to_string(&rec)?;
        std::fs::write(&self.lock_path, json)?;
        Ok(())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// RAII guard that releases its [`BackupLock`] when dropped.
pub struct LockGuard {
    lock: BackupLock,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.lock.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::TestTempDir;

    fn tmp_lock(dir: &TestTempDir, timeout: Duration) -> BackupLock {
        BackupLock::new(dir.path(), timeout)
    }

    #[test]
    fn free_lock_is_acquired() {
        let dir = TestTempDir::new("backup-lock").unwrap();
        let lock = tmp_lock(&dir, Duration::from_secs(60));
        assert!(lock.acquire().is_ok());
        assert!(lock.is_held().unwrap());
        lock.release().unwrap();
        assert!(!lock.is_held().unwrap());
    }

    #[test]
    fn second_acquire_while_held_fails() {
        let dir = TestTempDir::new("backup-lock-held").unwrap();
        let lock = tmp_lock(&dir, Duration::from_secs(60));
        lock.acquire().unwrap();
        // Same directory => same lock file => contention.
        let other = tmp_lock(&dir, Duration::from_secs(60));
        assert!(other.acquire().is_err());
    }

    #[test]
    fn stale_lock_is_taken_over() {
        let dir = TestTempDir::new("backup-lock-stale").unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        std::fs::write(
            &lock_path,
            serde_json::to_string(&LockRecord {
                pid: 9999,
                acquired_at: now_secs() - 1_000_000,
            })
            .unwrap(),
        )
        .unwrap();

        let fresh = BackupLock::new(dir.path(), Duration::from_secs(60));
        assert!(fresh.acquire().is_ok());
        let rec = fresh.read_record().unwrap().unwrap();
        assert_eq!(rec.pid, std::process::id());
    }

    #[test]
    fn malformed_lock_is_treated_as_free() {
        let dir = TestTempDir::new("backup-lock-bad").unwrap();
        let lock_path = dir.path().join(LOCK_FILE_NAME);
        std::fs::write(&lock_path, "not json").unwrap();
        let fresh = BackupLock::new(dir.path(), Duration::from_secs(60));
        assert!(fresh.acquire().is_ok());
    }
}
