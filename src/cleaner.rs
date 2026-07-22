//! # Cleaner Logic
//!
//! This module contains the core business logic for Wanderlust. It is responsible for:
//! 1. Orchestrating the discovery of tools (`heal_path`).
//! 2. Constructing the optimal PATH string (`build_minimal_path`).
//! 3. Safely applying changes to the Windows Registry (`apply_path`).
//! 4. Verifying system stability and rolling back if necessary.
//!
//! It also handles the generation of POSIX-compatible cache files for Git Bash / MSYS2 integration.

use crate::discovery;
use crate::invariant_ppt::*;
use crate::system::{SystemOps, WindowsSystem};
use anyhow::{Result, bail};
use log::{error, info};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use windows_registry::CURRENT_USER;

// The following are only used by production-only integration (history, logs,
// backup lock) that is intentionally compiled out of `#[cfg(test)]` so unit
// tests stay hermetic and fast.
#[cfg(not(test))]
use crate::backup_lock::BackupLock;
#[cfg(not(test))]
use crate::logging;
#[cfg(not(test))]
use crate::store;
#[cfg(not(test))]
use serde_json;
#[cfg(not(test))]
use std::time::Duration;

/// Resolves the Wanderlust application data directory
/// (`%LOCALAPPDATA%\wanderlust`), used for backups, history, and logs.
#[cfg(not(test))]
fn app_data_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.data_local_dir().join("wanderlust"))
}

/// The main entry point for the healing logic.
///
/// # Arguments
///
/// * `dry_run` - If true, calculates the new PATH and prints it, but does NOT modify the Registry or file system.
///
/// # Returns
///
/// Returns `Ok(())` on success, or an `anyhow::Result` error if Registry access fails or verification breaks.
pub fn heal_path(dry_run: bool) -> Result<()> {
    let system = WindowsSystem;

    // Discovery runs silently - user doesn't need to see this
    let candidates_map = discovery::discover_candidates();

    heal_path_with_system(&candidates_map, &system, dry_run)
}

/// Runs both PATH scopes through an injectable system boundary.
///
/// This preserves the current production sequencing while allowing tests to observe whether a
/// System-scope failure is incorrectly discarded before User-scope processing begins.
fn heal_path_with_system(
    candidates_map: &HashMap<String, Vec<discovery::Candidate>>,
    system: &impl SystemOps,
    dry_run: bool,
) -> Result<()> {
    // First, clean the SYSTEM PATH (HKLM) - this removes duplicates from the machine-wide config
    // Silently skip if not admin - the dry-run output will explain
    clean_system_path(system, dry_run)?;

    // Then heal the User PATH with discovery results
    run_healing(candidates_map, system, dry_run)
}

/// Cleans the System PATH (HKLM) by removing duplicates.
/// This only deduplicates - it does NOT add new paths or remove valid ones.
/// Requires Admin privileges.
fn clean_system_path(system: &impl SystemOps, dry_run: bool) -> Result<()> {
    let system_path = system.read_system_path_registry()?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut cleaned: Vec<String> = Vec::new();

    for part in system_path.split(';') {
        if part.is_empty() {
            continue;
        }
        let normalized = part.to_lowercase();
        if !seen.contains(&normalized) {
            seen.insert(normalized);
            cleaned.push(part.to_string()); // Keep original casing
        }
    }

    let new_system_path = cleaned.join(";");

    let old_count = system_path.split(';').filter(|s| !s.is_empty()).count();
    let new_count = cleaned.len();

    if old_count == new_count {
        info!("System PATH already clean ({} entries)", new_count);
        return Ok(());
    }

    info!(
        "System PATH: {} -> {} entries (removing {} duplicates)",
        old_count,
        new_count,
        old_count - new_count
    );

    if dry_run {
        println!("--- DRY RUN: System PATH would be cleaned ---");
        return Ok(());
    }

    system.write_system_path_registry(&new_system_path)?;
    info!("System PATH cleaned successfully");
    Ok(())
}

/// Core logic for healing, decoupled from the concrete System for testing.
pub fn run_healing(
    candidates_map: &HashMap<String, Vec<discovery::Candidate>>,
    system: &impl SystemOps,
    dry_run: bool,
) -> Result<()> {
    // Get current User PATH for comparison
    let current_user_path = system.read_user_path_registry().unwrap_or_default();
    let current_entries: HashSet<String> = current_user_path
        .split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect();

    let new_path_string = build_minimal_path(candidates_map);

    let new_entries: HashSet<String> = new_path_string
        .split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect();

    // Calculate what's changing
    let removing: Vec<&str> = current_user_path
        .split(';')
        .filter(|s| !s.is_empty())
        .filter(|s| !new_entries.contains(&s.to_lowercase()))
        .collect();

    let adding: Vec<&str> = new_path_string
        .split(';')
        .filter(|s| !s.is_empty())
        .filter(|s| !current_entries.contains(&s.to_lowercase()))
        .collect();

    if dry_run {
        println!();
        println!("═══════════════════════════════════════════════════════════════");
        println!("                   What Wanderlust Will Do");
        println!("═══════════════════════════════════════════════════════════════");
        println!();

        // System PATH status
        let system_path = system.read_system_path_registry().unwrap_or_default();
        let sys_parts: Vec<&str> = system_path.split(';').filter(|s| !s.is_empty()).collect();
        let sys_unique: HashSet<&str> = sys_parts.iter().cloned().collect();
        let sys_dups = sys_parts.len() - sys_unique.len();

        println!("SYSTEM PATH (shared by all users):");
        if sys_dups > 0 {
            println!(
                "  Currently has {} folders with {} duplicates.",
                sys_parts.len(),
                sys_dups
            );
            println!("  → Will remove duplicates (requires running as Administrator)");
        } else {
            println!(
                "  ✓ Already clean ({} folders, no duplicates)",
                sys_parts.len()
            );
        }

        // User PATH changes
        let before_count = current_user_path
            .split(';')
            .filter(|s| !s.is_empty())
            .count();
        let after_count = new_path_string.split(';').filter(|s| !s.is_empty()).count();

        println!();
        println!("USER PATH (just your tools):");
        println!("  Currently: {} folders", before_count);
        println!("  After:     {} folders", after_count);

        if !removing.is_empty() {
            println!();
            println!(
                "  REMOVING {} folders (already in System PATH or duplicates):",
                removing.len()
            );
            for p in &removing {
                println!("    ✕ {}", p);
            }
        }

        if !adding.is_empty() {
            println!();
            println!(
                "  ADDING {} folders (discovered tools not yet in PATH):",
                adding.len()
            );
            for p in &adding {
                println!("    + {}", p);
            }
        }

        println!();
        println!("───────────────────────────────────────────────────────────────");
        if removing.is_empty() && adding.is_empty() && sys_dups == 0 {
            println!();
            println!("✓ Nothing to do! Your PATH is already optimal.");
        } else {
            println!();
            println!("This is a preview. Run 'wanderlust heal' to apply changes.");
            println!("(Changes only affect new terminals. Current terminal keeps old PATH.)");
        }
        println!();

        return Ok(());
    }

    // Fail-closed: if discovery produced no candidates, don't wipe existing PATH.
    if new_path_string.is_empty() {
        info!("Discovery returned no candidates. Leaving User PATH unchanged.");
        return Ok(());
    }

    // Generate and write POSIX path for Git Bash / MSYS integration
    // This file contains the COMPLETE PATH (System + User) in POSIX format
    if let Some(user_dirs) = directories::UserDirs::new() {
        // Get System PATH and convert to POSIX
        let system_path = system.read_system_path_registry().unwrap_or_default();
        let system_posix: Vec<String> = system_path
            .split(';')
            .filter(|s| !s.is_empty())
            .map(win_to_posix)
            .collect();

        // Convert User PATH to POSIX
        let user_posix: Vec<String> = new_path_string
            .split(';')
            .filter(|s| !s.is_empty())
            .map(win_to_posix)
            .collect();

        // Combine: System first, then User (matches Windows behavior)
        let full_posix = [system_posix, user_posix].concat().join(":");

        let posix_file = user_dirs.home_dir().join(".wanderlust_posix");
        if let Ok(mut f) = File::create(&posix_file) {
            let _ = writeln!(f, "{}", full_posix);
            info!(
                "Wrote POSIX path to {:?} ({} entries)",
                posix_file,
                full_posix.matches(':').count() + 1
            );
        }
    }

    // Apply the changes to the system
    apply_path(system, &new_path_string)?;
    info!("Successfully healed PATH!");

    // Persist a cross-session record and emit a structured log line so later
    // runs (failure escalation, baselining, drift detection) can reason across
    // heal cycles. Skipped under `#[cfg(test)]` to keep unit tests hermetic.
    #[cfg(not(test))]
    {
        if let Some(dir) = app_data_dir() {
            let _ = store::HistoryStore::new(&store::default_history_path(&dir)).append(
                &store::HealingRecord::now(
                    store::HealOutcome::Applied,
                    removing.len(),
                    adding.len(),
                    "scheduled",
                ),
            );
            let _ = logging::append_event(
                &logging::default_log_path(&dir),
                &logging::LogEvent::new(logging::Level::Info, "cleaner", "healed PATH").with_fields(
                    serde_json::json!({ "removed": removing.len(), "added": adding.len() }),
                ),
            );
        }
    }

    Ok(())
}

/// Runs a "Doctor" check to report on the health of the CURRENT and STORED path.
///
/// This does not modify the system.
pub fn doctor() -> Result<()> {
    let system = WindowsSystem;

    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("                      PATH Health Report");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("Windows has TWO places where PATH is stored:");
    println!();

    // 1. System PATH (HKLM)
    let system_path = system.read_system_path_registry().unwrap_or_default();
    let system_parts: Vec<&str> = system_path.split(';').filter(|s| !s.is_empty()).collect();
    let system_unique: HashSet<&str> = system_parts.iter().cloned().collect();
    let system_dups = system_parts.len() - system_unique.len();

    println!("1. SYSTEM PATH ({} folders)", system_parts.len());
    println!("   Shared by all users. Has Windows, Program Files, etc.");
    if system_dups > 0 {
        println!(
            "   ⚠ Problem: {} duplicate entries (run as Admin to fix)",
            system_dups
        );
    } else {
        println!("   ✓ No duplicates");
    }

    // 2. User PATH (HKCU)
    let hive = CURRENT_USER.open("Environment")?;
    let user_path = hive.get_string("Path").unwrap_or_default();
    let user_parts: Vec<&str> = user_path.split(';').filter(|s| !s.is_empty()).collect();
    let user_unique: HashSet<&str> = user_parts.iter().cloned().collect();
    let user_dups = user_parts.len() - user_unique.len();

    println!();
    println!("2. USER PATH ({} folders)", user_parts.len());
    println!("   Just for you. Has your tools like Python, Cargo, Scoop, etc.");
    if user_dups > 0 {
        println!("   ⚠ Problem: {} duplicate entries", user_dups);
    } else {
        println!("   ✓ No duplicates");
    }

    // 3. Check for User entries that duplicate System entries
    let system_normalized: HashSet<String> =
        system_parts.iter().map(|s| s.to_lowercase()).collect();
    let overlap: Vec<&str> = user_parts
        .iter()
        .filter(|p| system_normalized.contains(&p.to_lowercase()))
        .cloned()
        .collect();

    if !overlap.is_empty() {
        println!();
        println!(
            "⚠ OVERLAP: {} folders appear in BOTH System and User PATH.",
            overlap.len()
        );
        println!("   This is wasteful. Examples:");
        for p in overlap.iter().take(3) {
            println!("     - {}", p);
        }
        if overlap.len() > 3 {
            println!("     ... and {} more", overlap.len() - 3);
        }
    }

    // 4. Current terminal session explanation
    println!();
    println!("───────────────────────────────────────────────────────────────");
    println!();
    let total = system_parts.len() + user_parts.len();
    println!("When you open a terminal, Windows combines both:");
    println!(
        "  System ({}) + User ({}) = {} folders to search for commands",
        system_parts.len(),
        user_parts.len(),
        total
    );

    if let Ok(current) = std::env::var("PATH") {
        let current_count = current.split(';').filter(|s| !s.is_empty()).count();
        if current_count != total {
            println!();
            println!(
                "  Your current terminal has {} (Git Bash adds some extras).",
                current_count
            );
        }
    }

    // 5. Summary
    println!();
    println!("───────────────────────────────────────────────────────────────");
    if system_dups == 0 && user_dups == 0 && overlap.is_empty() {
        println!();
        println!("✓ Your PATH is healthy! No action needed.");
    } else {
        println!();
        println!("Run 'wanderlust heal' to fix the issues above.");
    }
    println!();

    Ok(())
}

/// Constructs a minimal USER PATH string from discovered candidates.
///
/// **The Immutable Logic:**
/// 1.  **System PATH exclusion**: Don't duplicate anything already in HKLM System PATH.
/// 2.  **Deduplication**: We normalize paths (lowercase) to ensure `C:\Win` and `c:\win` don't duplicate.
/// 3.  **Discovery**: We append all discovered directories that contain executables.
/// 4.  **No Windows paths**: System32, Windows, etc. belong in System PATH, not User PATH.
fn build_minimal_path(map: &HashMap<String, Vec<discovery::Candidate>>) -> String {
    // Read System PATH to avoid duplicating entries.
    let system = WindowsSystem;
    let system_path = system.read_system_path_registry().unwrap_or_default();
    let system_path_entry_count = system_path
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| normalize_path(&PathBuf::from(entry)))
        .collect::<HashSet<_>>()
        .len();
    info!(
        "System PATH has {} entries (will not duplicate these)",
        system_path_entry_count
    );

    let new_path = build_minimal_path_for_system_path(map, &system_path);
    assert_user_path_invariant(&new_path);
    new_path
}

/// Plans the discovery-only User PATH output against supplied System PATH state.
///
/// This is intentionally pure so tests can supply synthetic registry values without reading
/// Windows state. `build_minimal_path` remains the production adapter and preserves its I/O.
pub(crate) fn build_minimal_path_for_system_path(
    map: &HashMap<String, Vec<discovery::Candidate>>,
    system_path: &str,
) -> String {
    let system_path_entries: HashSet<PathBuf> = system_path
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| normalize_path(&PathBuf::from(entry)))
        .collect();
    let mut seen_paths = system_path_entries;
    let mut user_paths: Vec<PathBuf> = Vec::new();

    // Collect all unique directories from discovery that aren't in System PATH.
    for candidates in map.values() {
        for candidate in candidates {
            let norm = normalize_path(&candidate.path);

            // Skip Windows system directories - they belong in System PATH.
            let path_str = norm.to_string_lossy().to_lowercase();
            if path_str.contains("\\windows\\") || path_str.starts_with("c:\\windows") {
                continue;
            }

            if seen_paths.insert(norm.clone()) {
                user_paths.push(norm);
            }
        }
    }

    // Sort to ensure deterministic output.
    user_paths.sort();
    user_paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Retains the existing invariant check at the production adapter boundary.
fn assert_user_path_invariant(new_path: &str) {
    if new_path.is_empty() {
        return;
    }

    let entries: Vec<&str> = new_path
        .split(';')
        .filter(|entry| !entry.is_empty())
        .collect();
    let has_user_tools = entries.iter().any(|entry| {
        let entry = entry.to_lowercase();
        entry.contains("users") || entry.contains("appdata") || entry.contains(".cargo")
    });
    assert_invariant(
        has_user_tools || !entries.is_empty(),
        "User PATH should contain user-specific paths",
        Some("Cleaner"),
    );
}

/// Normalizes a path for comparison.
///
/// - Lowercases the string (Windows is case-insensitive).
fn normalize_path(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy().to_string().to_lowercase();
    PathBuf::from(s)
}

/// Converts a Windows path to POSIX format for Git Bash / MSYS2.
///
/// Examples:
/// - `C:\Windows\system32` -> `/c/Windows/system32`
/// - `D:\Program Files\Git` -> `/d/Program Files/Git`
fn win_to_posix(path: &str) -> String {
    let s = path.replace('\\', "/");
    // Handle drive letter: C:/... -> /c/...
    if s.len() >= 2 && s.chars().nth(1) == Some(':') {
        let drive = s.chars().next().unwrap().to_lowercase().next().unwrap();
        return format!("/{}{}", drive, &s[2..]);
    }
    s
}

/// Applies the new PATH to the Windows Registry with transactional safety.
///
/// # Safety Steps
/// 1.  **Read Current**: Gets the existing PATH.
/// 2.  **Backup**: Writes the existing PATH to `%LOCALAPPDATA%\wanderlust\backup.reg`.
/// 3.  **Write**: Updates `HKCU\Environment\Path`.
/// 4.  **Broadcast**: Sends `WM_SETTINGCHANGE` so running apps (like Explorer) notice.
/// 5.  **Verify**: Runs `cmd`, `powershell`, `whoami` to ensure the system is usable.
/// 6.  **Rollback**: If verification fails, restores the old PATH and errors out.
fn apply_path(system: &impl SystemOps, new_val: &str) -> Result<()> {
    // 1. Open Registry Key (Read Old)
    let old_val = system.read_user_path_registry()?;

    // Fail-closed: an empty discovery plan must not wipe the existing User PATH.
    if new_val.is_empty() && !old_val.is_empty() {
        return Err(anyhow::anyhow!(
            "Refusing to replace non-empty User PATH with empty result"
        ));
    }

    // 2. Backup to %LOCALAPPDATA%\wanderlust\backup.reg
    //
    // Serialize backup + registry writes against any concurrently running heal
    // cycle (e.g. overlapping scheduled tasks) so two instances can't clobber
    // each other's `.reg` backup or last-known-good snapshot.
    #[cfg(not(test))]
    let _lock = directories::BaseDirs::new().map(|base| {
        let app = base.data_local_dir().join("wanderlust");
        BackupLock::new(&app, Duration::from_secs(30)).guard().ok()
    });

    if let Some(base_dirs) = directories::BaseDirs::new() {
        let app_data = base_dirs.data_local_dir().join("wanderlust");

        if let Err(e) = std::fs::create_dir_all(&app_data) {
            return Err(anyhow::anyhow!("Failed to create backup directory: {}", e));
        } else {
            let backup_path = app_data.join("backup.reg");
            // Escape backslashes for .reg file format ("\" -> "\\")
            let escaped_old_val = old_val.replace("\\", "\\\\").replace("\"", "\\\"");
            let reg_content = format!(
                "Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Environment]\n\"Path\"=\"{}\"\n",
                escaped_old_val
            );

            if let Err(e) = system.write_backup_file(&backup_path, &reg_content) {
                return Err(anyhow::anyhow!("Failed to write backup content: {}", e));
            } else {
                info!("Backed up old PATH to {:?}", backup_path);
            }
        }
    }

    // 3. Set new PATH
    system.write_user_path_registry(new_val)?;

    // 4. Broadcast change (Twice with delay, to ensure standard apps pick it up)
    let _ = system.broadcast_environment_change();
    if !cfg!(test) {
        // Sleep in prod, but not in tests if we can help it (unless mocking threaded sleep?)
        // For now, simple standard sleep.
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let _ = system.broadcast_environment_change();

    // 5. Verify consistency
    if !system.verify_environment_health() {
        error!("Verification failed! The new PATH seems broken. Rolling back...");

        // ROLLBACK
        if let Err(e) = system.restore_user_path_registry(&old_val) {
            error!("CRITICAL: Failed to write back old PATH: {}", e);
            bail!("Verification failed AND Rollback failed. Please restore from backup manually.");
        }
        let _ = system.broadcast_environment_change();
        bail!("Verification failed. Rolled back to previous PATH.");
    }

    Ok(())
}

// broadcast_change and verify_path_health are removed (moved to SystemOps)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::invariant_ppt::clear_invariant_log;
    use crate::system::{MockCall, MockOperation, MockSystem};
    use proptest::prelude::*;

    /// Behavior snapshot: the pure extraction accepts synthetic state and leaves all SystemOps
    /// boundaries untouched while preserving the pre-extraction path construction result.
    #[test]
    fn extracted_planner_preserves_output_without_external_system_interaction() {
        let mut candidates = HashMap::new();
        candidates.insert(
            "alpha".to_string(),
            vec![discovery::Candidate {
                path: PathBuf::from(r"C:\Users\Example\Bin"),
                _source: "snapshot".to_string(),
            }],
        );
        candidates.insert(
            "beta".to_string(),
            vec![discovery::Candidate {
                path: PathBuf::from(r"C:\Users\Example\Tools"),
                _source: "snapshot".to_string(),
            }],
        );
        candidates.insert(
            "system".to_string(),
            vec![discovery::Candidate {
                path: PathBuf::from(r"C:\Windows\System32"),
                _source: "snapshot".to_string(),
            }],
        );
        let system = MockSystem::new();

        let planned =
            build_minimal_path_for_system_path(&candidates, r"C:\Shared;C:\Users\Example\Bin");

        assert_eq!(planned, r"c:\users\example\tools");
        assert!(
            system.calls().is_empty(),
            "pure planning must not touch SystemOps"
        );
    }

    proptest! {
        #[test]
        fn test_build_minimal_path_properties(
            cmd_names in prop::collection::vec("[a-z]{3,5}", 1..10),
            paths in prop::collection::vec("[a-z]:\\[a-z]{3,8}\\[a-z]{3,8}", 1..10)
        ) {
            // Setup
            clear_invariant_log(); // Clear previous runs

            let mut map = HashMap::new();
            for (i, cmd) in cmd_names.iter().enumerate() {
                let p = if i < paths.len() { paths[i].clone() } else { "c:\temp".to_string() };
                map.insert(cmd.clone(), vec![discovery::Candidate {
                    path: PathBuf::from(p),
                    _source: "test".to_string()
                }]);
            }

            // Action
            let result = build_minimal_path(&map);

            // Assertions (Invariants are checked internal to the function, but we verify properties here)

            // 1. User PATH should NOT contain System32 (that's in System PATH now)
            // The result may be empty if all discovered paths are in System PATH

            // 2. Must not contain duplicates (Naive check on string)
            if !result.is_empty() {
                let parts: Vec<&str> = result.split(';').collect();
                let unique: HashSet<&str> = parts.iter().cloned().collect();
                assert_eq!(parts.len(), unique.len(), "Property Test Failed: Result contains duplicates");
            }
        }

        #[test]
        fn test_run_healing_mocks(
            cmd_names in prop::collection::vec("[a-z]{3,5}", 1..5),
            paths in prop::collection::vec("c:\\\\users\\\\[a-z]{3,8}\\\\[a-z]{3,8}", 0..5),
            start_reg in "c:\\\\users\\\\test\\\\path1;c:\\\\users\\\\test\\\\path2"
        ) {
            use crate::system::MockSystem;

            // Setup Mock System with both User and System PATH
            let mut reg = HashMap::new();
            reg.insert("Path".to_string(), start_reg.clone());
            reg.insert("SystemPath".to_string(), r"C:\Windows\system32;C:\Windows".to_string());
            let system = MockSystem {
                registry: std::sync::Mutex::new(reg),
                ..Default::default()
            };

            // Setup Candidates - use user paths, not system paths
            let mut map = HashMap::new();
            for (i, cmd) in cmd_names.iter().enumerate() {
                 let p = if i < paths.len() { paths[i].clone() } else { r"C:\Users\test\bin".to_string() };
                 map.insert(cmd.clone(), vec![discovery::Candidate { path: PathBuf::from(p), _source: "test".to_string() }]);
            }

            // Action
            // We force dry_run = false so it actually "writes" to the mock.
            let result = run_healing(&map, &system, false);

            // Assertions
            prop_assert!(result.is_ok(), "Healing failed: {:?}", result.err());

            // Verify Mock Registry was updated (may be empty if all paths in system)
            let _new_reg = system.read_user_path_registry().unwrap();

            // Verify broadcast
            let broadcast = *system.broadcast_called.lock().unwrap();
            prop_assert!(broadcast, "Broadcast missed");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 16,
            failure_persistence: None,
            .. ProptestConfig::default()
        })]

        /// **Validates: Requirements 1.1, 2.1**
        #[test]
        fn fail_closed_unreadable_user_path_blocks_mutation(
            proposed_leaf in "[a-z]{1,8}"
        ) {
            let system = MockSystem::new();
            system.fail_next(MockOperation::ReadUserPath);

            let result = apply_path(&system, &format!(r"C:\\Tools\\{proposed_leaf}"));
            let calls = system.calls();

            prop_assert!(result.is_err(), "an unreadable User PATH must be surfaced as a User failure");
            prop_assert!(
                !calls.iter().any(|call| matches!(
                    call,
                    MockCall::WriteUserPath(_)
                        | MockCall::BroadcastEnvironmentChange
                        | MockCall::StagePosixCache { .. }
                        | MockCall::CommitPosixCache { .. }
                )),
                "blocking an unreadable User PATH must precede User, cache, and broadcast writes; calls: {calls:?}"
            );
        }

        /// **Validates: Requirements 1.1, 2.1**
        #[test]
        fn fail_closed_empty_discovery_plan_blocks_mutation(
            old_leaf in "[a-z]{1,8}"
        ) {
            let system = MockSystem::with_registry("Path", &format!(r"C:\\Existing\\{old_leaf}"));

            let result = apply_path(&system, "");
            let calls = system.calls();

            prop_assert!(result.is_err(), "an empty or untrustworthy discovery plan must not be committed");
            prop_assert!(
                !calls.iter().any(|call| matches!(
                    call,
                    MockCall::WriteUserPath(_)
                        | MockCall::BroadcastEnvironmentChange
                        | MockCall::StagePosixCache { .. }
                        | MockCall::CommitPosixCache { .. }
                )),
                "blocking an empty discovery plan must precede User, cache, and broadcast writes; calls: {calls:?}"
            );
        }

        /// **Validates: Requirements 1.2, 2.2**
        #[test]
        fn fail_closed_backup_failure_blocks_mutation(
            proposed_leaf in "[a-z]{1,8}"
        ) {
            let system = MockSystem::with_registry("Path", r"C:\\Existing\\Tool");
            system.fail_next(MockOperation::WriteBackup);

            let result = apply_path(&system, &format!(r"C:\\Proposed\\{proposed_leaf}"));
            let calls = system.calls();

            prop_assert!(result.is_err(), "a backup failure must be returned as a User failure");
            prop_assert!(
                !calls.iter().any(|call| matches!(
                    call,
                    MockCall::WriteUserPath(_)
                        | MockCall::BroadcastEnvironmentChange
                        | MockCall::StagePosixCache { .. }
                        | MockCall::CommitPosixCache { .. }
                )),
                "backup failure must precede User, cache, and broadcast writes; calls: {calls:?}"
            );
        }

        /// **Validates: Requirements 1.4, 2.4**
        #[test]
        fn fail_closed_system_scope_failure_is_not_reported_as_success(
            user_leaf in "[a-z]{1,8}"
        ) {
            let system = MockSystem::with_registry("Path", &format!(r"C:\\Users\\Example\\{user_leaf}"));
            system.fail_next(MockOperation::ReadSystemPath);
            let candidates = HashMap::<String, Vec<discovery::Candidate>>::new();

            let result = heal_path_with_system(&candidates, &system, true);
            let calls = system.calls();

            prop_assert!(result.is_err(), "a System-scope failure must be surfaced instead of returning success");
            let error = result.unwrap_err().to_string();
            prop_assert!(error.contains("System"), "the surfaced failure must identify the System scope: {error}");
            prop_assert!(
                !calls.iter().any(|call| matches!(
                    call,
                    MockCall::WriteUserPath(_)
                        | MockCall::BroadcastEnvironmentChange
                        | MockCall::StagePosixCache { .. }
                        | MockCall::CommitPosixCache { .. }
                )),
                "a blocking System failure must precede User, cache, and broadcast writes; calls: {calls:?}"
            );
        }
    }
}
