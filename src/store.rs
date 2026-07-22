//! # Cross-Session Persistence
//!
//! Wanderlust previously had no memory between healing cycles. [`HistoryStore`]
//! persists a JSON-lines record of every heal cycle to
//! `%LOCALAPPDATA%\wanderlust\history.jsonl`, allowing later features (failure
//! escalation, baselining, drift detection) to reason across runs.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HISTORY_FILE_NAME: &str = "history.jsonl";

/// Why a heal cycle ended the way it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealOutcome {
    /// Changes were applied successfully.
    Applied,
    /// Dry-run only; nothing changed.
    DryRun,
    /// Verification failed and we rolled back to the previous PATH.
    RolledBack,
    /// No changes were necessary.
    NoOp,
}

/// A single persisted heal-cycle record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingRecord {
    pub ts: u64,
    pub outcome: HealOutcome,
    pub removed: usize,
    pub added: usize,
    /// Identifier of the trigger (e.g. "scheduled", "manual", "wm_settingchange").
    #[serde(default)]
    pub source: String,
}

impl HealingRecord {
    pub fn now(outcome: HealOutcome, removed: usize, added: usize, source: &str) -> Self {
        Self {
            ts: now_secs(),
            outcome,
            removed,
            added,
            source: source.to_string(),
        }
    }
}

/// A JSON-lines append-only store of heal history.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    /// Appends `record` as one JSON line.
    pub fn append(&self, record: &HealingRecord) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(record)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    }

    /// Loads all persisted records (skipping any malformed lines).
    pub fn load(&self) -> Result<Vec<HealingRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&self.path)?;
        let mut out = Vec::new();
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            if let Ok(rec) = serde_json::from_str::<HealingRecord>(line) {
                out.push(rec);
            }
        }
        Ok(out)
    }

    /// Counts records within the last `window` whose outcome is `RolledBack`.
    pub fn recent_failures(&self, window: Duration) -> Result<usize> {
        let cutoff = now_secs().saturating_sub(window.as_secs());
        let count = self
            .load()?
            .into_iter()
            .filter(|r| r.ts >= cutoff && r.outcome == HealOutcome::RolledBack)
            .count();
        Ok(count)
    }

    /// Returns the number of consecutive `RolledBack` outcomes ending at the
    /// most recent record (0 if the last record was not a rollback).
    pub fn failure_streak(&self) -> Result<usize> {
        let records = self.load()?;
        let mut streak = 0;
        for r in records.iter().rev() {
            if r.outcome == HealOutcome::RolledBack {
                streak += 1;
            } else {
                break;
            }
        }
        Ok(streak)
    }
}

/// Resolves the default history path under `app_dir`.
pub fn default_history_path(app_dir: &Path) -> PathBuf {
    app_dir.join(HISTORY_FILE_NAME)
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
    fn append_then_load_roundtrips() {
        let dir = TestTempDir::new("store").unwrap();
        let store = HistoryStore::new(&dir.child("history.jsonl"));
        store.append(&HealingRecord::now(HealOutcome::Applied, 2, 1, "scheduled")).unwrap();
        store.append(&HealingRecord::now(HealOutcome::NoOp, 0, 0, "manual")).unwrap();

        let records = store.load().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].outcome, HealOutcome::Applied);
        assert_eq!(records[0].removed, 2);
        assert_eq!(records[1].source, "manual");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let dir = TestTempDir::new("store-bad").unwrap();
        let path = dir.child("history.jsonl");
        std::fs::write(&path, "not json\n{\"ts\":1,\"outcome\":\"applied\",\"removed\":0,\"added\":0}\n").unwrap();
        let store = HistoryStore::new(&path);
        assert_eq!(store.load().unwrap().len(), 1);
    }

    #[test]
    fn failure_streak_counts_trailing_rollbacks() {
        let dir = TestTempDir::new("store-streak").unwrap();
        let store = HistoryStore::new(&dir.child("history.jsonl"));
        store.append(&HealingRecord::now(HealOutcome::Applied, 1, 0, "s")).unwrap();
        store.append(&HealingRecord::now(HealOutcome::RolledBack, 0, 0, "s")).unwrap();
        store.append(&HealingRecord::now(HealOutcome::RolledBack, 0, 0, "s")).unwrap();
        assert_eq!(store.failure_streak().unwrap(), 2);

        let store2 = HistoryStore::new(&dir.child("h2.jsonl"));
        store2.append(&HealingRecord::now(HealOutcome::RolledBack, 0, 0, "s")).unwrap();
        store2.append(&HealingRecord::now(HealOutcome::Applied, 0, 0, "s")).unwrap();
        assert_eq!(store2.failure_streak().unwrap(), 0);
    }

    #[test]
    fn recent_failures_in_window() {
        let dir = TestTempDir::new("store-recent").unwrap();
        let store = HistoryStore::new(&dir.child("history.jsonl"));
        store.append(&HealingRecord::now(HealOutcome::RolledBack, 0, 0, "s")).unwrap();
        store.append(&HealingRecord::now(HealOutcome::Applied, 0, 0, "s")).unwrap();
        assert_eq!(store.recent_failures(Duration::from_secs(60)).unwrap(), 1);
    }
}
