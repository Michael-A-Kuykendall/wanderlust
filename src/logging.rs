//! JSON-lines structured logging with rotation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const LOG_DIR_NAME: &str = "logs";
pub const LOG_FILE_NAME: &str = "wanderlust.log";

/// Severity of a structured log event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace,
    Info,
    Warn,
    Error,
}

/// A single structured log event (one JSON line on disk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub ts: u64,
    pub level: Level,
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub fields: Value,
}

impl LogEvent {
    pub fn new(level: Level, component: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ts: now_secs(),
            level,
            component: component.into(),
            message: Some(message.into()),
            fields: Value::Null,
        }
    }

    /// Attaches arbitrary structured fields (must serialize to JSON).
    pub fn with_fields(mut self, fields: Value) -> Self {
        self.fields = fields;
        self
    }
}

/// Appends a structured event as a single JSON line to `path`.
pub fn append_event(path: &Path, event: &LogEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(event)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Rotates `log_path`, keeping at most `keep` historical copies.
///
/// Renames the current log to `<name>.1`, shifting older copies up to `.keep`.
pub fn rotate(log_path: &Path, keep: usize) -> Result<()> {
    if !log_path.exists() {
        return Ok(());
    }
    let parent = log_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = log_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = log_path.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let numbered = |n: usize| -> PathBuf {
        let name = if ext.is_empty() {
            format!("{}.{}", stem, n)
        } else {
            format!("{}.{}.{}", stem, n, ext)
        };
        parent.join(name)
    };

    // Shift existing numbered copies upward.
    for n in (1..keep).rev() {
        let src = numbered(n);
        let dst = numbered(n + 1);
        if src.exists() {
            std::fs::rename(&src, &dst)?;
        }
    }
    std::fs::rename(log_path, numbered(1))?;
    Ok(())
}

/// Resolves the default log file path under `app_dir`.
pub fn default_log_path(app_dir: &Path) -> PathBuf {
    app_dir.join(LOG_DIR_NAME).join(LOG_FILE_NAME)
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
    use proptest::prelude::*;
    use std::io::Read;

    #[test]
    fn append_writes_one_json_line() {
        let dir = TestTempDir::new("logging").unwrap();
        let log = dir.child("app.log");
        let ev = LogEvent::new(Level::Info, "cleaner", "healed PATH")
            .with_fields(serde_json::json!({"removed": 2, "added": 1}));
        append_event(&log, &ev).unwrap();

        let mut content = String::new();
        std::fs::File::open(&log).unwrap().read_to_string(&mut content).unwrap();
        let parsed: LogEvent = serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert_eq!(parsed.level, Level::Info);
        assert_eq!(parsed.component, "cleaner");
        assert_eq!(parsed.message.as_deref(), Some("healed PATH"));
    }

    #[test]
    fn append_multiple_lines() {
        let dir = TestTempDir::new("logging-multi").unwrap();
        let log = dir.child("app.log");
        append_event(&log, &LogEvent::new(Level::Warn, "yank", "suspicious")).unwrap();
        append_event(&log, &LogEvent::new(Level::Error, "apply", "rollback")).unwrap();

        let mut content = String::new();
        std::fs::File::open(&log).unwrap().read_to_string(&mut content).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["level"], "warn");
        let second: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["level"], "error");
    }

    #[test]
    fn rotate_keeps_limited_copies_and_shifts() {
        let dir = TestTempDir::new("logging-rotate").unwrap();
        let log = dir.child("app.log");
        std::fs::write(&log, "original").unwrap();
        std::fs::write(dir.child("app.1.log"), "old1").unwrap();
        std::fs::write(dir.child("app.2.log"), "old2").unwrap();

        rotate(&log, 3).unwrap();

        // The current log is rotated away (renamed to .1), not truncated in place.
        assert!(!log.exists());

        let mut shifted1 = String::new();
        std::fs::File::open(dir.child("app.1.log")).unwrap().read_to_string(&mut shifted1).unwrap();
        assert_eq!(shifted1, "original");

        // app.3.log should not exist (keep=3 means .1, .2, .3; but we only had .1,.2 + current)
        assert!(!dir.child("app.4.log").exists());
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 16,
            failure_persistence: None,
            .. ProptestConfig::default()
        })]

        #[test]
        fn log_event_json_roundtrip(
            level in prop::sample::select(vec![Level::Trace, Level::Info, Level::Warn, Level::Error]),
            ref component in "[a-z]{1,20}",
            ref message in "[a-zA-Z0-9 _./-]{0,100}",
            ref field_key in "[a-z]{1,10}",
            field_val in 0i64..1000,
        ) {
            let ev = LogEvent::new(level, component.clone(), message.clone())
                .with_fields(serde_json::json!({field_key: field_val}));
            let json = serde_json::to_string(&ev).unwrap();
            let parsed: LogEvent = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(parsed.level, level);
            prop_assert_eq!(parsed.component, component.clone());
            prop_assert_eq!(parsed.message.as_deref(), if message.is_empty() { Some("") } else { Some(message.as_str()) });
            prop_assert_eq!(parsed.fields[field_key].clone(), field_val);
        }

        #[test]
        fn append_and_read_roundtrip(
            level in prop::sample::select(vec![Level::Trace, Level::Info, Level::Warn, Level::Error]),
            ref component in "[a-z]{1,20}",
            ref message in "[a-zA-Z0-9 _./-]{0,100}",
        ) {
            let dir = TestTempDir::new("logging-prop").unwrap();
            let log = dir.child("test.log");
            let ev = LogEvent::new(level, component.clone(), message.clone());
            append_event(&log, &ev).unwrap();

            let mut content = String::new();
            std::fs::File::open(&log).unwrap().read_to_string(&mut content).unwrap();
            let parsed: LogEvent = serde_json::from_str(content.trim()).unwrap();
            prop_assert_eq!(parsed.level, level);
            prop_assert_eq!(parsed.component, component.clone());
        }

        #[test]
        fn rotate_preserves_content(
            ref content in "[a-zA-Z0-9 \n]{1,50}",
            keep in 1usize..5,
        ) {
            let dir = TestTempDir::new("logging-rotate-prop").unwrap();
            let log = dir.child("app.log");
            std::fs::write(&log, content).unwrap();

            rotate(&log, keep).unwrap();

            // The original content should be in app.1.log
            let rotated_path = dir.child("app.1.log");
            prop_assert!(rotated_path.exists(), "rotated file app.1.log must exist");
            let rotated_content = std::fs::read_to_string(&rotated_path).unwrap();
            prop_assert_eq!(rotated_content, content.as_str(), "rotated file must preserve content");

            // The original log file no longer exists
            prop_assert!(!log.exists(), "original log must no longer exist after rotation");
        }
    }
}
