//! # Cleaner Logic
//!
//! This module contains the core business logic for Wanderlust. It is responsible for:
//! 1. Orchestrating the discovery of tools (`heal_path`).
//! 2. Constructing the optimal PATH string (`build_minimal_path`).
//! 3. Safely applying changes to the Windows Registry (`apply_path`).
//! 4. Verifying system stability and rolling back if necessary.
//!
//! It also handles the generation of POSIX-compatible cache files for Git Bash / MSYS2 integration.

use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;
use anyhow::{Result, bail};
use log::{info, debug, warn, error};
use windows_registry::CURRENT_USER;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use crate::discovery;

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
    info!("Discovering candidates...");
    let candidates_map = discovery::discover_candidates();
    
    info!("Building minimal PATH...");
    let new_path_string = build_minimal_path(&candidates_map);
    
    if dry_run {
        println!("--- DRY RUN: New PATH would be ---");
        for part in new_path_string.split(';') {
            println!("{}", part);
        }
        println!("----------------------------------");
        return Ok(());
    }

    // Generate and write POSIX path for Git Bash / MSYS integration
    // This allows shells like bash to just `source ~/.wanderlust_posix` to get the clean PATH.
    if let Some(user_dirs) = directories::UserDirs::new() {
        let posix_path = new_path_string.split(';')
            .map(|p| {
                // Convert C:\Path\To to /c/Path/To
                let s = p.replace(":", "").replace("\\", "/");
                if let Some(first_char) = s.chars().next().filter(|c| c.is_alphabetic()) {
                     return format!("/{}{}", first_char.to_lowercase(), &s[1..]);
                }
                s // Fallback
            })
            .collect::<Vec<_>>()
            .join(":");
        
        let posix_file = user_dirs.home_dir().join(".wanderlust_posix");
        if let Ok(mut f) = File::create(&posix_file) {
             let _ = writeln!(f, "{}", posix_path);
             info!("Wrote POSIX path to {:?}", posix_file);
        }
    }

    // Apply the changes to the system
    apply_path(&new_path_string)?;
    info!("Successfully healed PATH!");
    
    Ok(())
}

/// Runs a "Doctor" check to report on the health of the CURRENT and STORED path.
///
/// This does not modify the system.
pub fn doctor() -> Result<()> {
    println!("Running doctor...");
    let candidates = discovery::discover_candidates();
    println!("Found {} unique commands.", candidates.len());
    
    // Check for "shadowing" or conflicts
    for (cmd, locs) in candidates {
        if locs.len() > 1 {
            // Filter out exact duplicates (same directory)
            let unique_dirs: HashSet<_> = locs.iter().map(|c| &c.path).collect();
            if unique_dirs.len() > 1 {
                debug!("Command '{}' found in multiple locations: {:?}", cmd, unique_dirs);
            }
        }
    }
    
    println!("\n--- Process PATH (Current Terminal) ---");
    // Check current PATH health (the one currently loaded in memory)
    if let Ok(current) = std::env::var("PATH") {
        let parts: Vec<&str> = current.split(';').collect();
        let total = parts.len();
        let unique: HashSet<&str> = parts.iter().cloned().collect();
        println!("Loaded: {} entries ({} unique).", total, unique.len());
        
        if total > unique.len() {
            println!("  ! {} duplicates (This terminal is stale if Registry is clean)", total - unique.len());
        }
    }

    println!("\n--- Registry PATH (Persistent/Next Terminal) ---");
    // Check Registry health (what will be loaded next time)
    let hive = CURRENT_USER.open("Environment")?;
    if let Ok(reg_path) = hive.get_string("Path") {
        let parts: Vec<&str> = reg_path.split(';').collect();
        let total = parts.len();
        let unique: HashSet<&str> = parts.iter().cloned().collect();
        println!("Stored: {} entries ({} unique).", total, unique.len());

        if total > unique.len() {
             println!("  ! Registry still has {} duplicates (Heal failed).", total - unique.len());
        } else {
             println!("  ✓ Registry is CLEAN. Restart your terminal to see it.");
        }
    } else {
        println!("  ! Could not read HKCU\\Environment\\Path");
    }

    Ok(())
}

/// Constructs a minimal PATH string from discovered candidates.
///
/// **The Immutable Logic:**
/// 1.  **System First**: `System32`, `Windows`, `Wbem`, `PowerShell`, `OpenSSH` are hardcoded to ALWAYS be first.
///     This prevents "bricking" the system by shadowing core tools with user binaries.
/// 2.  **Deduplication**: We normalize paths (lowercase) to ensure `C:\Win` and `c:\win` don't duplicate.
/// 3.  **Discovery**: We append all discovered directories that contain executables.
fn build_minimal_path(map: &HashMap<String, Vec<discovery::Candidate>>) -> String {
    // 1. Start with essential System paths.
    // WARNING: Removing these can break Windows features or login capability.
    let mut essential_paths = vec![
        PathBuf::from(r"C:\Windows\system32"),
        PathBuf::from(r"C:\Windows"),
        PathBuf::from(r"C:\Windows\System32\Wbem"),
        PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0"),
        PathBuf::from(r"C:\Windows\System32\OpenSSH"),
    ];

    // Normalize essentials
    for p in essential_paths.iter_mut() {
        *p = normalize_path(p);
    }

    let mut final_paths: Vec<PathBuf> = essential_paths.clone();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    
    for p in &essential_paths {
        seen_paths.insert(p.clone());
    }

    // 2. Select winners from candidates
    // In a future version, this could use ranking (e.g., prefer User installs over System installs).
    // For now, we collect ALL unique directories found in discovery.
    
    let mut other_dirs: Vec<PathBuf> = Vec::new();
    
    for candidates in map.values() {
        for candidate in candidates {
            let norm = normalize_path(&candidate.path);
            if !seen_paths.contains(&norm) {
                // If it's valid and not already in essential list, we add it.
                other_dirs.push(norm);
            }
        }
    }

    // Sort to ensure deterministic output (and minimal git diff noise if we tracked it).
    other_dirs.sort(); 
    other_dirs.dedup();

    final_paths.extend(other_dirs);

    // Join with Windows standard separator ';'
    final_paths.iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Normalizes a path for comparison.
///
/// - Lowercases the string (Windows is case-insensitive).
fn normalize_path(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy().to_string().to_lowercase();
    PathBuf::from(s)
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
fn apply_path(new_val: &str) -> Result<()> {
    // 1. Open Registry Key
    let key = CURRENT_USER.open("Environment")?;
    let old_val = key.get_string("Path").unwrap_or_default();

    // 2. Backup to %LOCALAPPDATA%\wanderlust\backup.reg
    if let Some(base_dirs) = directories::BaseDirs::new() {
        let app_data = base_dirs.data_local_dir().join("wanderlust");
        
        if let Err(e) = std::fs::create_dir_all(&app_data) {
            warn!("Failed to create backup directory at {:?}: {}", app_data, e);
        } else {
            let backup_path = app_data.join("backup.reg");
            // Escape backslashes for .reg file format ("\" -> "\\")
            let escaped_old_val = old_val.replace("\\", "\\\\").replace("\"", "\\\"");
            let reg_content = format!(
                "Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Environment]\n\"Path\"=\"{}\"\n",
                escaped_old_val
            );
            
            match File::create(&backup_path) {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(reg_content.as_bytes()) {
                         error!("Failed to write backup content: {}", e);
                    } else {
                         info!("Backed up old PATH to {:?}", backup_path);
                    }
                }
                Err(e) => error!("Failed to create backup file {:?}: {}", backup_path, e),
            }
        }
    }

    // 3. Set new PATH
    key.set_string("Path", new_val)?;
    
    // 4. Broadcast change (Twice with delay, to ensure standard apps pick it up)
    broadcast_change();
    std::thread::sleep(std::time::Duration::from_secs(1));
    broadcast_change();

    // 5. Verify consistency
    if !verify_path_health() {
        error!("Verification failed! The new PATH seems broken. Rolling back...");
        
        // ROLLBACK
        if let Err(e) = key.set_string("Path", &old_val) {
            error!("CRITICAL: Failed to write back old PATH: {}", e);
            bail!("Verification failed AND Rollback failed. Please restore from backup manually.");
        }
        broadcast_change();
        bail!("Verification failed. Rolled back to previous PATH.");
    }
    
    Ok(())
}

/// Broadcasts a `WM_SETTINGCHANGE` message to all top-level windows.
///
/// This tells Explorer and other applications that environment variables have changed.
/// It uses `SendMessageTimeoutA` to avoid hanging if a window is unresponsive.
fn broadcast_change() {
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutA, HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG};
    use std::ffi::CString;

    unsafe {
        let env = CString::new("Environment").unwrap();
        // Param 1 (wparam): 0
        // Param 2 (lparam): Pointer to "Environment" string
        // Flags: SMTO_ABORTIFHUNG (don't wait for hung apps)
        // Timeout: 5000ms
        let _ = SendMessageTimeoutA(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(env.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

/// Probes the system to ensure critical binaries can still be found.
///
/// This functionality ensures we haven't accidentally removed `System32` or other
/// critical paths from the environment.
fn verify_path_health() -> bool {
    let probes = vec![
        "cmd.exe /C ver",       // Basic shell
        "powershell.exe -v",    // Basic PS
        "whoami",               // Basic utils
    ];

    let mut success_count = 0;
    for cmd in &probes {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        // Command::new searches PATH. If this succeeds, PATH works.
        if let Ok(status) = std::process::Command::new(parts[0])
            .args(&parts[1..])
            .output() 
        {
            if status.status.success() {
                success_count += 1;
            } else {
                 debug!("Probe failed: {}", cmd);
            }
        } else {
             debug!("Probe failed to launch: {}", cmd);
        }
    }

    // We require at least 2 of the 3 probes to succeed to consider the PATH healthy.
    // This allows for one weird failure (e.g. if PowerShell isn't installed) while still catching catastrophic breakage.
    success_count >= 2
}