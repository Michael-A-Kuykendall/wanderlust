//! # Discovery Module
//!
//! This module is responsible for the "Heuristic Discovery" phase of Wanderlust.
//! Instead of relying solely on what the user has manually added to their PATH,
//! Wanderlust actively crawls the system to find tools that *should* be available.
//!
//! ## Discovery Strategies
//!
//! 1.  **Registry Scanning**: Checks `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall`
//!     to find installation locations of software (e.g., VS Code, Node.js).
//! 2.  **Common Locations**: Checks "Well Known" paths like `~/.cargo/bin`, `~/.local/bin`,
//!     and Scoop shims.
//! 3.  **Existing PATH**: Ingests the current PATH to ensure we don't lose any manual configurations.
//!
//! The result is a unified map of `Command Name -> List of Directories`.

use std::collections::HashMap;
use std::path::PathBuf;
use walkdir::WalkDir;
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
use log::debug;

/// Represents a potential location for a specific command.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The directory containing the executable.
    pub path: PathBuf,
    /// The origin of this discovery (e.g., "scoop", "registry", "cargo").
    /// This is currently used for debugging but will drive ranking logic in v2.0.
    pub _source: String, 
}

/// The main entry point for discovery.
///
/// Scans the system using multiple strategies and returns a map where:
/// - **Key**: The executable name (lowercase, e.g., "node", "cargo").
/// - **Value**: A list of directories where this executable was found.
///
/// Use this map to construct a new PATH or to detect conflicts (shadowing).
pub fn discover_candidates() -> HashMap<String, Vec<Candidate>> {
    let mut map: HashMap<String, Vec<Candidate>> = HashMap::new();

    // 1. Scan Registry for installed programs
    scan_registry_uninstall(&mut map);

    // 2. Scan Common Locations (heuristic)
    scan_common_locations(&mut map);

    // 3. Scan existing PATH (to not lose what we already have, just clean it)
    scan_existing_path(&mut map);

    map
}

/// Scans the Windows Registry for installed applications.
///
/// Looks at `HKCU` and `HKLM` `Software\Microsoft\Windows\CurrentVersion\Uninstall` for `InstallLocation` keys.
/// If a `bin` directory exists inside the install location, that is preferred.
fn scan_registry_uninstall(map: &mut HashMap<String, Vec<Candidate>>) {
    let key_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    
    // Check both HKCU (Current User) and HKLM (Local Machine / System-wide)
    let hives = [
        (CURRENT_USER, "HKCU_Uninstall"), 
        (LOCAL_MACHINE, "HKLM_Uninstall")
    ];

    for (hive, source_label) in hives {
        if let Ok(uninstall_key) = hive.open(key_path) {
            for subkey_name in uninstall_key.keys().into_iter().flatten() {
                if let Ok(subkey) = uninstall_key.open(&subkey_name) {
                    // Try "InstallLocation"
                    if let Some(install_loc) = subkey.get_string("InstallLocation").ok().filter(|s| !s.is_empty()) {
                        let path = PathBuf::from(&install_loc);
                        // Heuristic: check if there's a 'bin' folder, otherwise use root
                        let bin_path = path.join("bin");
                        if bin_path.exists() {
                            add_dir_candidates(map, &bin_path, source_label);
                        } else if path.exists() {
                            add_dir_candidates(map, &path, source_label);
                        }
                    }
                }
            }
        }
    }
}

/// Scans "well-known" directories that developers commonly use.
///
/// Currently supports:
/// - Cargo (`~/.cargo/bin`)
/// - Local User Bin (`~/.local/bin`)
/// - Scoop Shims (`~/scoop/shims`)
fn scan_common_locations(map: &mut HashMap<String, Vec<Candidate>>) {
    if let Some(user_profile) = directories::UserDirs::new() {
        let home = user_profile.home_dir();
        
        // Cargo
        let cargo_bin = home.join(".cargo").join("bin");
        if cargo_bin.exists() {
            add_dir_candidates(map, &cargo_bin, "cargo");
        }

        // Local bin
        let local_bin = home.join(".local").join("bin");
        if local_bin.exists() {
            add_dir_candidates(map, &local_bin, "local_bin");
        }
        
        // Scoop shims
        let scoop_shims = home.join("scoop").join("shims");
        if scoop_shims.exists() {
             add_dir_candidates(map, &scoop_shims, "scoop");
        }
    }
    
    // Add more predictable locations here (Program Files, etc) if needed, 
    // though Registry scan covers most "installed" things.
}

/// Scans the current environment variable `PATH`.
///
/// This ensures that even if we don't heuristically find a tool,
/// if the user had it in their PATH before, we preserve it.
fn scan_existing_path(map: &mut HashMap<String, Vec<Candidate>>) {
    if let Ok(path_var) = std::env::var("PATH") {
        for part in path_var.split(';') {
            if part.is_empty() { continue; }
            let path = PathBuf::from(part);
            if path.exists() {
                add_dir_candidates(map, &path, "existing_path");
            }
        }
    }
}

/// Helper function to scan a specific directory for executables.
///
/// Adds any found `.exe`, `.cmd`, `.bat`, or `.com` files to the candidate map.
/// This function is shallow (depth 1) generally, to avoid massive crawls.
fn add_dir_candidates(map: &mut HashMap<String, Vec<Candidate>>, dir: &PathBuf, source: &str) {
    debug!("Scanning directory: {:?}", dir);
    // Only go 1 level deep
    let walker = WalkDir::new(dir).max_depth(1);
    
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let (Some(stem), Some(ext)) = (path.file_stem(), path.extension()) {
            let ext_str = ext.to_string_lossy().to_lowercase();
            // We only care about executables for Windows
            if ext_str == "exe" || ext_str == "cmd" || ext_str == "bat" || ext_str == "com" {
                let cmd_name = stem.to_string_lossy().to_lowercase();
                
                // Add to map
                map.entry(cmd_name).or_default().push(Candidate {
                    path: dir.to_path_buf(), // Store the *directory* containing the tool
                    _source: source.to_string(),
                });
            }
        }
    }
}
