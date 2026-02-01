use std::path::{Path, PathBuf};
use std::collections::HashMap;
use anyhow::Result;
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

/// Abstraction for System interactions (Registry, File System, Environment).
/// This allows us to mock the dangerous Windows Registry interactions for testing.
pub trait SystemOps {
    /// Read the current PATH from the Registry (User scope).
    fn read_user_path_registry(&self) -> Result<String>;
    
    /// Write the new PATH to the Registry (User scope).
    fn write_user_path_registry(&self, new_path: &str) -> Result<()>;
    
    /// Broadcast the "Environment Changed" message to the system.
    fn broadcast_environment_change(&self) -> Result<()>;
    
    /// Check if a directory exists on the file system.
    fn path_exists(&self, path: &Path) -> bool;

    /// Write a backup file to disk.
    fn write_backup_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Run system verification probes (cmd, powershell) to ensure PATH is valid.
    fn verify_environment_health(&self) -> bool;

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
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutA, HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG};
        use windows::Win32::Foundation::{LPARAM, WPARAM};

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
        let probes = vec![
            "cmd.exe /C ver",
            "powershell.exe -v",
            "whoami",
        ];
    
        let mut success_count = 0;
        for cmd in &probes {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if let Ok(status) = std::process::Command::new(parts[0])
                .args(&parts[1..])
                .output() 
            {
                if status.status.success() {
                    success_count += 1;
                }
            }
        }
    
        success_count >= 2
    }

    fn read_system_path_registry(&self) -> Result<String> {
        let key = LOCAL_MACHINE.open(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")?;
        let path_val = key.get_string("Path")?;
        Ok(path_val)
    }

    fn write_system_path_registry(&self, new_path: &str) -> Result<()> {
        let key = LOCAL_MACHINE.create(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")?;
        key.set_string("Path", new_path)?;
        Ok(())
    }
}

/// A Mock System for Testing.
#[derive(Debug, Default)]
pub struct MockSystem {
    pub registry: std::sync::Mutex<HashMap<String, String>>,
    pub file_system: std::sync::Mutex<Vec<PathBuf>>,
    pub broadcast_called: std::sync::Mutex<bool>,
}

impl MockSystem {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn with_registry(key: &str, value: &str) -> Self {
        let mut map = HashMap::new();
        map.insert(key.to_string(), value.to_string());
        Self {
            registry: std::sync::Mutex::new(map),
            ..Default::default()
        }
    }
}

impl SystemOps for MockSystem {
    fn read_user_path_registry(&self) -> Result<String> {
        let map = self.registry.lock().unwrap();
        map.get("Path")
           .cloned()
           .ok_or_else(|| anyhow::anyhow!("Path not found in mock registry"))
    }

    fn write_user_path_registry(&self, new_path: &str) -> Result<()> {
        let mut map = self.registry.lock().unwrap();
        map.insert("Path".to_string(), new_path.to_string());
        Ok(())
    }

    fn broadcast_environment_change(&self) -> Result<()> {
        let mut called = self.broadcast_called.lock().unwrap();
        *called = true;
        Ok(())
    }

    fn path_exists(&self, path: &Path) -> bool {
        let fs = self.file_system.lock().unwrap();
        fs.contains(&path.to_path_buf())
    }

    fn write_backup_file(&self, path: &Path, _content: &str) -> Result<()> {
        let mut fs = self.file_system.lock().unwrap();
        fs.push(path.to_path_buf());
        Ok(())
    }

    fn verify_environment_health(&self) -> bool {
        true
    }

    fn read_system_path_registry(&self) -> Result<String> {
        let map = self.registry.lock().unwrap();
        map.get("SystemPath")
           .cloned()
           .ok_or_else(|| anyhow::anyhow!("SystemPath not found in mock registry"))
    }

    fn write_system_path_registry(&self, new_path: &str) -> Result<()> {
        let mut map = self.registry.lock().unwrap();
        map.insert("SystemPath".to_string(), new_path.to_string());
        Ok(())
    }
}
