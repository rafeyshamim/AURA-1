use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActionType {
    Read,
    Write,
    Delete,
    ExecuteShell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditEntry {
    pub action_type: ActionType,
    pub target: String,
    pub timestamp: i64,
    pub approved_by_user: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPrompt {
    pub prompt_id: String,
    pub action_type: ActionType,
    pub target: String,
    pub preview_diff: String,
}

pub struct SecurityManager {
    pub audit_log: Vec<SecurityAuditEntry>,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self {
            audit_log: Vec::new(),
        }
    }

    /// Safe Read Operation: Auto-allowed without interrupting the conversation
    pub fn execute_read(&mut self, path_str: &str) -> Result<String, String> {
        let path = Path::new(path_str);
        if !path.exists() {
            return Err(format!("File does not exist: {}", path_str));
        }

        match fs::read_to_string(path) {
            Ok(content) => {
                self.audit_log.push(SecurityAuditEntry {
                    action_type: ActionType::Read,
                    target: path_str.to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    approved_by_user: true, // Read is pre-approved by policy
                    status: "Completed".to_string(),
                });
                Ok(content)
            }
            Err(e) => Err(format!("Failed to read file {}: {}", path_str, e)),
        }
    }

    /// List Directory Contents (Safe Read)
    pub fn list_directory(&mut self, dir_path: &str) -> Result<Vec<String>, String> {
        let path = Path::new(dir_path);
        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", dir_path));
        }

        let mut entries = Vec::new();
        match fs::read_dir(path) {
            Ok(read_dir) => {
                for entry in read_dir.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        entries.push(name);
                    }
                }
                self.audit_log.push(SecurityAuditEntry {
                    action_type: ActionType::Read,
                    target: dir_path.to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    approved_by_user: true,
                    status: "Completed".to_string(),
                });
                Ok(entries)
            }
            Err(e) => Err(format!("Failed to read directory {}: {}", dir_path, e)),
        }
    }

    /// Strict Guardrail: Write Operation requires explicit User Permission
    pub fn execute_guarded_write(
        &mut self,
        path_str: &str,
        content: &str,
        user_approved: bool,
    ) -> Result<String, String> {
        if !user_approved {
            self.audit_log.push(SecurityAuditEntry {
                action_type: ActionType::Write,
                target: path_str.to_string(),
                timestamp: chrono::Utc::now().timestamp(),
                approved_by_user: false,
                status: "Blocked: Pending User Permission".to_string(),
            });
            return Err(format!(
                "SECURITY GUARDRAIL: Write action on '{}' was denied or requires user permission.",
                path_str
            ));
        }

        // Parent directory verification
        let path = Path::new(path_str);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                let _ = fs::create_dir_all(parent);
            }
        }

        match fs::write(path, content) {
            Ok(_) => {
                self.audit_log.push(SecurityAuditEntry {
                    action_type: ActionType::Write,
                    target: path_str.to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    approved_by_user: true,
                    status: "Completed with User Permission".to_string(),
                });
                Ok(format!("Successfully wrote {} bytes to {}", content.len(), path_str))
            }
            Err(e) => Err(format!("Failed to write to {}: {}", path_str, e)),
        }
    }

    /// Guarded Shell Execution: Requires User Permission
    pub fn execute_guarded_shell(
        &mut self,
        command_str: &str,
        user_approved: bool,
    ) -> Result<String, String> {
        if !user_approved {
            self.audit_log.push(SecurityAuditEntry {
                action_type: ActionType::ExecuteShell,
                target: command_str.to_string(),
                timestamp: chrono::Utc::now().timestamp(),
                approved_by_user: false,
                status: "Blocked: Pending User Approval".to_string(),
            });
            return Err(format!(
                "SECURITY GUARDRAIL: Execution of command '{}' requires explicit user authorization.",
                command_str
            ));
        }

        let output = if cfg!(target_os = "windows") {
            Command::new("powershell")
                .args(["-Command", command_str])
                .output()
        } else {
            Command::new("sh")
                .args(["-c", command_str])
                .output()
        };

        match output {
            Ok(res) => {
                let stdout = String::from_utf8_lossy(&res.stdout).to_string();
                let stderr = String::from_utf8_lossy(&res.stderr).to_string();

                self.audit_log.push(SecurityAuditEntry {
                    action_type: ActionType::ExecuteShell,
                    target: command_str.to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    approved_by_user: true,
                    status: if res.status.success() { "Success".to_string() } else { "Failed".to_string() },
                });

                if res.status.success() {
                    Ok(stdout)
                } else {
                    Err(format!("Command failed: {}\nStderr: {}", stdout, stderr))
                }
            }
            Err(e) => Err(format!("Could not spawn process: {}", e)),
        }
    }
}
