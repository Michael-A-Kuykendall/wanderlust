//! Detects orphaned PATH entries after program uninstall.

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
    use proptest::prelude::*;
    use std::collections::HashSet;

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

    proptest! {
        #[test]
        fn candidate_stale_entries_are_subset_of_path(
            ref path_entries in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_\\\\]+", 0..10),
            ref removed_dirs in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..5),
        ) {
            let current = path_entries.join(";");
            let stale = candidate_stale_entries(&current, removed_dirs);

            // All stale entries must be present in the current path
            let path_set: Vec<String> = path_entries.iter().map(|e| normalize(e)).collect();
            for s in &stale {
                let ns = normalize(s);
                prop_assert!(
                    path_set.contains(&ns),
                    "stale '{}' not in original path", s
                );
            }
        }

        #[test]
        fn prune_removes_all_stale(
            ref path_entries in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_\\\\]+", 0..10),
            ref stale_entries in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_\\\\]+", 0..5),
        ) {
            let current = path_entries.join(";");
            let pruned = prune_stale_entries(&current, stale_entries);
            let stale_norm: HashSet<String> = stale_entries.iter().map(|s| normalize(s)).collect();
            let pruned_parts: Vec<&str> = pruned.split(';').filter(|s| !s.is_empty()).collect();
            // None of the pruned entries should contain a stale entry
            for part in &pruned_parts {
                let np = normalize(part);
                prop_assert!(
                    !stale_norm.contains(&np),
                    "stale entry '{}' not removed by prune", part
                );
            }
        }

        #[test]
        fn prune_preserves_non_stale_order(
            ref survivors in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..8),
            ref victims in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..5),
        ) {
            // Interleave survivors and victims to test order preservation
            let mut current_parts: Vec<String> = Vec::new();
            for (i, s) in survivors.iter().enumerate() {
                if i < victims.len() {
                    current_parts.push(victims[i].clone());
                }
                current_parts.push(s.clone());
            }
            let current = current_parts.join(";");
            let pruned = prune_stale_entries(&current, victims);
            let pruned_parts: Vec<&str> = pruned.split(';').filter(|s| !s.is_empty()).collect();

            // All survivors must appear in pruned result, in original order
            let survivor_lower: Vec<String> = survivors.iter().map(|s| normalize(s)).collect();
            let mut pruned_iter = pruned_parts.iter();
            for expected in &survivor_lower {
                // Skip any pruned entries that aren't in the survivor list (they may be victims or empty)
                let found = pruned_iter.by_ref().find(|p| normalize(p) == *expected);
                prop_assert!(
                    found.is_some(),
                    "survivor '{}' not found in pruned result or order violated", expected
                );
            }
        }

        #[test]
        fn candidate_and_prune_are_idempotent(
            ref path_entries in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..8),
            ref removed_dirs in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+", 0..4),
        ) {
            let current = path_entries.join(";");
            let stale = candidate_stale_entries(&current, removed_dirs);
            let pruned_once = prune_stale_entries(&current, &stale);
            let pruned_twice = prune_stale_entries(&pruned_once, &stale);
            prop_assert_eq!(
                pruned_once, pruned_twice,
                "prune must be idempotent: call twice same result"
            );
        }
    }
}
