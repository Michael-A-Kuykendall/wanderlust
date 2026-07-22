//! # Backup Integrity & Restore
//!
//! Backups in Wanderlust are `.reg` files. A corrupted backup is worse than no
//! backup: it creates false confidence before a rollback. This module adds:
//!
//! * A SHA-256 checksum written alongside every backup (so restore can verify
//!   the bytes are intact).
//! * Rotation that keeps only the most recent `N` successful backups.
//! * Partial restore: reassemble a PATH containing only specific entries in
//!   their original order, instead of blindly overwriting with the full backup.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Computes the SHA-256 hex digest of `content`.
pub fn checksum(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Writes `content` to `path` and a sidecar `<path>.sha256` containing its digest.
pub fn write_backup_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    let sum = checksum(content);
    std::fs::write(sidecar_path(path), sum)?;
    Ok(())
}

/// Returns the sidecar path for a backup file.
pub fn sidecar_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap().to_string_lossy().to_string();
    name.push_str(".sha256");
    path.with_file_name(name)
}

/// Verifies a backup file against its stored checksum.
///
/// Returns `Ok(true)` if the digest matches, `Ok(false)` if the sidecar is
/// missing or mismatched, and `Err` only on I/O failure reading the file.
pub fn verify_backup_file(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(path)?;
    let expected = std::fs::read_to_string(sidecar_path(path)).unwrap_or_default();
    Ok(checksum(&content) == expected.trim())
}

/// Keeps only the `keep` most-recently-modified backups matching `glob_prefix`
/// in `dir`, deleting older ones. Backup freshness is determined by file mtime.
pub fn retain_last_n(dir: &Path, prefix: &str, keep: usize) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let matches = p
            .file_name()
            .map(|n| n.to_string_lossy().starts_with(prefix))
            .unwrap_or(false)
            && !p.to_string_lossy().ends_with(".sha256");
        if matches {
            let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
            if let Some(mtime) = mtime {
                candidates.push((mtime, p));
            }
        }
    }
    candidates.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime)); // newest first
    let mut removed = 0;
    for (_, p) in candidates.into_iter().skip(keep) {
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(sidecar_path(&p));
        removed += 1;
    }
    Ok(removed)
}

/// Reassembles a PATH string containing only `keep_entries`, preserving their
/// original order from `original_path`.
///
/// Entries are compared case-insensitively. Entries in `keep_entries` not
/// present in `original_path` are appended at the end. This enables a partial
/// rollback ("restore these specific folders") rather than a full overwrite.
pub fn restore_partial(original_path: &str, keep_entries: &[String]) -> String {
    let want: std::collections::HashSet<String> =
        keep_entries.iter().map(|s| s.to_lowercase()).collect();
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for part in original_path.split(';') {
        if part.is_empty() {
            continue;
        }
        let key = part.to_lowercase();
        if want.contains(&key) && seen.insert(key) {
            out.push(part.to_string());
        }
    }
    // Append any requested entries not found in the original.
    for e in keep_entries {
        let key = e.to_lowercase();
        if seen.insert(key) {
            out.push(e.clone());
        }
    }
    out.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::TestTempDir;

    #[test]
    fn checksum_is_stable_and_hex() {
        assert_eq!(checksum("hello"), checksum("hello"));
        assert_ne!(checksum("hello"), checksum("world"));
        assert!(checksum("x").chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn write_and_verify_roundtrip() {
        let dir = TestTempDir::new("backup-verify").unwrap();
        let path = dir.child("backup.reg");
        write_backup_file(&path, "Windows Registry Editor Version 5.00").unwrap();
        assert!(verify_backup_file(&path).unwrap());
    }

    #[test]
    fn verify_fails_on_corruption() {
        let dir = TestTempDir::new("backup-corrupt").unwrap();
        let path = dir.child("backup.reg");
        write_backup_file(&path, "original").unwrap();
        // Tamper with content without updating checksum.
        std::fs::write(&path, "tampered").unwrap();
        assert!(!verify_backup_file(&path).unwrap());
    }

    #[test]
    fn retain_last_n_removes_oldest() {
        let dir = TestTempDir::new("backup-retain").unwrap();
        for i in 0..5u64 {
            let p = dir.child(format!("backup-{}.reg", i));
            std::fs::write(&p, format!("v{}", i)).unwrap();
            // Stagger mtimes deterministically so rotation keeps the newest.
            let f = std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .unwrap();
            f.set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(i + 1),
            )
            .unwrap();
        }
        // keep=2 → keeps backup-4 (newest) and backup-3, removes the other 3.
        let removed = retain_last_n(dir.path(), "backup-", 2).unwrap();
        assert_eq!(removed, 3);
        assert!(dir.child("backup-4.reg").exists());
        assert!(!dir.child("backup-0.reg").exists());
    }

    #[test]
    fn restore_partial_preserves_order_and_appends_missing() {
        let original = r"C:\A;C:\B;C:\C";
        let kept = vec![r"c:\c".to_string(), r"c:\a".to_string(), r"c:\new".to_string()];
        let result = restore_partial(original, &kept);
        // Original order of kept entries (A then C) is preserved; new entry appended verbatim.
        assert_eq!(result, r"C:\A;C:\C;c:\new");
    }

    #[test]
    fn restore_partial_drops_unwanted() {
        let original = r"C:\A;C:\B;C:\C";
        let kept = vec![r"c:\a".to_string()];
        let result = restore_partial(original, &kept);
        assert_eq!(result, r"C:\A");
    }
}
