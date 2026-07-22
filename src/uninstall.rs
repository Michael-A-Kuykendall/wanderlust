//! # Uninstall Healing
//!
//! Wanderlust does not watch for software uninstall events. When a program is
//! removed, its PATH entries remain orphaned until the next scheduled heal (up
//! to 30 minutes later). This module detects, immediately after an uninstall,
//! which PATH entries are now dead (they live under a removed program
//! directory) so healing can react without waiting for the next cycle.
//!
//! Note: Windows broadcasts `WM_SETTINGCHANGE` after PATH changes (see
//! [`crate::system::SystemOps::broadcast_environment_change`]); the live
//! daemon can hook that message to trigger [`crate::cleaner::heal_path`] right
//! away. This module provides the *decision* logic for what to clean.

/// Returns the PATH entries (from `current_path`) that live under any of the
/// `removed_dirs` program locations. Such entries are orphaned by an uninstall
/// and are safe to remove on the next immediate heal.
///
/// Matching is case-insensitive prefix matching on normalized paths.
pub fn candidate_stale_entries(current_path: &str, removed_dirs: &[String]) -> Vec<String> {
    let removed_norm: Vec<String> = removed_dirs
        .iter()
        .map(|d| normalize(d))
        .filter(|d| !d.is_empty())
        .collect();

    current_path
        .split(';')
        .filter(|s| !s.is_empty())
        .filter(|entry| {
            let e = normalize(entry);
            removed_norm.iter().any(|d| e == *d || e.starts_with(&format!("{}\\", d)))
        })
        .map(|s| s.to_string())
        .collect()
}

/// Removes the orphaned entries from `current_path`, preserving order of the
/// surviving entries. Case-insensitive.
pub fn prune_stale_entries(current_path: &str, stale: &[String]) -> String {
    let stale_set: std::collections::HashSet<String> =
        stale.iter().map(|s| normalize(s)).collect();
    current_path
        .split(';')
        .filter(|s| !s.is_empty())
        .filter(|entry| !stale_set.contains(&normalize(entry)))
        .collect::<Vec<_>>()
        .join(";")
}

fn normalize(p: &str) -> String {
    p.trim_end_matches('\\').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_entries_under_removed_dir() {
        let current = r"C:\Program Files\OldApp\bin;C:\Users\me\tools;C:\Python39";
        let removed = vec![r"C:\Program Files\OldApp".to_string()];
        let stale = candidate_stale_entries(current, &removed);
        assert_eq!(stale, vec![r"C:\Program Files\OldApp\bin".to_string()]);
    }

    #[test]
    fn detects_exact_removed_dir_as_entry() {
        let current = r"C:\OldApp;C:\Keep";
        let removed = vec![r"C:\OldApp".to_string()];
        let stale = candidate_stale_entries(current, &removed);
        assert_eq!(stale, vec![r"C:\OldApp".to_string()]);
    }

    #[test]
    fn ignores_unrelated_entries() {
        let current = r"C:\Keep\bin;C:\Other";
        let removed = vec![r"C:\OldApp".to_string()];
        assert!(candidate_stale_entries(current, &removed).is_empty());
    }

    #[test]
    fn prune_removes_only_stale() {
        let current = r"C:\Program Files\OldApp\bin;C:\Users\me\tools;C:\Python39";
        let stale = vec![r"C:\Program Files\OldApp\bin".to_string()];
        let pruned = prune_stale_entries(current, &stale);
        assert_eq!(pruned, r"C:\Users\me\tools;C:\Python39");
    }

    #[test]
    fn prune_case_insensitive() {
        let current = r"c:\oldapp\bin;C:\Keep";
        let stale = vec![r"C:\OLDAPP\BIN".to_string()];
        let pruned = prune_stale_entries(current, &stale);
        assert_eq!(pruned, r"C:\Keep");
    }
}
