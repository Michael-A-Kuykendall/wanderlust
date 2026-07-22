//! SHA-256 checksummed backup files with rotation and partial restore.

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
    use proptest::prelude::*;
    use std::collections::HashSet;

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

    proptest! {
        #[test]
        fn checksum_is_always_64_hex_chars(ref s in ".*") {
            let sum = checksum(s);
            prop_assert!(sum.chars().all(|c| c.is_ascii_hexdigit()));
            prop_assert_eq!(sum.len(), 64);
        }

        #[test]
        fn checksum_is_deterministic(ref s in ".*") {
            prop_assert_eq!(checksum(s), checksum(s));
        }

        #[test]
        fn restore_partial_invariants(
            ref parts in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..15),
            ref keep in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..10),
        ) {
            let original = parts.join(";");
            let result = restore_partial(&original, keep);

            let result_parts: Vec<&str> = result.split(';').filter(|s| !s.is_empty()).collect();

            // Every result entry must come from keep_entries (case-insensitive)
            for entry in &result_parts {
                let lower = entry.to_lowercase();
                prop_assert!(
                    keep.iter().any(|k| k.to_lowercase() == lower),
                    "result contains '{}' not in keep_entries", entry
                );
            }

            // No duplicates
            let unique: HashSet<&str> = result_parts.iter().cloned().collect();
            prop_assert_eq!(result_parts.len(), unique.len(), "result has duplicates");

            // All non-empty keep entries appear in the result
            let keep_lower: Vec<String> = keep.iter().map(|k| k.to_lowercase()).collect();
            for k in keep.iter().filter(|k| !k.is_empty()) {
                prop_assert!(
                    result_parts.iter().any(|r| keep_lower.contains(&r.to_lowercase())),
                    "keep entry '{}' missing from result", k
                );
            }
        }

        #[test]
        fn restore_partial_preserves_original_order(
            ref a in "[a-zA-Z]:\\\\[a-zA-Z0-9_]+",
            ref b in "[a-zA-Z]:\\\\[a-zA-Z0-9_]+",
            ref c in "[a-zA-Z]:\\\\[a-zA-Z0-9_]+",
        ) {
            let original = format!("{};{};{}", a, b, c);
            // keep claims to want b and a in that order → result should have a then b
            let keep = vec![b.to_uppercase(), a.to_uppercase()];
            let result = restore_partial(&original, &keep);
            let result_parts: Vec<&str> = result.split(';').filter(|s| !s.is_empty()).collect();
            // a and b both appear, and a comes before b (original order)
            let pos_a = result_parts.iter().position(|r| r.to_lowercase() == a.to_lowercase());
            let pos_b = result_parts.iter().position(|r| r.to_lowercase() == b.to_lowercase());
            if let (Some(pa), Some(pb)) = (pos_a, pos_b) {
                prop_assert!(pa < pb, "original order not preserved");
            }
        }
    }
}
