//! # Known-Good PATH Snapshots
//!
//! Healing decisions are currently based only on the live registry. This module
//! adds a "source of truth": periodically we capture a snapshot of a verified
//! healthy PATH (taken right after a successful heal + probe). Later cycles can
//! diff against the last known-good snapshot to detect drift *before* it causes
//! failures, and to roll back to it on probe failure.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SNAPSHOT_FILE_NAME: &str = "last_known_good.json";

/// A captured, validated PATH state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSnapshot {
    pub taken_at: u64,
    pub entries: Vec<String>,
}

impl PathSnapshot {
    pub fn from_path(path: &str) -> Self {
        let entries = path
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        Self {
            taken_at: now_secs(),
            entries,
        }
    }

    fn set(&self) -> std::collections::HashSet<String> {
        self.entries.iter().map(|s| s.to_lowercase()).collect()
    }
}

/// The comparison of a live PATH against a known-good snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct PathDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// 0.0..=1.0 fraction of the snapshot that is missing from live PATH.
    pub drift_score: f64,
}

impl PathDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Computes the diff between a known-good `snapshot` and the live `current` PATH.
pub fn diff_snapshot(snapshot: &PathSnapshot, current: &str) -> PathDiff {
    let snap_set = snapshot.set();
    let live_entries: Vec<String> = current
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let live_set: std::collections::HashSet<String> =
        live_entries.iter().map(|s| s.to_lowercase()).collect();

    let removed: Vec<String> = snapshot
        .entries
        .iter()
        .filter(|e| !live_set.contains(&e.to_lowercase()))
        .cloned()
        .collect();
    let added: Vec<String> = live_entries
        .iter()
        .filter(|e| !snap_set.contains(&e.to_lowercase()))
        .cloned()
        .collect();

    let drift_score = if snapshot.entries.is_empty() {
        0.0
    } else {
        removed.len() as f64 / snapshot.entries.len() as f64
    };

    PathDiff {
        added,
        removed,
        drift_score,
    }
}

/// Stores the most recent known-good snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    path: PathBuf,
}

impl SnapshotStore {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    pub fn save(&self, snapshot: &PathSnapshot) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Option<PathSnapshot>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&content).ok())
    }
}

/// Returns true if `current` has drifted from `snapshot` beyond `threshold`
/// (a 0.0..=1.0 fraction of the snapshot's entries that are missing).
pub fn is_drift_beyond(snapshot: &PathSnapshot, current: &str, threshold: f64) -> bool {
    diff_snapshot(snapshot, current).drift_score > threshold
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::TestTempDir;

    #[test]
    fn snapshot_from_path_splits_entries() {
        let snap = PathSnapshot::from_path(r"C:\A;C:\B");
        assert_eq!(snap.entries, vec![r"C:\A".to_string(), r"C:\B".to_string()]);
    }

    #[test]
    fn diff_detects_added_and_removed() {
        let snap = PathSnapshot::from_path(r"C:\A;C:\B");
        let diff = diff_snapshot(&snap, r"C:\B;C:\C");
        assert_eq!(diff.removed, vec![r"C:\A".to_string()]);
        assert_eq!(diff.added, vec![r"C:\C".to_string()]);
        assert!((diff.drift_score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn diff_is_empty_when_identical() {
        let snap = PathSnapshot::from_path(r"C:\A;C:\B");
        let diff = diff_snapshot(&snap, r"C:\A;C:\B");
        assert!(diff.is_empty());
        assert_eq!(diff.drift_score, 0.0);
    }

    #[test]
    fn drift_beyond_threshold() {
        let snap = PathSnapshot::from_path(r"C:\A;C:\B;C:\C");
        assert!(is_drift_beyond(&snap, r"C:\A", 0.5));
        assert!(!is_drift_beyond(&snap, r"C:\A;C:\B", 0.5));
    }

    #[test]
    fn snapshot_store_roundtrip() {
        let dir = TestTempDir::new("snapshot").unwrap();
        let store = SnapshotStore::new(&dir.child("last_known_good.json"));
        store.save(&PathSnapshot::from_path(r"C:\A;C:\B")).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.entries, vec![r"C:\A".to_string(), r"C:\B".to_string()]);
    }
}
