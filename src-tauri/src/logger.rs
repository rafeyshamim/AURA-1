use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub category: String,
    pub level: String,
    pub message: String,
}

static LOG_BUFFER: OnceLock<Mutex<Vec<LogEntry>>> = OnceLock::new();
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

fn get_buffer() -> &'static Mutex<Vec<LogEntry>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

fn get_app_handle() -> &'static Mutex<Option<AppHandle>> {
    APP_HANDLE.get_or_init(|| Mutex::new(None))
}

pub fn init_logger(app_handle: Option<AppHandle>) {
    if let Some(app) = app_handle {
        if let Ok(mut handle_guard) = get_app_handle().lock() {
            *handle_guard = Some(app);
        }
    }
    log("SYSTEM", "INFO", "AURA-1 logging system initialized.");
}

pub fn log(category: &str, level: &str, message: &str) {
    let now = Local::now().format("%H:%M:%S").to_string();
    let file_now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = LogEntry {
        timestamp: now.clone(),
        category: category.to_string(),
        level: level.to_string(),
        message: message.to_string(),
    };

    // Print to stdout in dev mode
    println!("[{}] [{}] [{}] {}", now, category, level, message);

    // Save to in-memory buffer (up to 500 entries)
    if let Ok(mut buf) = get_buffer().lock() {
        buf.push(entry.clone());
        if buf.len() > 500 {
            buf.remove(0);
        }
    }

    // Emit live event to UI
    if let Ok(guard) = get_app_handle().lock() {
        if let Some(app) = guard.as_ref() {
            let _ = app.emit("aura-log", &entry);
        }
    }

    // Write to %USERPROFILE%/.aura/logs/aura.log (outside watched source dirs to prevent cargo rebuild loops)
    let log_dir = std::env::var("USERPROFILE")
        .map(|p| PathBuf::from(p).join(".aura").join("logs"))
        .unwrap_or_else(|_| PathBuf::from("..").join("logs"));
    let _ = create_dir_all(&log_dir);
    let log_file = log_dir.join("aura.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = writeln!(file, "[{}] [{}] [{}] {}", file_now, category, level, message);
    }
}

pub fn get_recent_logs() -> Vec<LogEntry> {
    if let Ok(buf) = get_buffer().lock() {
        buf.clone()
    } else {
        Vec::new()
    }
}

pub fn clear_logs() {
    if let Ok(mut buf) = get_buffer().lock() {
        buf.clear();
    }
    log("SYSTEM", "INFO", "Log buffer cleared by user.");
}
