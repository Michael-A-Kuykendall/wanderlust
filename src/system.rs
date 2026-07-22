use anyhow::Result;
use std::path::Path;
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

/// Abstraction for System interactions (Registry, File System, Environment).
/// This allows production code to be isolated from dangerous Windows Registry interactions.
pub trait SystemOps {
    /// Read the current PATH from the Registry (User scope).
    fn read_user_path_registry(&self) -> Result<String>;

    /// Write the new PATH to the Registry (User scope).
    fn write_user_path_registry(&self, new_path: &str) -> Result<()>;

    /// Restore a previously read User PATH value. The default preserves the existing write behavior.
    fn restore_user_path_registry(&self, old_path: &str) -> Result<()> {
        self.write_user_path_registry(old_path)
    }

    /// Broadcast the "Environment Changed" message to the system.
    fn broadcast_environment_change(&self) -> Result<()>;

    /// Check if a directory exists on the file system.
    fn path_exists(&self, path: &Path) -> bool;

    /// Write a backup file to disk.
    fn write_backup_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Run system verification probes (cmd, powershell) to ensure PATH is valid.
    fn verify_environment_health(&self) -> bool;

    /// Verify a planned effective PATH.
    fn verify_effective_path(&self, _effective_path: &str) -> Result<()> {
        if self.verify_environment_health() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("environment health verification failed"))
        }
    }

    /// Read the System PATH from the Registry (Machine scope - HKLM).
    fn read_system_path_registry(&self) -> Result<String>;

    /// Write the System PATH to the Registry (Machine scope - HKLM).
    /// Requires Admin privileges.
    fn write_system_path_registry(&self, new_path: &str) -> Result<()>;
}

/// The Real System implementation (Production).
pub struct WindowsSystem;

impl SystemOps for WindowsSystem {
    fn read_user_path_registry(&self) -> Result<String> {
        let key = CURRENT_USER.open("Environment")?;
        let path_val = key.get_string("Path")?;
        Ok(path_val)
    }

    fn write_user_path_registry(&self, new_path: &str) -> Result<()> {
        let key = CURRENT_USER.create("Environment")?;
        key.set_string("Path", new_path)?;
        Ok(())
    }

    fn broadcast_environment_change(&self) -> Result<()> {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutA, WM_SETTINGCHANGE,
        };

        unsafe {
            let env_str = std::ffi::CString::new("Environment").unwrap();
            let mut result: usize = 0;
            SendMessageTimeoutA(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(env_str.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                Some(&mut result),
            );
        }
        Ok(())
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn write_backup_file(&self, path: &Path, content: &str) -> Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        f.write_all(content.as_bytes())?;
        Ok(())
    }

    fn verify_environment_health(&self) -> bool {
        let probes = vec!["cmd.exe /C ver", "powershell.exe -v", "whoami"];

        let mut success_count = 0;
        for cmd in &probes {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            let status = std::process::Command::new(parts[0])
                .args(&parts[1..])
                .output();
            if status.map(|s| s.status.success()).unwrap_or(false) {
                success_count += 1;
            }
        }

        success_count >= 2
    }

    fn read_system_path_registry(&self) -> Result<String> {
        let key =
            LOCAL_MACHINE.open(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")?;
        let path_val = key.get_string("Path")?;
        Ok(path_val)
    }

    fn write_system_path_registry(&self, new_path: &str) -> Result<()> {
        let key = LOCAL_MACHINE
            .create(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")?;
        key.set_string("Path", new_path)?;
        Ok(())
    }
}

/// Test-only system operation failure points.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MockOperation {
    ReadUserPath,
    WriteUserPath,
    RestoreUserPath,
    BroadcastEnvironmentChange,
    WriteBackup,
    VerifyEnvironment,
    VerifyEffectivePath,
    ReadSystemPath,
    WriteSystemPath,
}

/// Test-only record of a call made through [`MockSystem`].
#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MockCall {
    ReadUserPath,
    WriteUserPath(String),
    RestoreUserPath(String),
    BroadcastEnvironmentChange,
    WriteBackup {
        path: std::path::PathBuf,
        content: String,
    },
    VerifyEnvironment,
    VerifyEffectivePath(String),
    ReadSystemPath,
    WriteSystemPath(String),
}

/// A configurable, in-memory SystemOps implementation for isolated unit tests.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockSystem {
    pub registry: std::sync::Mutex<std::collections::HashMap<String, String>>,
    pub file_system: std::sync::Mutex<Vec<std::path::PathBuf>>,
    pub broadcast_called: std::sync::Mutex<bool>,
    pub(crate) calls: std::sync::Mutex<Vec<MockCall>>,
    pub(crate) failures: std::sync::Mutex<
        std::collections::HashMap<MockOperation, std::collections::BTreeSet<usize>>,
    >,
    pub(crate) operation_counts: std::sync::Mutex<std::collections::HashMap<MockOperation, usize>>,
}

#[cfg(test)]
impl MockSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_registry(key: &str, value: &str) -> Self {
        let mut map = std::collections::HashMap::new();
        map.insert(key.to_string(), value.to_string());
        Self {
            registry: std::sync::Mutex::new(map),
            ..Default::default()
        }
    }

    /// Fail the next invocation of `operation`.
    pub fn fail_next(&self, operation: MockOperation) {
        let next = self
            .operation_counts
            .lock()
            .unwrap()
            .get(&operation)
            .copied()
            .unwrap_or(0)
            + 1;
        self.fail_on_nth(operation, next);
    }

    /// Fail a specific one-based invocation of `operation`.
    pub fn fail_on_nth(&self, operation: MockOperation, invocation: usize) {
        assert!(invocation > 0, "mock operation invocations are one-based");
        self.failures
            .lock()
            .unwrap()
            .entry(operation)
            .or_default()
            .insert(invocation);
    }

    pub fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().unwrap().clone()
    }

    pub fn call_count(&self, operation: MockOperation) -> usize {
        self.operation_counts
            .lock()
            .unwrap()
            .get(&operation)
            .copied()
            .unwrap_or(0)
    }

    fn record(&self, call: MockCall) {
        self.calls.lock().unwrap().push(call);
    }

    fn fail_if_configured(&self, operation: MockOperation) -> Result<()> {
        let invocation = {
            let mut counts = self.operation_counts.lock().unwrap();
            let count = counts.entry(operation).or_insert(0);
            *count += 1;
            *count
        };
        if self
            .failures
            .lock()
            .unwrap()
            .get(&operation)
            .is_some_and(|calls| calls.contains(&invocation))
        {
            Err(anyhow::anyhow!(
                "configured mock failure for {operation:?} invocation {invocation}"
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
impl SystemOps for MockSystem {
    fn read_user_path_registry(&self) -> Result<String> {
        self.record(MockCall::ReadUserPath);
        self.fail_if_configured(MockOperation::ReadUserPath)?;
        let map = self.registry.lock().unwrap();
        map.get("Path")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Path not found in mock registry"))
    }

    fn write_user_path_registry(&self, new_path: &str) -> Result<()> {
        self.record(MockCall::WriteUserPath(new_path.to_string()));
        self.fail_if_configured(MockOperation::WriteUserPath)?;
        self.registry
            .lock()
            .unwrap()
            .insert("Path".to_string(), new_path.to_string());
        Ok(())
    }

    fn restore_user_path_registry(&self, old_path: &str) -> Result<()> {
        self.record(MockCall::RestoreUserPath(old_path.to_string()));
        self.fail_if_configured(MockOperation::RestoreUserPath)?;
        self.registry
            .lock()
            .unwrap()
            .insert("Path".to_string(), old_path.to_string());
        Ok(())
    }

    fn broadcast_environment_change(&self) -> Result<()> {
        self.record(MockCall::BroadcastEnvironmentChange);
        self.fail_if_configured(MockOperation::BroadcastEnvironmentChange)?;
        *self.broadcast_called.lock().unwrap() = true;
        Ok(())
    }

    fn path_exists(&self, path: &Path) -> bool {
        self.file_system
            .lock()
            .unwrap()
            .contains(&path.to_path_buf())
    }

    fn write_backup_file(&self, path: &Path, content: &str) -> Result<()> {
        self.record(MockCall::WriteBackup {
            path: path.to_path_buf(),
            content: content.to_string(),
        });
        self.fail_if_configured(MockOperation::WriteBackup)?;
        self.file_system.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }

    fn verify_environment_health(&self) -> bool {
        self.record(MockCall::VerifyEnvironment);
        self.fail_if_configured(MockOperation::VerifyEnvironment)
            .is_ok()
    }

    fn verify_effective_path(&self, effective_path: &str) -> Result<()> {
        self.record(MockCall::VerifyEffectivePath(effective_path.to_string()));
        self.fail_if_configured(MockOperation::VerifyEffectivePath)
    }

    fn read_system_path_registry(&self) -> Result<String> {
        self.record(MockCall::ReadSystemPath);
        self.fail_if_configured(MockOperation::ReadSystemPath)?;
        let map = self.registry.lock().unwrap();
        map.get("SystemPath")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("SystemPath not found in mock registry"))
    }

    fn write_system_path_registry(&self, new_path: &str) -> Result<()> {
        self.record(MockCall::WriteSystemPath(new_path.to_string()));
        self.fail_if_configured(MockOperation::WriteSystemPath)?;
        self.registry
            .lock()
            .unwrap()
            .insert("SystemPath".to_string(), new_path.to_string());
        Ok(())
    }
}

/// An automatically removed, process-unique temporary directory for isolated tests.
#[cfg(test)]
#[derive(Debug)]
pub struct TestTempDir {
    path: std::path::PathBuf,
}

#[cfg(test)]
impl TestTempDir {
    pub fn new(label: &str) -> std::io::Result<Self> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        let label: String = label
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("wanderlust-{label}-{}-{id}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn child(&self, name: impl AsRef<Path>) -> std::path::PathBuf {
        self.path.join(name)
    }
}

#[cfg(test)]
impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_system_records_configured_failure_points_and_inputs() {
        let system = MockSystem::with_registry("Path", "C:\\old");
        system.fail_on_nth(MockOperation::RestoreUserPath, 1);

        assert_eq!(system.read_user_path_registry().unwrap(), "C:\\old");
        system.write_user_path_registry("C:\\new").unwrap();
        system.broadcast_environment_change().unwrap();
        system.verify_effective_path("C:\\system;C:\\new").unwrap();
        assert!(system.restore_user_path_registry("C:\\old").is_err());

        assert_eq!(
            system.calls(),
            vec![
                MockCall::ReadUserPath,
                MockCall::WriteUserPath("C:\\new".to_string()),
                MockCall::BroadcastEnvironmentChange,
                MockCall::VerifyEffectivePath("C:\\system;C:\\new".to_string()),
                MockCall::RestoreUserPath("C:\\old".to_string()),
            ]
        );
    }

    #[test]
    fn temporary_directory_is_unique_and_removed_on_drop() {
        let path = {
            let temp_dir = TestTempDir::new("system seam").unwrap();
            let cache_file = temp_dir.child("cache.stage");
            std::fs::write(&cache_file, "staged").unwrap();
            assert!(cache_file.exists());
            temp_dir.path().to_path_buf()
        };

        assert!(!path.exists());
    }
}
