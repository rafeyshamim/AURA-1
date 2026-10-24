use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
fn run_powershell_encoded(script: &str) -> Result<std::process::Output, String> {
    use base64::Engine;
    use std::process::Stdio;

    let encoded = base64::engine::general_purpose::STANDARD.encode(
        script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>(),
    );
    let mut command = Command::new("powershell.exe");
    command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command.output().map_err(|error| format!("Could not start Windows input control: {error}"))
}

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

    /// Control a Windows app by its Start menu display name. The app name is
    /// embedded as a single-quoted PowerShell literal after escaping quotes,
    /// then the complete script is UTF-16LE/base64 encoded for safe invocation.
    pub fn manage_windows_application(action: &str, app_name: &str) -> PcActionResult {
        let name = app_name.trim();
        let action = action.trim().to_lowercase();
        if name.is_empty() || name.chars().any(char::is_control) {
            return PcActionResult {
                success: false,
                message: "Enter a valid Windows application name.".to_string(),
                action_type: "manage_windows_application".to_string(),
            };
        }
        if !matches!(action.as_str(), "open" | "switch" | "close") {
            return PcActionResult {
                success: false,
                message: "Unsupported app action. Choose open, switch, or close.".to_string(),
                action_type: "manage_windows_application".to_string(),
            };
        }

        #[cfg(target_os = "windows")]
        {
            use base64::Engine;
            let escaped_name = name.replace('\'', "''");
            let script = format!(r#"
$ErrorActionPreference = 'Stop'
$name = '{escaped_name}'.Trim()
$action = '{action}'
$needle = $name.ToLowerInvariant()
$normalizedNeedle = [regex]::Replace(($needle -replace '\s+ide$', ''), '[^a-z0-9]', '')
$apps = @()
try {{ $apps = @(Get-StartApps | Where-Object {{ $_.Name -and $_.AppID }}) }} catch {{ $apps = @() }}
$appExact = @($apps | Where-Object {{ $_.Name -ieq $name }})
$appNameExact = @($apps | Where-Object {{ [regex]::Replace(($_.Name.ToLowerInvariant() -replace '\s+ide$', ''), '[^a-z0-9]', '') -eq $normalizedNeedle }})
$appDisplayName = if ($appExact.Count -eq 1) {{ $appExact[0].Name }} elseif ($appNameExact.Count -eq 1) {{ $appNameExact[0].Name }} else {{ $null }}
$processAliases = @()
if ($appDisplayName) {{
  $shortcutRoots = @((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'), (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs'))
  $shell = New-Object -ComObject WScript.Shell
  foreach ($root in $shortcutRoots) {{
    if (Test-Path -LiteralPath $root) {{
      foreach ($link in (Get-ChildItem -LiteralPath $root -Filter '*.lnk' -File -Recurse -ErrorAction SilentlyContinue | Where-Object {{ $_.BaseName -ieq $appDisplayName }})) {{
        $targetPath = $shell.CreateShortcut($link.FullName).TargetPath
        if ($targetPath) {{ $processAliases += [IO.Path]::GetFileNameWithoutExtension($targetPath).ToLowerInvariant() }}
      }}
    }}
  }}
}}
$windows = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -ne 0 -and (([regex]::Replace($_.ProcessName.ToLowerInvariant(), '[^a-z0-9]', '').Contains($normalizedNeedle)) -or ($processAliases -contains $_.ProcessName.ToLowerInvariant()) -or ($_.MainWindowTitle -and [regex]::Replace($_.MainWindowTitle.ToLowerInvariant(), '[^a-z0-9]', '').Contains($normalizedNeedle))) }})
$exact = @($windows | Where-Object {{ $_.ProcessName -ieq $name -or $processAliases -contains $_.ProcessName.ToLowerInvariant() -or $_.MainWindowTitle -ieq $name }})
$candidateWindows = if ($exact.Count -gt 0) {{ $exact }} else {{ $windows }}
if ($action -eq 'close' -or $action -eq 'switch') {{
  if ($candidateWindows.Count -eq 0) {{ Write-Output "No running application window matched '$name'."; exit 3 }}
  $processGroups = @($candidateWindows | Group-Object ProcessName)
  if ($exact.Count -eq 0 -and $processGroups.Count -gt 1) {{
    $choices = ($candidateWindows | ForEach-Object {{ if ($_.MainWindowTitle) {{ "{{0}} — {{1}}" -f $_.ProcessName, $_.MainWindowTitle }} else {{ $_.ProcessName }} }} | Sort-Object -Unique) -join '; '
    Write-Output "Several running apps matched '$name': $choices. Please use a more specific name."
    exit 4
  }}
  if ($action -eq 'switch') {{
    $target = $candidateWindows | Sort-Object {{ if ($_.MainWindowTitle -ieq $name) {{ 0 }} else {{ 1 }} }} | Select-Object -First 1
    $shell = New-Object -ComObject WScript.Shell
    if ($shell.AppActivate($target.Id)) {{ Write-Output "Switched to $($target.ProcessName): $($target.MainWindowTitle)"; exit 0 }}
    Write-Output "Found $($target.ProcessName), but Windows did not allow its window to come to the foreground."
    exit 5
  }}
  $closed = 0
  foreach ($target in $candidateWindows) {{ if ($target.CloseMainWindow()) {{ $closed++ }} }}
  if ($closed -gt 0) {{ Write-Output "Sent a normal close request to $closed window(s) for '$name'. Apps with unsaved work can show their own save prompt; nothing was force-terminated."; exit 0 }}
  Write-Output "Found '$name', but its window did not accept a normal close request. It was not force-terminated."
  exit 5
}}
$openWindowGroups = @($candidateWindows | Group-Object ProcessName)
if ($candidateWindows.Count -gt 0 -and ($exact.Count -gt 0 -or $openWindowGroups.Count -eq 1)) {{
  $target = $candidateWindows | Select-Object -First 1
  $shell = New-Object -ComObject WScript.Shell
  if ($shell.AppActivate($target.Id)) {{ Write-Output "'$name' is already open; switched to $($target.MainWindowTitle)."; exit 0 }}
}}
$appMatches = if ($appExact.Count -gt 0) {{ $appExact }} elseif ($appNameExact.Count -gt 0) {{ $appNameExact }} else {{ @($apps | Where-Object {{ [regex]::Replace(($_.Name.ToLowerInvariant() -replace '\s+ide$', ''), '[^a-z0-9]', '').Contains($normalizedNeedle) }}) }}
if ($appMatches.Count -eq 0) {{ Write-Output "No installed Start menu application matched '$name'. Try the app's full Start menu name."; exit 3 }}
if ($appMatches.Count -gt 1) {{
  $choices = ($appMatches | Select-Object -ExpandProperty Name | Sort-Object -Unique | Select-Object -First 8) -join '; '
  Write-Output "Several installed apps matched '$name': $choices. Please use a more specific name."
  exit 4
}}
$app = $appMatches[0]
Start-Process -FilePath (Join-Path $env:WINDIR 'explorer.exe') -ArgumentList @('shell:AppsFolder\' + $app.AppID)
Write-Output "Opened $($app.Name)."
"#);
            let encoded = base64::engine::general_purpose::STANDARD.encode(
                script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>(),
            );
            let mut command = Command::new("powershell.exe");
            command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded]);
            use std::process::Stdio;
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }
            match command.output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let message = if stdout.is_empty() { stderr } else { stdout };
                    let success = output.status.success();
                    crate::logger::log(
                        "PC_ACTION",
                        if success { "SUCCESS" } else { "WARN" },
                        &format!("App {action} '{name}': {message}"),
                    );
                    PcActionResult { success, message, action_type: "manage_windows_application".to_string() }
                }
                Err(error) => PcActionResult {
                    success: false,
                    message: format!("Could not start Windows app control: {error}"),
                    action_type: "manage_windows_application".to_string(),
                },
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (action, name);
            PcActionResult {
                success: false,
                message: "Application control by name is currently available on Windows only.".to_string(),
                action_type: "manage_windows_application".to_string(),
            }
        }
    }

    /// Perform a single left click only when the live display and active
    /// foreground process still match the screenshot used to choose the point.
    pub fn click_screen(
        x: i32,
        y: i32,
        screen_width: u32,
        screen_height: u32,
        expected_process_id: u32,
        expected_window_title: &str,
    ) -> PcActionResult {
        #[cfg(target_os = "windows")]
        {
            use base64::Engine;
            let encoded_title = base64::engine::general_purpose::STANDARD.encode(expected_window_title.as_bytes());
            let script = format!(r#"
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class AuraMouseInput {{
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);
  [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int maxCount);
  public static uint ForegroundProcessId() {{ uint id; GetWindowThreadProcessId(GetForegroundWindow(), out id); return id; }}
  public static string ForegroundTitle() {{ StringBuilder title = new StringBuilder(512); GetWindowText(GetForegroundWindow(), title, title.Capacity); return title.ToString(); }}
  public static bool Click(int x, int y, int width, int height, uint expectedPid, string expectedTitle) {{
    SetProcessDPIAware();
    if (GetSystemMetrics(0) != width || GetSystemMetrics(1) != height) return false;
    if (ForegroundProcessId() != expectedPid || !String.Equals(ForegroundTitle(), expectedTitle, StringComparison.Ordinal) || x < 0 || y < 0 || x >= width || y >= height) return false;
    if (!SetCursorPos(x, y)) return false;
    Thread.Sleep(80);
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    Thread.Sleep(45);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    return true;
  }}
}}
'@
 $expectedTitle = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded_title}'))
if ([AuraMouseInput]::Click({x}, {y}, {screen_width}, {screen_height}, {expected_process_id}, $expectedTitle)) {{ Write-Output 'Clicked.'; exit 0 }}
Write-Output 'Click cancelled because the screen size or foreground app changed, or the coordinates were outside the screen.'
exit 5
"#);
            match run_powershell_encoded(&script) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let message = if stdout.is_empty() { stderr } else { stdout };
                    PcActionResult { success: output.status.success(), message, action_type: "click_screen".to_string() }
                }
                Err(message) => PcActionResult { success: false, message, action_type: "click_screen".to_string() },
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (x, y, screen_width, screen_height, expected_process_id, expected_window_title);
            PcActionResult { success: false, message: "Screen clicking is available on Windows only.".to_string(), action_type: "click_screen".to_string() }
        }
    }

    /// Click a uniquely named accessible button/tab in the screenshot's foreground app.
    /// Returns None when Windows UI Automation has no exact accessible-name match.
    pub fn click_accessible_control(process_id: u32, accessible_name: &str) -> Option<PcActionResult> {
        let label = accessible_name.trim();
        if label.is_empty() || label.chars().any(char::is_control) { return None; }
        #[cfg(target_os = "windows")]
        {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(label.as_bytes());
            let script = format!(r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$wanted = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}'))
$condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, [int]{process_id})
$windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
$matches = @()
foreach ($window in $windows) {{
  $elements = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  foreach ($element in $elements) {{
    if ($element.Current.Name -ceq $wanted -and ($element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -or $element.Current.ControlType -eq [System.Windows.Automation.ControlType]::TabItem)) {{ $matches += $element }}
  }}
}}
if ($matches.Count -eq 0) {{ exit 3 }}
if ($matches.Count -gt 1) {{ Write-Output "More than one accessible control is named '$wanted'; no click was made."; exit 4 }}
try {{
  if ($matches[0].Current.ControlType -eq [System.Windows.Automation.ControlType]::TabItem) {{
    $pattern = $matches[0].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern); $pattern.Select()
  }} else {{
    $pattern = $matches[0].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern); $pattern.Invoke()
  }}
  Write-Output "Activated accessible control '$wanted'."; exit 0
}} catch {{ Write-Output "The accessible control '$wanted' could not be invoked."; exit 5 }}
"#);
            match run_powershell_encoded(&script) {
                Ok(output) if output.status.success() => Some(PcActionResult { success: true, message: String::from_utf8_lossy(&output.stdout).trim().to_string(), action_type: "click_accessible_control".to_string() }),
                Ok(output) if output.status.code() == Some(3) => None,
                Ok(output) => {
                    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    Some(PcActionResult { success: false, message: if out.is_empty() { err } else { out }, action_type: "click_accessible_control".to_string() })
                }
                Err(message) => Some(PcActionResult { success: false, message, action_type: "click_accessible_control".to_string() }),
            }
        }
        #[cfg(not(target_os = "windows"))]
        { let _ = (process_id, label); None }
    }

    /// Send one explicitly requested key or shortcut to a uniquely matched running app.
    pub fn press_key(app_name: &str, key: &str) -> PcActionResult {
        let name = app_name.trim();
        let key = key.trim().to_ascii_uppercase().replace(' ', "");
        let vk: Vec<u16> = match key.as_str() {
            "ENTER" | "RETURN" => vec![0x0D],
            "TAB" => vec![0x09],
            "ESC" | "ESCAPE" => vec![0x1B],
            "CTRL+S" => vec![0x11, 0x53],
            "CTRL+N" => vec![0x11, 0x4E],
            "CTRL+TAB" => vec![0x11, 0x09],
            _ => return PcActionResult { success: false, message: "Supported keys are Enter, Tab, Escape, Ctrl+S, Ctrl+N, and Ctrl+Tab.".to_string(), action_type: "press_key".to_string() },
        };
        if name.is_empty() || name.chars().any(char::is_control) {
            return PcActionResult { success: false, message: "Choose a valid target application name.".to_string(), action_type: "press_key".to_string() };
        }
        #[cfg(target_os = "windows")]
        {
            let escaped_name = name.replace('\'', "''");
            let mut down = String::new();
            let mut up = String::new();
            for code in &vk { down.push_str(&format!("if (-not [AuraKeyInput]::Key({code}, $false)) {{ throw 'Key down failed' }}\n")); }
            for code in vk.iter().rev() { up.push_str(&format!("if (-not [AuraKeyInput]::Key({code}, $true)) {{ throw 'Key up failed' }}\n")); }
            let script = format!(r#"
$ErrorActionPreference = 'Stop'
$name = '{escaped_name}'.Trim()
$needle = [regex]::Replace($name.ToLowerInvariant(), '[^a-z0-9]', '')
$windows = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -ne 0 -and ([regex]::Replace($_.ProcessName.ToLowerInvariant(), '[^a-z0-9]', '').Contains($needle) -or ($_.MainWindowTitle -and [regex]::Replace($_.MainWindowTitle.ToLowerInvariant(), '[^a-z0-9]', '').Contains($needle))) }})
if ($windows.Count -eq 0) {{ Write-Output "No running window matched '$name'."; exit 3 }}
$ids = @($windows | ForEach-Object {{ $_.Id }} | Select-Object -Unique)
if ($ids.Count -ne 1) {{ Write-Output "More than one running app matched '$name'; no key was sent."; exit 4 }}
$shell = New-Object -ComObject WScript.Shell
if (-not $shell.AppActivate([int]$ids[0])) {{ Write-Output "Could not focus '$name'; no key was sent."; exit 5 }}
Start-Sleep -Milliseconds 100
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AuraKeyInput {{
  [StructLayout(LayoutKind.Sequential)] struct K {{ public ushort vk; public ushort scan; public uint flags; public uint time; public IntPtr extra; }}
  [StructLayout(LayoutKind.Sequential)] struct M {{ public int dx; public int dy; public uint data; public uint flags; public uint time; public IntPtr extra; }}
  [StructLayout(LayoutKind.Explicit)] struct U {{ [FieldOffset(0)] public K key; [FieldOffset(0)] public M mouse; }}
  [StructLayout(LayoutKind.Sequential)] struct I {{ public uint type; public U data; }}
  [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, ref I input, int size);
  public static bool Key(ushort vk, bool up) {{ I i = new I(); i.type = 1; i.data.key = new K {{ vk = vk, flags = up ? 2u : 0u }}; return SendInput(1, ref i, Marshal.SizeOf(typeof(I))) == 1; }}
}}
'@
{down}{up}
Write-Output "Sent {key} to '$name'."
"#);
            match run_powershell_encoded(&script) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    PcActionResult { success: output.status.success(), message: if stdout.is_empty() { stderr } else { stdout }, action_type: "press_key".to_string() }
                }
                Err(message) => PcActionResult { success: false, message, action_type: "press_key".to_string() },
            }
        }
        #[cfg(not(target_os = "windows"))]
        { let _ = (name, vk); PcActionResult { success: false, message: "Keyboard control is available on Windows only.".to_string(), action_type: "press_key".to_string() } }
    }

    /// Type Unicode text into the focused field of a named running app. This
    /// routine never presses Enter, clicks, or submits the surrounding form.
    pub fn type_text(app_name: &str, text: &str) -> PcActionResult {
        let name = app_name.trim();
        if name.is_empty() || name.chars().any(char::is_control) {
            return PcActionResult { success: false, message: "Choose a valid target application name.".to_string(), action_type: "type_text".to_string() };
        }
        if text.is_empty() || text.chars().count() > 8000 {
            return PcActionResult { success: false, message: "Text must contain between 1 and 8,000 characters.".to_string(), action_type: "type_text".to_string() };
        }
        if text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r')) {
            return PcActionResult { success: false, message: "Text contains control characters that could trigger app shortcuts. Remove them and try again.".to_string(), action_type: "type_text".to_string() };
        }

        #[cfg(target_os = "windows")]
        {
            use base64::Engine;
            let escaped_name = name.replace('\'', "''");
            let encoded_text = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
            let script = format!(r#"
$ErrorActionPreference = 'Stop'
$name = '{escaped_name}'.Trim()
$needle = $name.ToLowerInvariant()
$normalizedNeedle = [regex]::Replace(($needle -replace '\s+ide$', ''), '[^a-z0-9]', '')
$apps = @(Get-StartApps -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -and $_.AppID }})
$appExact = @($apps | Where-Object {{ $_.Name -ieq $name }})
$appNameExact = @($apps | Where-Object {{ [regex]::Replace(($_.Name.ToLowerInvariant() -replace '\s+ide$', ''), '[^a-z0-9]', '') -eq $normalizedNeedle }})
$processAliases = @()
if ($appExact.Count -eq 1 -or $appNameExact.Count -eq 1) {{
  $appDisplayName = if ($appExact.Count -eq 1) {{ $appExact[0].Name }} else {{ $appNameExact[0].Name }}
  $shell = New-Object -ComObject WScript.Shell
  $shortcutRoots = @((Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'), (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs'))
  foreach ($root in $shortcutRoots) {{
    if (Test-Path -LiteralPath $root) {{
      foreach ($link in (Get-ChildItem -LiteralPath $root -Filter '*.lnk' -File -Recurse -ErrorAction SilentlyContinue | Where-Object {{ $_.BaseName -ieq $appDisplayName }})) {{
        $targetPath = $shell.CreateShortcut($link.FullName).TargetPath
        if ($targetPath) {{ $processAliases += [IO.Path]::GetFileNameWithoutExtension($targetPath).ToLowerInvariant() }}
      }}
    }}
  }}
}}
$windows = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -ne 0 -and (([regex]::Replace($_.ProcessName.ToLowerInvariant(), '[^a-z0-9]', '').Contains($normalizedNeedle)) -or ($processAliases -contains $_.ProcessName.ToLowerInvariant()) -or ($_.MainWindowTitle -and [regex]::Replace($_.MainWindowTitle.ToLowerInvariant(), '[^a-z0-9]', '').Contains($normalizedNeedle))) }})
$exact = @($windows | Where-Object {{ $_.ProcessName -ieq $name -or $processAliases -contains $_.ProcessName.ToLowerInvariant() -or $_.MainWindowTitle -ieq $name }})
$candidates = if ($exact.Count -gt 0) {{ $exact }} else {{ $windows }}
if ($candidates.Count -eq 0) {{ Write-Output "No running window matched '$name'. Open the app and try again."; exit 3 }}
$groups = @($candidates | Group-Object ProcessName)
if ($exact.Count -eq 0 -and $groups.Count -gt 1) {{ Write-Output "More than one running app matched '$name'. Use its exact app name."; exit 4 }}
$target = $candidates | Select-Object -First 1
$shell = New-Object -ComObject WScript.Shell
if (-not $shell.AppActivate($target.Id)) {{ Write-Output "Could not focus '$name'; no text was entered."; exit 5 }}
Start-Sleep -Milliseconds 100
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class AuraTextInput {{
  [StructLayout(LayoutKind.Sequential)] struct KeyInput {{ public ushort vk; public ushort scan; public uint flags; public uint time; public IntPtr extra; }}
  [StructLayout(LayoutKind.Sequential)] struct MouseInput {{ public int dx; public int dy; public uint data; public uint flags; public uint time; public IntPtr extra; }}
  [StructLayout(LayoutKind.Explicit)] struct InputUnion {{ [FieldOffset(0)] public MouseInput mouse; [FieldOffset(0)] public KeyInput key; }}
  [StructLayout(LayoutKind.Sequential)] struct Input {{ public uint type; public InputUnion data; }}
  [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, ref Input input, int size);
  static bool Key(ushort vk, ushort scan, uint flags) {{
    Input input = new Input(); input.type = 1; input.data.key = new KeyInput {{ vk = vk, scan = scan, flags = flags }};
    return SendInput(1, ref input, Marshal.SizeOf(typeof(Input))) == 1;
  }}
  public static bool Type(string value) {{
    foreach (char c in value) {{
      if (c == '\r') continue;
      if (c == '\n') {{ if (!Key(0x10, 0, 0) || !Key(0x0D, 0, 0) || !Key(0x0D, 0, 2) || !Key(0x10, 0, 2)) return false; continue; }}
      if (!Key(0, c, 4) || !Key(0, c, 6)) return false;
    }}
    return true;
  }}
}}
'@
$text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded_text}'))
if ([AuraTextInput]::Type($text)) {{ Write-Output "Typed $($text.Length) characters into $name. No Enter key was sent."; exit 0 }}
Write-Output "Text entry was interrupted; some text may have been entered. No Enter key was sent."
exit 5
"#);
            match run_powershell_encoded(&script) {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let message = if stdout.is_empty() { stderr } else { stdout };
                    PcActionResult { success: output.status.success(), message, action_type: "type_text".to_string() }
                }
                Err(message) => PcActionResult { success: false, message, action_type: "type_text".to_string() },
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (name, text);
            PcActionResult { success: false, message: "Text entry is available on Windows only.".to_string(), action_type: "type_text".to_string() }
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
