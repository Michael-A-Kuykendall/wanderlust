//! UAC privilege check and admin relanch via ShellExecute "runas".

use log::info;
use std::ffi::CString;
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteA;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

pub fn is_elevated() -> bool {
    let mut token = windows::Win32::Foundation::HANDLE::default();
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
            let mut elevation = TOKEN_ELEVATION::default();
            let mut size = 0;
            // GetTokenInformation is the Win32 API to read struct data from a token handle.
            if GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut _),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            )
            .is_ok()
            {
                return elevation.TokenIsElevated != 0;
            }
        }
    }
    false
}

pub fn relaunch_as_admin() -> bool {
    if let Ok(exe_path) = std::env::current_exe() {
        // Safe conversion handling null bytes
        let exe_path_str = match CString::new(exe_path.to_string_lossy().as_bytes()) {
            Ok(s) => s,
            Err(_) => return false,
        };

        // Reconstruct the command line arguments
        let args: Vec<String> = std::env::args().skip(1).collect();
        let args_str = match CString::new(args.join(" ")) {
            Ok(s) => s,
            Err(_) => return false,
        };

        info!("Relaunching as admin: {:?} {:?}", exe_path, args);

        let operation = CString::new("runas").unwrap();

        unsafe {
            // ShellExecuteA executes an operation on a specified file.
            // "runas" is the magic verb that requests elevation.
            let result = ShellExecuteA(
                None, // Parent window (None = Desktop)
                windows::core::PCSTR(operation.as_ptr() as *const _),
                windows::core::PCSTR(exe_path_str.as_ptr() as *const _),
                windows::core::PCSTR(args_str.as_ptr() as *const _),
                windows::core::PCSTR(std::ptr::null()), // Working directory (NULL = current)
                SW_SHOW,                                // Show command normally
            );

            // ShellExecute returns an HINSTANCE > 32 on success.
            // Values <= 32 are error codes (e.g. SE_ERR_ACCESSDENIED).
            if result.0 as isize > 32 {
                return true;
            }
        }
    }
    false
}
