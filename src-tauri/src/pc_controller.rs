use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcActionResult {
    pub success: bool,
    pub message: String,
    pub action_type: String,
}

pub struct PcController;

impl PcController {
    /// Resolve desktop path on Windows
    fn get_desktop_dir() -> PathBuf {
        std::env::var("USERPROFILE")
            .map(|p| PathBuf::from(p).join("Desktop"))
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Minimize the AURA window or active applications
    pub fn minimize_window(app_handle: &AppHandle) -> PcActionResult {
        if let Some(window) = app_handle.get_webview_window("main") {
            match window.minimize() {
                Ok(_) => {
                    crate::logger::log("PC_ACTION", "SUCCESS", "Minimized AURA window.");
                    PcActionResult {
                        success: true,
                        message: "Minimized window successfully.".to_string(),
                        action_type: "minimize".to_string(),
                    }
                }
                Err(e) => PcActionResult {
                    success: false,
                    message: format!("Failed to minimize: {}", e),
                    action_type: "minimize".to_string(),
                },
            }
        } else {
            PcActionResult {
                success: false,
                message: "Main window not found.".to_string(),
                action_type: "minimize".to_string(),
            }
        }
    }

    /// Restore or unminimize window
    pub fn unminimize_window(app_handle: &AppHandle) -> PcActionResult {
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            crate::logger::log("PC_ACTION", "SUCCESS", "Restored AURA window.");
            PcActionResult {
                success: true,
                message: "Restored window.".to_string(),
                action_type: "unminimize".to_string(),
            }
        } else {
            PcActionResult {
                success: false,
                message: "Main window not found.".to_string(),
                action_type: "unminimize".to_string(),
            }
        }
    }

    /// Create a new folder on the Desktop or at a target path
    pub fn create_folder(folder_name: &str, target_path: Option<&str>) -> PcActionResult {
        let base_dir = match target_path {
            Some(p) if !p.trim().is_empty() => PathBuf::from(p),
            _ => Self::get_desktop_dir(),
        };

        let new_folder_path = base_dir.join(folder_name.trim());
        match fs::create_dir_all(&new_folder_path) {
            Ok(_) => {
                let msg = format!("Created folder '{}' at {:?}", folder_name, new_folder_path);
                crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                PcActionResult {
                    success: true,
                    message: msg,
                    action_type: "create_folder".to_string(),
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to create folder '{}': {}", folder_name, e);
                crate::logger::log("PC_ACTION", "ERROR", &err_msg);
                PcActionResult {
                    success: false,
                    message: err_msg,
                    action_type: "create_folder".to_string(),
                }
            }
        }
    }

    /// Open an application or folder in Windows Explorer
    pub fn open_path_or_app(target: &str) -> PcActionResult {
        let trimmed = target.trim();
        let path = Path::new(trimmed);
        
        let mut cmd = if path.exists() {
            let mut c = Command::new("explorer.exe");
            c.arg(trimmed);
            c
        } else {
            // Attempt to start program via cmd /C start
            let mut c = Command::new("cmd.exe");
            c.args(["/C", "start", "", trimmed]);
            c
        };

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.spawn() {
            Ok(_) => {
                let msg = format!("Opened '{}'", trimmed);
                crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                PcActionResult {
                    success: true,
                    message: msg,
                    action_type: "open".to_string(),
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to open '{}': {}", trimmed, e);
                crate::logger::log("PC_ACTION", "ERROR", &err_msg);
                PcActionResult {
                    success: false,
                    message: err_msg,
                    action_type: "open".to_string(),
                }
            }
        }
    }

    /// Execute a safe command requested by voice (e.g. "open notepad", "minimize", "make folder notes")
    pub fn execute_voice_command(app_handle: &AppHandle, cmd_str: &str) -> Option<PcActionResult> {
        let lower = cmd_str.to_lowercase();
        
        if lower.contains("minimize") {
            return Some(Self::minimize_window(app_handle));
        }

        if lower.contains("unminimize") || lower.contains("restore window") {
            return Some(Self::unminimize_window(app_handle));
        }

        if lower.contains("create folder") || lower.contains("make a folder") || lower.contains("new folder") {
            // Extract folder name
            let name = if let Some(idx) = lower.find("named") {
                cmd_str[idx + 5..].trim().trim_matches('"').trim_matches('\'')
            } else if let Some(idx) = lower.find("folder") {
                let sub = cmd_str[idx + 6..].trim();
                let candidate = sub.trim_start_matches("called").trim_start_matches("named").trim();
                if candidate.is_empty() { "New Folder" } else { candidate }
            } else {
                "New Folder"
            };
            return Some(Self::create_folder(name, None));
        }

        if lower.contains("open notepad") {
            return Some(Self::open_path_or_app("notepad.exe"));
        }

        if lower.contains("open calculator") || lower.contains("calc") {
            return Some(Self::open_path_or_app("calc.exe"));
        }

        if lower.contains("open explorer") || lower.contains("open my desktop") {
            let desktop = Self::get_desktop_dir();
            return Some(Self::open_path_or_app(desktop.to_str().unwrap_or(".")));
        }

        None
    }
}
