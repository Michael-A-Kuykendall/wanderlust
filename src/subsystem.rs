//! Protects WSL/Cygwin/MSYS2 PATH entries from removal during healing.

use std::collections::HashSet;

/// Prefixes that mark a PATH entry as owned by / shared with a POSIX subsystem.
const PROTECTED_PREFIXES: &[&str] = &[
    "\\\\wsl$",
    "\\\\wsl.localhost",
    "cygwin",
    "cygdrive",
    "/cygdrive",
    "/mnt/",
    "msys",
    "git\\usr",
    "git\\bin",
    "git\\cmd",
];

/// Logic for protecting POSIX-subsystem PATH entries during healing.
#[derive(Debug, Clone, Default)]
pub struct SubsystemSafety {
    /// Extra user-supplied prefixes that must never be removed.
    extra_prefixes: Vec<String>,
}

impl SubsystemSafety {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an extra protected prefix (e.g. a custom WSL distro mount root).
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.extra_prefixes.push(prefix.into());
        self
    }

    /// Returns true if `entry` should be preserved (never auto-removed) because
    /// it is owned by or shared with a POSIX subsystem.
    pub fn should_preserve(&self, entry: &str) -> bool {
        let lower = entry.to_lowercase();
        for p in PROTECTED_PREFIXES {
            if lower.contains(&p.to_lowercase()) {
                return true;
            }
        }
        for p in &self.extra_prefixes {
            if lower.contains(&p.to_lowercase()) {
                return true;
            }
        }
        false
    }

    /// Filters a candidate PATH, returning only the entries that are safe to
    /// remove (i.e. not subsystem-protected).
    pub fn removable_entries<'a>(&self, entries: &'a [String]) -> Vec<&'a String> {
        entries
            .iter()
            .filter(|e| !self.should_preserve(e))
            .collect()
    }
}

/// Extracts Windows-style directory paths from a raw WSL `$PATH` string.
///
/// WSL `$PATH` may contain:
/// * `/mnt/c/Users/...`  (DrvFS mounts)
/// * `/c/Users/...`      (some distros)
/// * Native Windows paths like `C:\Users\...`
///
/// Returns the set of lowercased Windows root paths that WSL can currently see.
pub fn parse_wsl_path_output(raw: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut tokens: Vec<String> = Vec::new();
    for part in raw.split(':') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // Rejoin a drive letter token with its path component.
        if !tokens.is_empty() {
            let last = tokens.last().unwrap();
            let merge = last.len() == 1
                && last.chars().next().unwrap().is_ascii_alphabetic()
                && part.starts_with('\\');
            if merge {
                let last_mut = tokens.last_mut().unwrap();
                *last_mut = format!("{}:{}", last_mut, part);
                continue;
            }
        }
        tokens.push(part.to_string());
    }
    for part in &tokens {
        if let Some(win) = wsl_part_to_windows(part) {
            out.insert(win);
        }
    }
    out
}

/// Converts a single WSL path component to a Windows root path if applicable.
fn wsl_part_to_windows(part: &str) -> Option<String> {
    let lowered = part.to_lowercase();
    // /mnt/c/foo -> c:\foo
    if let Some(rest) = lowered.strip_prefix("/mnt/") {
        let well_formed = rest.len() >= 2 && rest.chars().nth(1) == Some('/');
        if well_formed {
            let drive = rest.chars().next().unwrap();
            return Some(format!("{}:\\{}", drive, &rest[2..]).replace('/', "\\"));
        }
    }
    // /c/foo -> c:\foo  (note: skip the leading slash at index 2)
    if lowered.starts_with('/') && lowered.len() >= 3 {
        let chars: Vec<char> = lowered.chars().collect();
        if chars[1].is_ascii_alphabetic() && chars[2] == '/' {
            let drive = chars[1];
            return Some(format!("{}:\\{}", drive, &lowered[3..]).replace('/', "\\"));
        }
    }
    // Already a Windows path.
    if part.contains('\\') && part.chars().nth(1) == Some(':') {
        return Some(part.to_lowercase());
    }
    None
}

/// Probes a live WSL instance for the Windows paths it currently surfaces.
///
/// Runs `wsl.exe echo $PATH` and parses the output. Returns `None` if WSL is
/// unavailable or the probe fails (callers should treat `None` as "no extra
/// constraints").
pub fn probe_wsl_path() -> Option<HashSet<String>> {
    let output = std::process::Command::new("wsl.exe").arg("echo").arg("$PATH").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let parsed = parse_wsl_path_output(&raw);
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn protected_prefixes_are_preserved() {
        let safety = SubsystemSafety::new();
        assert!(safety.should_preserve(r"\\wsl$\Ubuntu\bin"));
        assert!(safety.should_preserve(r"C:\cygwin64\bin"));
        assert!(safety.should_preserve(r"C:\Program Files\Git\usr\bin"));
        assert!(!safety.should_preserve(r"C:\Users\me\tools"));
    }

    #[test]
    fn extra_prefix_is_protected() {
        let safety = SubsystemSafety::new().with_prefix(r"\\my-nas\share");
        assert!(safety.should_preserve(r"\\my-nas\share\app"));
    }

    #[test]
    fn removable_entries_skips_protected() {
        let safety = SubsystemSafety::new();
        let entries = vec![
            r"C:\Users\me\tools".to_string(),
            r"\\wsl$\Ubuntu\bin".to_string(),
        ];
        let removable: Vec<&String> = safety.removable_entries(&entries);
        assert_eq!(removable.len(), 1);
        assert_eq!(removable[0], &r"C:\Users\me\tools".to_string());
    }

    #[test]
    fn parse_wsl_mnt_paths() {
        let parsed = parse_wsl_path_output("/mnt/c/Windows/System32:/mnt/d/tools:/usr/bin");
        assert!(parsed.contains(r"c:\windows\system32"));
        assert!(parsed.contains(r"d:\tools"));
        assert!(!parsed.contains("/usr/bin"));
    }

    #[test]
    fn parse_wsl_drive_letter_paths() {
        let parsed = parse_wsl_path_output("/c/Users/me:/d/Data");
        assert!(parsed.contains(r"c:\users\me"));
        assert!(parsed.contains(r"d:\data"));
    }

    #[test]
    fn parse_wsl_native_windows_paths() {
        // WSL $PATH uses ':' separators; a drive-colon is rejoined by the parser.
        let parsed = parse_wsl_path_output(r"C:\Python39:D:\Apps");
        assert!(parsed.contains(r"c:\python39"));
        assert!(parsed.contains(r"d:\apps"));
    }

    proptest! {
        #[test]
        fn parse_wsl_path_output_never_panics(ref raw in "\\PC*") {
            // Must never panic, regardless of input.
            let result = parse_wsl_path_output(raw);
            // All returned paths should look like valid Windows paths.
            for path in &result {
                let bytes = path.as_bytes();
                // Basic shape: <letter>:\\...
                prop_assert!(path.len() >= 3, "path '{}' too short", path);
                prop_assert!(
                    bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\',
                    "extracted '{}' is not a valid Windows path", path
                );
            }
        }

        #[test]
        fn should_preserve_is_deterministic(
            ref entry in "\\PC{1,100}",
            ref extra_prefix in "\\PC{0,20}",
        ) {
            let safety = if extra_prefix.is_empty() {
                SubsystemSafety::new()
            } else {
                SubsystemSafety::new().with_prefix(extra_prefix.clone())
            };
            // Deterministic: same entry always same result
            let v1 = safety.should_preserve(entry);
            let v2 = safety.should_preserve(entry);
            prop_assert_eq!(v1, v2, "should_preserve must be deterministic");
        }

        #[test]
        fn removable_entries_is_subset(
            ref entries in prop::collection::vec("\\PC{1,50}", 0..10),
        ) {
            let safety = SubsystemSafety::new();
            let removable = safety.removable_entries(entries);
            // Every removable entry must be in the original list
            for r in &removable {
                prop_assert!(
                    entries.iter().any(|e| e == *r),
                    "removable entry '{}' not in original", r
                );
            }
            // Count: removable + preserved = total
            let preserved: Vec<&String> = entries.iter().filter(|e| safety.should_preserve(e)).collect();
            prop_assert_eq!(removable.len() + preserved.len(), entries.len(),
                "removable + preserved must equal total entries");
        }
    }
}
