//! # YANK Detection Grace Period
//!
//! When a PATH entry's backing directory disappears (e.g. a removable drive,
//! network share, or temporarily disconnected mount), Wanderlust must not
//! immediately delete it. The entry may reappear on the next heal cycle.
//!
//! [`YankGuard`] tracks consecutive "missing" observations per entry and only
//! classifies an entry for removal once it has been missing for `grace_cycles`
//! consecutive healing cycles. This is the "suspicious" vs "confirmed dead"
//! distinction that prevents false-positive removals.

use std::collections::{HashMap, HashSet};

/// The decision produced by [`YankGuard::classify`] for a single heal cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YankDecision {
    /// Entries confirmed missing for `grace_cycles` cycles → safe to remove.
    pub confirmed_dead: Vec<String>,
    /// Entries missing but still inside the grace window → keep, monitor.
    pub suspicious: Vec<String>,
    /// Entries present this cycle (reset their miss counter).
    pub healthy: Vec<String>,
}

/// Tracks consecutive misses per PATH entry to implement a removal grace period.
#[derive(Debug, Clone)]
pub struct YankGuard {
    grace_cycles: u32,
    miss_counts: HashMap<String, u32>,
}

impl YankGuard {
    /// Creates a guard that removes entries only after they have been missing
    /// for `grace_cycles` consecutive cycles. A value of `0` means "remove
    /// immediately" (legacy behavior); `1` means missing once is enough.
    pub fn new(grace_cycles: u32) -> Self {
        Self {
            grace_cycles,
            miss_counts: HashMap::new(),
        }
    }

    /// Advances the guard by one heal cycle.
    ///
    /// * `current_entries` — the full set of entries currently in (or desired in) PATH.
    /// * `still_present`   — entries whose backing directory exists right now.
    ///
    /// Returns the classification. State is updated internally.
    pub fn classify(
        &mut self,
        current_entries: &[String],
        still_present: &HashSet<String>,
    ) -> YankDecision {
        let current_set: HashSet<String> = current_entries
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        let mut confirmed_dead = Vec::new();
        let mut suspicious = Vec::new();
        let mut healthy = Vec::new();

        // Only reason about entries that are supposed to be in PATH.
        let mut observed: HashSet<String> = HashSet::new();
        for entry in current_entries {
            let key = entry.to_lowercase();
            if observed.contains(&key) {
                continue;
            }
            observed.insert(key.clone());

            if still_present.contains(&key) {
                self.miss_counts.remove(&key);
                healthy.push(entry.clone());
            } else {
                let count = self.miss_counts.entry(key.clone()).or_insert(0);
                *count += 1;
                if *count > self.grace_cycles {
                    confirmed_dead.push(entry.clone());
                } else {
                    suspicious.push(entry.clone());
                }
            }
        }

        // Drop counters for entries no longer part of the path at all.
        self.miss_counts
            .retain(|key, _| current_set.contains(key));

        YankDecision {
            confirmed_dead,
            suspicious,
            healthy,
        }
    }

    /// Number of entries currently inside the grace window (missing but not yet dead).
    pub fn suspicious_count(&self) -> usize {
        self.miss_counts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_lowercase()).collect()
    }

    #[test]
    fn entry_present_is_healthy_and_never_removed() {
        let mut guard = YankGuard::new(2);
        let present = set(&["c:\\tools"]);
        let decision = guard.classify(&["C:\\Tools".to_string()], &present);
        assert_eq!(decision.healthy, vec!["C:\\Tools".to_string()]);
        assert!(decision.confirmed_dead.is_empty());
        assert!(decision.suspicious.is_empty());
    }

    #[test]
    fn entry_missing_within_grace_is_suspicious_not_dead() {
        let mut guard = YankGuard::new(3);
        let present = set(&[]);
        let d1 = guard.classify(&["Z:\\Drive".to_string()], &present);
        assert_eq!(d1.suspicious, vec!["Z:\\Drive".to_string()]);
        assert!(d1.confirmed_dead.is_empty());

        let d2 = guard.classify(&["Z:\\Drive".to_string()], &present);
        assert_eq!(d2.suspicious, vec!["Z:\\Drive".to_string()]);
        assert!(d2.confirmed_dead.is_empty());

        let d3 = guard.classify(&["Z:\\Drive".to_string()], &present);
        assert_eq!(d3.suspicious, vec!["Z:\\Drive".to_string()]);
        assert!(d3.confirmed_dead.is_empty());
    }

    #[test]
    fn entry_missing_past_grace_is_confirmed_dead() {
        let mut guard = YankGuard::new(2);
        let present = set(&[]);
        guard.classify(&["Z:\\Drive".to_string()], &present);
        guard.classify(&["Z:\\Drive".to_string()], &present);
        let d3 = guard.classify(&["Z:\\Drive".to_string()], &present);
        assert_eq!(d3.confirmed_dead, vec!["Z:\\Drive".to_string()]);
    }

    #[test]
    fn grace_cycles_zero_removes_immediately() {
        let mut guard = YankGuard::new(0);
        let present = set(&[]);
        let d = guard.classify(&["Z:\\Drive".to_string()], &present);
        assert_eq!(d.confirmed_dead, vec!["Z:\\Drive".to_string()]);
        assert!(d.suspicious.is_empty());
    }

    #[test]
    fn reappearance_resets_counter() {
        let mut guard = YankGuard::new(2);
        let absent = set(&[]);
        let present = set(&["z:\\drive"]);
        guard.classify(&["Z:\\Drive".to_string()], &absent);
        guard.classify(&["Z:\\Drive".to_string()], &absent);
        // Reappears → healthy, counter cleared.
        let d = guard.classify(&["Z:\\Drive".to_string()], &present);
        assert_eq!(d.healthy, vec!["Z:\\Drive".to_string()]);
        assert!(d.confirmed_dead.is_empty());
        assert!(d.suspicious.is_empty());
        assert_eq!(guard.suspicious_count(), 0);
    }
}
