use crate::llm::{LlmEngine, ModelInfo};
use crate::mcp::McpManager;
use crate::memory::{ChromaMemoryEngine, MemoryItem};
use crate::movement::MovementController;
use crate::security::SecurityManager;
use crate::vision::VisionEngine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolAction {
    pub tool_name: String,
    pub summary: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub reply: String,
    pub model_used: String,
    pub extracted_memories: Vec<MemoryItem>,
    pub retrieved_context: Vec<MemoryItem>,
    pub screen_seen: bool,
    pub pending_write_action: Option<String>,
    pub color_action: Option<String>,
    pub pc_action: Option<String>,
}

pub struct AgentOrchestrator {
    pub llm: Arc<Mutex<LlmEngine>>,
    pub memory: Arc<Mutex<ChromaMemoryEngine>>,
    pub security: Arc<Mutex<SecurityManager>>,
    pub mcp: Arc<Mutex<McpManager>>,
    pub movement: Arc<Mutex<MovementController>>,
    pub conversation_history: Vec<ChatMessage>,
    pub session_tool_history: Vec<SessionToolAction>,
    pub app_handle: Option<tauri::AppHandle>,
}

impl AgentOrchestrator {
    pub fn new() -> Self {
        println!("[Orchestrator] Initializing AURA-1 agent...");
        Self {
            llm: Arc::new(Mutex::new(LlmEngine::new(None))),
            memory: Arc::new(Mutex::new(ChromaMemoryEngine::new(None, None))),
            security: Arc::new(Mutex::new(SecurityManager::new())),
            mcp: Arc::new(Mutex::new(McpManager::new())),
            movement: Arc::new(Mutex::new(MovementController::new())),
            conversation_history: Vec::new(),
            session_tool_history: Vec::new(),
            app_handle: None,
        }
    }

    pub fn set_app_handle(&mut self, app_handle: tauri::AppHandle) {
        self.app_handle = Some(app_handle);
    }

    /// Record a tool execution into active runtime session memory
    pub fn record_session_action(&mut self, tool_name: &str, summary: &str) {
        self.session_tool_history.push(SessionToolAction {
            tool_name: tool_name.to_string(),
            summary: summary.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        });
    }

    /// Format session tool history for LLM context injection
    pub fn get_session_history_summary(&self) -> String {
        if self.session_tool_history.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n[Current Runtime Session Actions (Active Session Memory - Forgotten on restart)]:\n");
        for action in &self.session_tool_history {
            out.push_str(&format!("- {}\n", action.summary));
        }
        out.push_str("Note: Use this runtime session history to remember folders created, locations opened, position changes, or actions taken during this session.\n");
        out
    }

    /// Process an incoming message with optional Screen Vision
    pub async fn process_user_message(
        &mut self,
        user_text: &str,
        enable_vision: bool,
    ) -> AgentResponse {
        println!("[Orchestrator] Processing message: '{}'", &user_text[..user_text.len().min(80)]);
        
        let now = chrono::Utc::now().timestamp();
        self.conversation_history.push(ChatMessage {
            role: "user".to_string(),
            content: user_text.to_string(),
            timestamp: now,
        });

        // 1. Query ChromaDB for relevant personal memories instantaneously in-memory (0ms)
        let memory_guard = self.memory.lock().await;
        let retrieved_context = memory_guard.query_relevant_memories_fast(user_text, 3);
        let extracted_facts = memory_guard.extract_facts_from_convo(user_text);
        drop(memory_guard);

        // 2. Store extracted facts in background (fire-and-forget) — don't block the LLM call
        if !extracted_facts.is_empty() {
            let mem_arc = self.memory.clone();
            let facts_clone = extracted_facts.clone();
            tokio::spawn(async move {
                let mut mg = mem_arc.lock().await;
                for fact in facts_clone {
                    let _ = mg.store_memory(fact).await;
                }
            });
        }

        // 3. Optional Screen Vision capture
        let mut screen_image_b64: Option<String> = None;
        if enable_vision {
            println!("[Orchestrator] Vision enabled, capturing screen...");
            match VisionEngine::capture_screen() {
                Ok(cap) => {
                    println!("[Orchestrator] Screen captured: {}x{}", cap.width, cap.height);
                    screen_image_b64 = Some(cap.base64_image);
                }
                Err(e) => {
                    println!("[Orchestrator] Screen capture failed: {}", e);
                }
            }
        }

        // 4. Check if user is asking for a write / modifying command
        let lower = user_text.to_lowercase();
        let mut pending_write = None;
        if lower.contains("write")
            || lower.contains("create file")
            || lower.contains("save")
            || lower.contains("delete")
        {
            pending_write = Some(format!("Request to modify filesystem: {}", user_text));
        }

        // 5. Construct System Context incorporating ChromaDB memories & Desktop Location
        let location_info = {
            let mut mov = self.movement.lock().await;
            mov.get_location_description()
        };

        let mut context_str = format!(
            "You are AURA-1, an intelligent desktop companion AI with real-time Windows OS integration.\n\
             [Current Live Desktop Location]: {}\n\
             YOU HAVE REAL-TIME NATIVE TOOLS:\n\
             - `find_and_open_folder(folder_name)`: Searches for a folder by name across Desktop, Downloads, Documents, and PC, and immediately opens it in Windows File Explorer.\n\
             - `search_folders(query)`: Searches Desktop and user directories for folders matching a query without opening them.\n\
             - `create_desktop_folder(folder_name)`: Creates a new folder directly on the user's active Windows Desktop.\n\
             - `search_desktop_files_or_folders(query)`: Searches the Desktop for files or folders to verify where they are or check if they exist.\n\
             - `get_current_desktop_location()`: Queries your live screen coordinates and location on the user's desktop at runtime.\n\
             - `clear_session_context()`: Forgets or clears the runtime session actions and transient history for this session.\n\
             - `search_vectordb_memory(query)`: Searches ChromaDB long-term memory for user facts, user's name, user preferences, or saved context.\n\
             - `open_folder_or_app(target)`: Opens Windows File Explorer (e.g. 'desktop') or launches apps ('notepad', 'calc').\n\
             - `minimize_window()`: Minimizes the AURA companion window.\n\
             - `restore_window()`: Restores/unminimizes the AURA companion window.\n\
             - `change_ui_color(color)`: Changes the glowing UI orb color theme ('red', 'blue', 'purple', 'violet', 'green', 'gold', 'yellow', 'pink', 'cyan', 'orange', 'white').\n\
             - `move_desktop_position(position)`: Moves the AURA orb to screen presets ('top-right', 'bottom-right', 'top-left', 'bottom-left', 'center', 'roam', 'stop').\n\
             CRITICAL BEHAVIORAL RULES:\n\
             1. Call tools whenever the user commands actions (open folder, search folder, create folder, open app, change color, minimize) or asks for information (e.g. 'what is my name' -> search_vectordb_memory, 'where is the folder' -> search_desktop_files_or_folders, 'where are you' -> get_current_desktop_location).\n\
             2. You can call multiple tools simultaneously in a single turn if needed (e.g. create a folder and open it, or change color and move).\n\
             3. NEVER speak or narrate tool names, tool calls, or background processes (never say 'I am using the search tool' or 'Calling find_and_open_folder'). Perform all tools silently.\n\
             4. Speak directly, warmly, and concisely to the user (1-2 sentences) about the outcome.\n",
            location_info
        );

        let session_history = self.get_session_history_summary();
        if !session_history.is_empty() {
            context_str.push_str(&session_history);
        }

        if !retrieved_context.is_empty() {
            context_str.push_str("\n[Initial long-term memories retrieved from ChromaDB]:\n");
            for m in &retrieved_context {
                context_str.push_str(&format!("- {}\n", m.text));
            }
        }
        if screen_image_b64.is_some() {
            context_str.push_str("\n[Screen Vision Active]: You have a real-time capture of the user's desktop screen.\n");
        }

        // 6. Generate Response using active LLM (Gemini 2.0 Flash by default) with tool support
        let llm_guard = self.llm.lock().await;
        let model_name = llm_guard.config.active_model.display_name();
        crate::logger::log("ORCHESTRATOR", "INFO", &format!("Processing message with model: {}", model_name));

        let llm_output = if let Some(ref write_req) = pending_write {
            crate::logger::log("SECURITY", "WARN", &format!("Write action guarded: {}", write_req));
            crate::llm::LlmOutput::Text(format!(
                "I detected an action that modifies files: '{}'. Under our strict safety policy, please click 'Authorize' on the Golden Orb security prompt to proceed.",
                write_req
            ))
        } else {
            match llm_guard
                .generate_with_tools(
                    user_text,
                    &context_str,
                    screen_image_b64.as_deref(),
                )
                .await
            {
                Ok(out) => out,
                Err(err) => {
                    crate::logger::log("ORCHESTRATOR", "ERROR", &format!("LLM Error: {}", err));
                    crate::llm::LlmOutput::Text("I had a brief connection issue reaching the AI service. Please try asking again in a moment.".to_string())
                }
            }
        };
        drop(llm_guard);

        // 7. Execute tools or process text output
        let mut final_reply;
        let mut color_action: Option<String> = None;
        let mut pc_action: Option<String> = None;

        match llm_output {
            crate::llm::LlmOutput::ToolCalls { calls, raw_parts, combined_prompt } => {
                crate::logger::log("ORCHESTRATOR", "INFO", &format!("Executing {} tool calls simultaneously...", calls.len()));
                let mut tool_responses = Vec::new();
                let mut session_actions: Vec<(String, String)> = Vec::new();
                let mut should_clear_session = false;

                for call in &calls {
                    crate::logger::log("TOOLS", "INFO", &format!("Tool Call: '{}' with {:?}", call.name, call.args));
                    match call.name.as_str() {
                        "find_and_open_folder" => {
                            let folder_name = call.args.get("folder_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let res = crate::pc_controller::PcController::find_and_open_folder(folder_name);
                            pc_action = Some(format!("open_folder {}", folder_name));
                            session_actions.push((
                                "find_and_open_folder".to_string(),
                                format!("Opened folder '{}': {}", folder_name, res.message),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": if res.success { "success" } else { "not_found" },
                                "message": res.message
                            })));
                        }
                        "search_folders" => {
                            let query = call.args.get("query")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let res = crate::pc_controller::PcController::search_folders(query);
                            session_actions.push((
                                "search_folders".to_string(),
                                format!("Searched folders for '{}': {}", query, res.message),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": if res.success { "success" } else { "not_found" },
                                "result": res.message
                            })));
                        }
                        "get_current_desktop_location" => {
                            let mut mov = self.movement.lock().await;
                            let loc = mov.get_location_description();
                            session_actions.push((
                                "get_current_desktop_location".to_string(),
                                format!("Live desktop location: {}", loc),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": "success",
                                "location": loc
                            })));
                        }
                        "clear_session_context" => {
                            should_clear_session = true;
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": "success",
                                "message": "Active session context and history cleared."
                            })));
                        }
                        "create_desktop_folder" => {
                            let folder_name = call.args.get("folder_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("New Folder");
                            let res = crate::pc_controller::PcController::create_folder(folder_name, None);
                            pc_action = Some(format!("create_folder {}", folder_name));
                            session_actions.push((
                                "create_desktop_folder".to_string(),
                                format!("Created desktop folder '{}': {}", folder_name, res.message),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": if res.success { "success" } else { "error" },
                                "message": res.message
                            })));
                        }
                        "search_desktop_files_or_folders" => {
                            let query = call.args.get("query")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let res = crate::pc_controller::PcController::search_desktop_files_or_folders(query);
                            session_actions.push((
                                "search_desktop_files_or_folders".to_string(),
                                format!("Searched desktop for '{}': {}", query, res.message),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": if res.success { "success" } else { "not_found" },
                                "result": res.message
                            })));
                        }
                        "search_vectordb_memory" => {
                            let query = call.args.get("query")
                                .and_then(|v| v.as_str())
                                .unwrap_or(user_text);
                            let memory_guard = self.memory.lock().await;
                            let mems = memory_guard.query_relevant_memories(query, 5).await;
                            drop(memory_guard);
                            let mem_texts: Vec<String> = mems.iter().map(|m| m.text.clone()).collect();
                            session_actions.push((
                                "search_vectordb_memory".to_string(),
                                format!("Retrieved memory for '{}': {} items", query, mem_texts.len()),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": "success",
                                "memories_found": mem_texts
                            })));
                        }
                        "open_folder_or_app" => {
                            let target = call.args.get("target")
                                .and_then(|v| v.as_str())
                                .unwrap_or("desktop");
                            let res = crate::pc_controller::PcController::open_path_or_app(target);
                            pc_action = Some(format!("open {}", target));
                            session_actions.push((
                                "open_folder_or_app".to_string(),
                                format!("Opened target '{}': {}", target, res.message),
                            ));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": if res.success { "success" } else { "error" },
                                "message": res.message
                            })));
                        }
                        "minimize_window" => {
                            if let Some(ref handle) = self.app_handle {
                                let res = crate::pc_controller::PcController::minimize_window(handle);
                                pc_action = Some("minimize".to_string());
                                session_actions.push(("minimize_window".to_string(), "Minimized window".to_string()));
                                tool_responses.push((call.name.clone(), serde_json::json!({
                                    "status": "success",
                                    "message": res.message
                                })));
                            } else {
                                tool_responses.push((call.name.clone(), serde_json::json!({
                                    "status": "success",
                                    "message": "Window minimize requested"
                                })));
                            }
                        }
                        "restore_window" => {
                            if let Some(ref handle) = self.app_handle {
                                let res = crate::pc_controller::PcController::unminimize_window(handle);
                                pc_action = Some("restore".to_string());
                                session_actions.push(("restore_window".to_string(), "Restored window".to_string()));
                                tool_responses.push((call.name.clone(), serde_json::json!({
                                    "status": "success",
                                    "message": res.message
                                })));
                            } else {
                                tool_responses.push((call.name.clone(), serde_json::json!({
                                    "status": "success",
                                    "message": "Window restore requested"
                                })));
                            }
                        }
                        "change_ui_color" => {
                            let color = call.args.get("color")
                                .and_then(|v| v.as_str())
                                .unwrap_or("default");
                            color_action = Some(color.to_string());
                            if let Some(ref handle) = self.app_handle {
                                use tauri::Emitter;
                                let _ = handle.emit("aura-color", serde_json::json!({ "color": color }));
                            }
                            session_actions.push(("change_ui_color".to_string(), format!("Changed UI color to '{}'", color)));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": "success",
                                "color": color
                            })));
                        }
                        "move_desktop_position" => {
                            let pos = call.args.get("position")
                                .and_then(|v| v.as_str())
                                .unwrap_or("top-right");
                            let mov_arc = self.movement.clone();
                            let pos_str = pos.to_string();
                            tauri::async_runtime::spawn(async move {
                                let mut mov = mov_arc.lock().await;
                                if pos_str == "roam" {
                                    mov.start_autonomous_roam();
                                } else if pos_str == "stop" || pos_str == "stay" {
                                    mov.stop_roaming();
                                } else {
                                    let _ = mov.move_to_preset(&pos_str).await;
                                }
                            });
                            session_actions.push(("move_desktop_position".to_string(), format!("Moved position to '{}'", pos)));
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": "success",
                                "new_position": pos
                            })));
                        }
                        _ => {
                            tool_responses.push((call.name.clone(), serde_json::json!({
                                "status": "unsupported_tool"
                            })));
                        }
                    }
                }

                // Update runtime session history
                if should_clear_session {
                    self.session_tool_history.clear();
                } else {
                    for (tool, summary) in session_actions {
                        self.record_session_action(&tool, &summary);
                    }
                }

                // Check if any tool called was an informational query (search_vectordb_memory or search_folders)
                let has_info_query = calls.iter().any(|c| c.name == "search_vectordb_memory" || c.name == "search_folders");

                if has_info_query {
                    let llm_guard = self.llm.lock().await;
                    final_reply = match llm_guard.send_tool_results_to_gemini(&combined_prompt, raw_parts, tool_responses).await {
                        Ok(speech) if !speech.trim().is_empty() => speech,
                        _ => "Here is what I found in your memory records.".to_string(),
                    };
                } else {
                    // Action tools (move, open folder, create folder, minimize, restore, color):
                    // Direct instantaneous local natural reply (0ms network delay!)
                    final_reply = if let Some(ref action) = pc_action {
                        if action.starts_with("open_folder") {
                            let name = action.trim_start_matches("open_folder").trim();
                            format!("I've opened the '{}' folder for you.", name)
                        } else if action.starts_with("create_folder") {
                            let name = action.trim_start_matches("create_folder").trim();
                            format!("I've created the '{}' folder on your desktop.", name)
                        } else if action == "minimize" {
                            "I've minimized the window for you.".to_string()
                        } else if action == "restore" {
                            "I've restored the window for you.".to_string()
                        } else {
                            "I've completed that for you.".to_string()
                        }
                    } else if let Some(ref col) = color_action {
                        format!("I've changed my color to {}.", col)
                    } else {
                        "Done! I've completed your request.".to_string()
                    };
                }
            }
            crate::llm::LlmOutput::Text(t) => {
                final_reply = t;
                let mut movement_action: Option<String> = None;

                // Fallback tag parsing if model returned tags in text
                if let Some(start) = final_reply.find("[MOVE:") {
                    if let Some(end) = final_reply[start..].find(']') {
                        let tag = &final_reply[start..start + end + 1];
                        let action = tag.trim_start_matches("[MOVE:").trim_end_matches(']').trim().to_lowercase();
                        movement_action = Some(action);
                        final_reply = final_reply.replace(tag, "").trim().to_string();
                    }
                }

                if let Some(start) = final_reply.find("[COLOR:") {
                    if let Some(end) = final_reply[start..].find(']') {
                        let tag = &final_reply[start..start + end + 1];
                        let color_val = tag.trim_start_matches("[COLOR:").trim_end_matches(']').trim().to_lowercase();
                        color_action = Some(color_val);
                        final_reply = final_reply.replace(tag, "").trim().to_string();
                    }
                }

                if let Some(start) = final_reply.find("[PC:") {
                    if let Some(end) = final_reply[start..].find(']') {
                        let tag = &final_reply[start..start + end + 1];
                        let action = tag.trim_start_matches("[PC:").trim_end_matches(']').trim().to_string();
                        pc_action = Some(action);
                        final_reply = final_reply.replace(tag, "").trim().to_string();
                    }
                }

                if let Some(action) = movement_action {
                    let mov_arc = self.movement.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut mov = mov_arc.lock().await;
                        if action == "roam" {
                            mov.start_autonomous_roam();
                        } else if action == "stop" || action == "stay" || action == "stationary" {
                            mov.stop_roaming();
                        } else {
                            let _ = mov.move_to_preset(&action).await;
                        }
                    });
                }
            }
        }

        let reply = final_reply;

        let now = chrono::Utc::now().timestamp();
        self.conversation_history.push(ChatMessage {
            role: "agent".to_string(),
            content: reply.clone(),
            timestamp: now,
        });

        // Note: Transient session context is maintained in-memory in self.session_tool_history
        // and is naturally forgotten on session restart without polluting ChromaDB.
        // ChromaDB is strictly reserved for durable extracted user facts.

        AgentResponse {
            reply,
            model_used: model_name,
            extracted_memories: extracted_facts,
            retrieved_context,
            screen_seen: screen_image_b64.is_some(),
            pending_write_action: pending_write,
            color_action,
            pc_action,
        }
    }

    pub async fn set_model(&mut self, model_id: &str) {
        let mut llm_guard = self.llm.lock().await;
        llm_guard.set_model(model_id);
    }

    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let llm_guard = self.llm.lock().await;
        llm_guard.list_models()
    }
}
