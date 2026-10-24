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
    /// Resolve the active desktop directory on Windows.
    /// Checks for OneDrive\Desktop first (synced active desktop), then local USERPROFILE\Desktop.
    pub fn get_desktop_dir() -> PathBuf {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let onedrive_desktop = PathBuf::from(&user_profile).join("OneDrive").join("Desktop");
            if onedrive_desktop.exists() {
                return onedrive_desktop;
            }
            let standard_desktop = PathBuf::from(&user_profile).join("Desktop");
            if standard_desktop.exists() {
                return standard_desktop;
            }
        }
        PathBuf::from(".")
    }

    /// Minimize the AURA window
    pub fn minimize_window(app_handle: &AppHandle) -> PcActionResult {
        if let Some(window) = app_handle.get_webview_window("main") {
            match window.minimize() {
                Ok(_) => {
                    crate::logger::log("PC_ACTION", "SUCCESS", "Minimized AURA window.");
                    PcActionResult {
                        success: true,
                        message: "Minimized AURA window successfully.".to_string(),
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
                message: "Restored window to screen.".to_string(),
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

        let clean_name = folder_name.trim().trim_matches('"').trim_matches('\'');
        let new_folder_path = base_dir.join(clean_name);
        match fs::create_dir_all(&new_folder_path) {
            Ok(_) => {
                let msg = format!("Created folder '{}' at {:?}", clean_name, new_folder_path);
                crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                PcActionResult {
                    success: true,
                    message: msg,
                    action_type: "create_folder".to_string(),
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to create folder '{}': {}", clean_name, e);
                crate::logger::log("PC_ACTION", "ERROR", &err_msg);
                PcActionResult {
                    success: false,
                    message: err_msg,
                    action_type: "create_folder".to_string(),
                }
            }
        }
    }

    /// Get list of common search roots for the user
    pub fn get_search_directories() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let desktop = Self::get_desktop_dir();
        dirs.push(desktop.clone());

        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let p = PathBuf::from(&user_profile);
            let candidates = [
                p.join("Downloads"),
                p.join("Documents"),
                p.join("OneDrive"),
                p.join("Desktop"),
                p.clone(),
            ];
            for c in &candidates {
                if c.exists() && !dirs.contains(c) {
                    dirs.push(c.clone());
                }
            }
        }
        dirs
    }

    /// Search across Desktop and user directories for folders matching a query
    pub fn search_folders(query: &str) -> PcActionResult {
        let clean_q = query.trim().trim_matches('"').trim_matches('\'').to_lowercase();
        if clean_q.is_empty() {
            return PcActionResult {
                success: false,
                message: "No folder query specified.".to_string(),
                action_type: "search_folders".to_string(),
            };
        }

        let search_dirs = Self::get_search_directories();
        let mut found_folders: Vec<PathBuf> = Vec::new();

        for root in &search_dirs {
            if let Ok(entries) = fs::read_dir(root) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.to_lowercase().contains(&clean_q) {
                                let path = entry.path();
                                if !found_folders.contains(&path) {
                                    found_folders.push(path);
                                }
                            }
                        }
                    }
                }
            }
        }

        if found_folders.is_empty() {
            let msg = format!("No folders matching '{}' were found on Desktop or common user directories.", query);
            crate::logger::log("PC_ACTION", "INFO", &msg);
            PcActionResult {
                success: false,
                message: msg,
                action_type: "search_folders".to_string(),
            }
        } else {
            let list_str = found_folders
                .iter()
                .map(|p| format!("'{}' at {:?}", p.file_name().unwrap_or_default().to_string_lossy(), p))
                .collect::<Vec<_>>()
                .join(", ");
            let msg = format!("Found {} matching folder(s): {}", found_folders.len(), list_str);
            crate::logger::log("PC_ACTION", "SUCCESS", &msg);
            PcActionResult {
                success: true,
                message: msg,
                action_type: "search_folders".to_string(),
            }
        }
    }

    /// Search for a folder by name and immediately open it in Windows File Explorer
    pub fn find_and_open_folder(folder_name: &str) -> PcActionResult {
        let clean_name = folder_name.trim().trim_matches('"').trim_matches('\'');
        let lower_name = clean_name.to_lowercase();

        if lower_name.is_empty() {
            return PcActionResult {
                success: false,
                message: "No folder name provided to open.".to_string(),
                action_type: "find_and_open_folder".to_string(),
            };
        }

        // Special case: opening Desktop itself
        if lower_name == "desktop" || lower_name == "the desktop" || lower_name == "my desktop" {
            return Self::open_path_or_app("desktop");
        }

        let search_dirs = Self::get_search_directories();
        let mut target_folder: Option<PathBuf> = None;

        // 1. Direct match check in search directories
        for root in &search_dirs {
            let candidate = root.join(clean_name);
            if candidate.exists() && candidate.is_dir() {
                target_folder = Some(candidate);
                break;
            }
        }

        // 2. Case-insensitive exact match in search directories
        if target_folder.is_none() {
            'outer: for root in &search_dirs {
                if let Ok(entries) = fs::read_dir(root) {
                    for entry in entries.flatten() {
                        if let Ok(ft) = entry.file_type() {
                            if ft.is_dir() {
                                let name = entry.file_name().to_string_lossy().to_string();
                                if name.to_lowercase() == lower_name {
                                    target_folder = Some(entry.path());
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Partial substring match in search directories
        if target_folder.is_none() {
            'outer2: for root in &search_dirs {
                if let Ok(entries) = fs::read_dir(root) {
                    for entry in entries.flatten() {
                        if let Ok(ft) = entry.file_type() {
                            if ft.is_dir() {
                                let name = entry.file_name().to_string_lossy().to_string();
                                if name.to_lowercase().contains(&lower_name) {
                                    target_folder = Some(entry.path());
                                    break 'outer2;
                                }
                            }
                        }
                    }
                }
            }
        }

        // If target folder found, launch explorer.exe
        if let Some(path) = target_folder {
            let mut cmd = Command::new("explorer.exe");
            cmd.arg(&path);
            match cmd.spawn() {
                Ok(_) => {
                    let msg = format!("Found and opened folder '{}' at {:?}", clean_name, path);
                    crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                    PcActionResult {
                        success: true,
                        message: msg,
                        action_type: "find_and_open_folder".to_string(),
                    }
                }
                Err(e) => {
                    let err_msg = format!("Found folder at {:?}, but failed to launch File Explorer: {}", path, e);
                    crate::logger::log("PC_ACTION", "ERROR", &err_msg);
                    PcActionResult {
                        success: false,
                        message: err_msg,
                        action_type: "find_and_open_folder".to_string(),
                    }
                }
            }
        } else {
            let not_found_msg = format!(
                "Folder '{}' was not found on your Desktop or user directories.",
                clean_name
            );
            crate::logger::log("PC_ACTION", "WARN", &not_found_msg);
            PcActionResult {
                success: false,
                message: not_found_msg,
                action_type: "find_and_open_folder".to_string(),
            }
        }
    }

    /// Search for files or folders matching a query on the active Desktop
    pub fn search_desktop_files_or_folders(query: &str) -> PcActionResult {
        let desktop = Self::get_desktop_dir();
        let q = query.trim().to_lowercase();
        let mut matches = Vec::new();

        if let Ok(entries) = fs::read_dir(&desktop) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().contains(&q) {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let kind = if is_dir { "Folder" } else { "File" };
                    matches.push(format!("{} '{}' at {:?}", kind, name, entry.path()));
                }
            }
        }

        if matches.is_empty() {
            let msg = format!("No files or folders matching '{}' were found on Desktop ({:?}).", query, desktop);
            crate::logger::log("PC_ACTION", "INFO", &msg);
            PcActionResult {
                success: false,
                message: msg,
                action_type: "search_desktop".to_string(),
            }
        } else {
            let msg = format!("Found on Desktop: {}", matches.join("; "));
            crate::logger::log("PC_ACTION", "SUCCESS", &msg);
            PcActionResult {
                success: true,
                message: msg,
                action_type: "search_desktop".to_string(),
            }
        }
    }

    /// Open an application, desktop folder, or target path cleanly
    pub fn open_path_or_app(target: &str) -> PcActionResult {
        let trimmed = target.trim().trim_matches('"').trim_matches('\'');
        let lower = trimmed.to_lowercase();

        // 1. If user asks to open Desktop or Explorer
        if lower == "desktop" || lower == "the desktop" || lower == "my desktop" || lower == "explorer" {
            let desktop = Self::get_desktop_dir();
            let mut cmd = Command::new("explorer.exe");
            cmd.arg(&desktop);
            return match cmd.spawn() {
                Ok(_) => {
                    let msg = format!("Opened Desktop folder at {:?}", desktop);
                    crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                    PcActionResult {
                        success: true,
                        message: msg,
                        action_type: "open".to_string(),
                    }
                }
                Err(e) => PcActionResult {
                    success: false,
                    message: format!("Failed to open Desktop folder: {}", e),
                    action_type: "open".to_string(),
                },
            };
        }

        // 2. Common application shortcuts
        if lower == "notepad" || lower == "notepad.exe" {
            let _ = Command::new("notepad.exe").spawn();
            return PcActionResult {
                success: true,
                message: "Opened Notepad".to_string(),
                action_type: "open".to_string(),
            };
        }

        if lower == "calc" || lower == "calculator" || lower == "calc.exe" {
            let _ = Command::new("calc.exe").spawn();
            return PcActionResult {
                success: true,
                message: "Opened Calculator".to_string(),
                action_type: "open".to_string(),
            };
        }

        // 3. Direct path that exists on disk
        let path = Path::new(trimmed);
        if path.exists() {
            let mut cmd = Command::new("explorer.exe");
            cmd.arg(trimmed);
            return match cmd.spawn() {
                Ok(_) => {
                    let msg = format!("Opened '{:?}'", path);
                    crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                    PcActionResult {
                        success: true,
                        message: msg,
                        action_type: "open".to_string(),
                    }
                }
                Err(e) => PcActionResult {
                    success: false,
                    message: format!("Failed to open '{:?}': {}", path, e),
                    action_type: "open".to_string(),
                },
            };
        }

        // 4. Check if it exists inside the active Desktop directory
        let in_desktop = Self::get_desktop_dir().join(trimmed);
        if in_desktop.exists() {
            let mut cmd = Command::new("explorer.exe");
            cmd.arg(&in_desktop);
            return match cmd.spawn() {
                Ok(_) => {
                    let msg = format!("Opened Desktop path '{:?}'", in_desktop);
                    crate::logger::log("PC_ACTION", "SUCCESS", &msg);
                    PcActionResult {
                        success: true,
                        message: msg,
                        action_type: "open".to_string(),
                    }
                }
                Err(e) => PcActionResult {
                    success: false,
                    message: format!("Failed to open '{:?}': {}", in_desktop, e),
                    action_type: "open".to_string(),
                },
            };
        }

        // 5. Try searching for folder by name
        let folder_search = Self::find_and_open_folder(trimmed);
        if folder_search.success {
            return folder_search;
        }

        // 6. Fallback: try explorer.exe with the parameter
        let mut cmd = Command::new("explorer.exe");
        cmd.arg(trimmed);
        match cmd.spawn() {
            Ok(_) => PcActionResult {
                success: true,
                message: format!("Opened '{}'", trimmed),
                action_type: "open".to_string(),
            },
            Err(e) => PcActionResult {
                success: false,
                message: format!("Unable to open '{}': {}", trimmed, e),
                action_type: "open".to_string(),
            },
        }
    }

    /// Execute a safe command requested by voice (fallback handler)
    pub fn execute_voice_command(app_handle: &AppHandle, cmd_str: &str) -> Option<PcActionResult> {
        let lower = cmd_str.to_lowercase();
        
        if lower.contains("minimize") {
            return Some(Self::minimize_window(app_handle));
        }

        if lower.contains("unminimize") || lower.contains("restore window") {
            return Some(Self::unminimize_window(app_handle));
        }

        if lower.contains("search desktop") || lower.contains("find folder") || lower.contains("find file") {
            let query = if let Some(idx) = lower.find("for ") {
                &cmd_str[idx + 4..]
            } else if let Some(idx) = lower.find("named ") {
                &cmd_str[idx + 6..]
            } else {
                cmd_str
            };
            return Some(Self::search_desktop_files_or_folders(query));
        }

        if lower.contains("create folder") || lower.contains("make a folder") || lower.contains("new folder") {
            let name = if let Some(idx) = lower.find("named") {
                cmd_str[idx + 5..].trim().trim_matches('"').trim_matches('\'')
            } else if let Some(idx) = lower.find("called") {
                cmd_str[idx + 6..].trim().trim_matches('"').trim_matches('\'')
            } else if let Some(idx) = lower.find("folder") {
                let sub = cmd_str[idx + 6..].trim();
                let candidate = sub.trim_start_matches("called").trim_start_matches("named").trim();
                if candidate.is_empty() { "New Folder" } else { candidate }
            } else {
                "New Folder"
            };
            return Some(Self::create_folder(name, None));
        }

        if lower.contains("open folder") || lower.contains("open the folder") {
            let name = if let Some(idx) = lower.find("named ") {
                &cmd_str[idx + 6..]
            } else if let Some(idx) = lower.find("called ") {
                &cmd_str[idx + 7..]
            } else if let Some(idx) = lower.find("folder ") {
                &cmd_str[idx + 7..]
            } else {
                "desktop"
            };
            return Some(Self::find_and_open_folder(name));
        }

        if lower.contains("open notepad") {
            return Some(Self::open_path_or_app("notepad"));
        }

        if lower.contains("open calculator") || lower.contains("calc") {
            return Some(Self::open_path_or_app("calc"));
        }

        if lower.contains("open explorer") || lower.contains("open my desktop") || lower.contains("open desktop") {
            return Some(Self::open_path_or_app("desktop"));
        }

        None
    }
}
