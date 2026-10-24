// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code, unused_imports, unused_variables)]

mod llm;
pub mod logger;
mod mcp;
mod memory;
mod movement;
mod orchestrator;
pub mod pc_controller;
mod security;
mod speech;
mod vision;

use llm::ModelInfo;
use logger::LogEntry;
use memory::MemoryItem;
use movement::DesktopPosition;
use orchestrator::{AgentOrchestrator, AgentResponse};
use pc_controller::PcActionResult;
use speech::SpeechProcess;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::Mutex;
use vision::ScreenCaptureResult;

struct AppState {
    orchestrator: Arc<Mutex<AgentOrchestrator>>,
    speech_engine: Arc<SpeechProcess>,
}

#[tauri::command]
async fn process_message(
    state: State<'_, AppState>,
    message: String,
    enable_vision: bool,
    speak_aloud: bool,
) -> Result<AgentResponse, String> {
    logger::log("CHAT", "INFO", &format!("User message: '{}'", message));
    let mut orch = state.orchestrator.lock().await;
    let res = orch.process_user_message(&message, enable_vision).await;
    if speak_aloud {
        state.speech_engine.speak(&res.reply);
    }
    Ok(res)
}

#[tauri::command]
async fn speak_text_native(state: State<'_, AppState>, text: String) -> Result<String, String> {
    state.speech_engine.speak(&text);
    Ok("Speech triggered".to_string())
}

#[tauri::command]
async fn drag_window(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_recent_logs() -> Result<Vec<LogEntry>, String> {
    Ok(logger::get_recent_logs())
}

#[tauri::command]
async fn clear_system_logs() -> Result<String, String> {
    logger::clear_logs();
    Ok("Logs cleared".to_string())
}

#[tauri::command]
async fn open_log_file() -> Result<String, String> {
    let log_dir = std::env::var("USERPROFILE")
        .map(|p| std::path::PathBuf::from(p).join(".aura").join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from("..").join("logs"));
    let log_path = log_dir.join("aura.log");
    if !log_path.exists() {
        let _ = std::fs::create_dir_all(&log_dir);
        let _ = std::fs::write(&log_path, "[SYSTEM] AURA-1 Log File Initialized\n");
    }
    let abs_path = std::fs::canonicalize(&log_path).unwrap_or(log_path);
    std::process::Command::new("notepad.exe")
        .arg(&abs_path)
        .spawn()
        .map_err(|e| format!("Failed to open log file: {}", e))?;
    logger::log("SYSTEM", "INFO", "Log file opened in Notepad.");
    Ok("Log file opened in Notepad".to_string())
}

#[tauri::command]
async fn set_active_model(state: State<'_, AppState>, model_id: String) -> Result<String, String> {
    let mut orch = state.orchestrator.lock().await;
    orch.set_model(&model_id).await;
    logger::log("LLM", "INFO", &format!("Switched active model to: {}", model_id));
    Ok(format!("Model switched to {}", model_id))
}

#[tauri::command]
async fn list_available_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let orch = state.orchestrator.lock().await;
    let models = orch.list_models().await;
    Ok(models)
}

#[tauri::command]
async fn capture_screen() -> Result<ScreenCaptureResult, String> {
    logger::log("VISION", "INFO", "Capturing Windows desktop screen...");
    let res = vision::VisionEngine::capture_screen();
    match &res {
        Ok(cap) => logger::log("VISION", "SUCCESS", &format!("Captured screen: {}x{}", cap.width, cap.height)),
        Err(e) => logger::log("VISION", "ERROR", &format!("Screen capture error: {}", e)),
    }
    res
}

#[tauri::command]
async fn trigger_autonomous_roam(state: State<'_, AppState>) -> Result<DesktopPosition, String> {
    let orch = state.orchestrator.lock().await;
    let mut movement = orch.movement.lock().await;
    movement.start_autonomous_roam();
    Ok(movement.current_position.clone())
}

#[tauri::command]
async fn stop_autonomous_roam(state: State<'_, AppState>) -> Result<String, String> {
    let orch = state.orchestrator.lock().await;
    let mut movement = orch.movement.lock().await;
    movement.stop_roaming();
    Ok("Roaming stopped".to_string())
}

#[tauri::command]
async fn move_to_preset(state: State<'_, AppState>, preset: String) -> Result<String, String> {
    let orch = state.orchestrator.lock().await;
    let mut movement = orch.movement.lock().await;
    movement.move_to_preset(&preset).await
}

#[tauri::command]
async fn get_desktop_location(state: State<'_, AppState>) -> Result<String, String> {
    let orch = state.orchestrator.lock().await;
    let mut movement = orch.movement.lock().await;
    Ok(movement.get_location_description())
}

#[tauri::command]
async fn minimize_window(app_handle: tauri::AppHandle) -> Result<PcActionResult, String> {
    Ok(pc_controller::PcController::minimize_window(&app_handle))
}

#[tauri::command]
async fn create_desktop_folder(folder_name: String) -> Result<PcActionResult, String> {
    Ok(pc_controller::PcController::create_folder(&folder_name, None))
}

#[tauri::command]
async fn open_desktop_target(target: String) -> Result<PcActionResult, String> {
    Ok(pc_controller::PcController::open_path_or_app(&target))
}

#[tauri::command]
async fn query_chroma_memory(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<MemoryItem>, String> {
    let orch = state.orchestrator.lock().await;
    let memory = orch.memory.lock().await;
    let results = memory.query_relevant_memories(&query, 5).await;
    logger::log("MEMORY", "INFO", &format!("Memory search for '{}': {} results", query, results.len()));
    Ok(results)
}

#[tauri::command]
async fn store_chroma_memory(state: State<'_, AppState>, detail: String) -> Result<String, String> {
    let orch = state.orchestrator.lock().await;
    let mut memory = orch.memory.lock().await;
    let item = MemoryItem {
        id: uuid::Uuid::new_v4().to_string(),
        text: detail.clone(),
        category: "user_detail".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        confidence: 0.95,
        embedding: None,
    };
    memory.store_memory(item).await?;
    logger::log("MEMORY", "SUCCESS", &format!("Stored memory: '{}'", detail));
    Ok("Memory stored successfully".to_string())
}

#[tauri::command]
async fn execute_safe_command(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let orch = state.orchestrator.lock().await;
    let mut sec = orch.security.lock().await;
    sec.execute_read(&path)
}

#[tauri::command]
async fn execute_guarded_write(
    state: State<'_, AppState>,
    path: String,
    content: String,
    approved: bool,
) -> Result<String, String> {
    let orch = state.orchestrator.lock().await;
    let mut sec = orch.security.lock().await;
    sec.execute_guarded_write(&path, &content, approved)
}

#[tauri::command]
async fn list_mcp_tools(state: State<'_, AppState>) -> Result<Vec<mcp::McpToolDefinition>, String> {
    let orch = state.orchestrator.lock().await;
    let mcp = orch.mcp.lock().await;
    Ok(mcp.list_tools().to_vec())
}

fn main() {
    let orchestrator = Arc::new(Mutex::new(AgentOrchestrator::new()));

    tauri::Builder::default()
        .setup({
            let orchestrator = orchestrator.clone();
            move |app| {
                let handle = app.handle().clone();
                logger::init_logger(Some(handle.clone()));
                logger::log("SYSTEM", "INFO", "AURA-1 Desktop Agent core initialized.");
                logger::log("LLM", "INFO", "Default Model: Google Gemini Flash (Active & Verified)");

                // Initialize movement controller with app handle for real desktop movement
                let orch_mov = orchestrator.clone();
                let handle_mov = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let orch = orch_mov.lock().await;
                    let mut mov = orch.movement.lock().await;
                    mov.set_app_handle(handle_mov);
                });

                // Spawn offline python speech engine
                let speech_engine = speech::SpeechProcess::spawn(handle, orchestrator.clone());

                app.manage(AppState {
                    orchestrator,
                    speech_engine,
                });

                logger::log("SYSTEM", "INFO", "AURA-1 initialization complete. Speech engine online.");
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            process_message,
            speak_text_native,
            get_recent_logs,
            clear_system_logs,
            open_log_file,
            set_active_model,
            list_available_models,
            capture_screen,
            trigger_autonomous_roam,
            stop_autonomous_roam,
            move_to_preset,
            get_desktop_location,
            minimize_window,
            create_desktop_folder,
            open_desktop_target,
            query_chroma_memory,
            store_chroma_memory,
            execute_safe_command,
            execute_guarded_write,
            list_mcp_tools,
            drag_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running AURA-1 personal agent");
}
