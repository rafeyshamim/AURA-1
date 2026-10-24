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
        }
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

        // 1. Query ChromaDB for relevant personal memories & extract facts concurrently
        let memory_guard = self.memory.lock().await;
        let retrieved_context = memory_guard.query_relevant_memories(user_text, 3).await;
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
            "You are AURA-1, a personal desktop companion AI with a high-performance Rust core.\n\
             [Current Desktop Location]: {}\n\
             You possess autonomous desktop movement capabilities. You can physically glide across the user's screen or float around.\n\
             - If the user asks where you are, state your location naturally (e.g. 'I am at the top right of your screen'). CRITICAL: NEVER mention numbers, pixel coordinates, or screen dimensions.\n\
             - If the user commands you to move (e.g. 'move to top right', 'move to center', 'dock at bottom right', 'move to left', 'roam around', 'float around', 'stop moving'), append one of these tags at the end of your response:\n\
               [MOVE: top-right]\n\
               [MOVE: bottom-right]\n\
               [MOVE: top-left]\n\
               [MOVE: bottom-left]\n\
               [MOVE: center]\n\
               [MOVE: roam]\n\
               [MOVE: stop]\n\
             Respond naturally and concisely in 1-2 sentences.\n",
            location_info
        );
        context_str.push_str(
            "- Dynamic UI Color Theme:\n\
             If the user asks to change your color (e.g., 'change color to red', 'turn purple', 'make yourself green', 'switch to blue', 'yellow', 'cyan', 'pink', 'orange', 'white'), acknowledge it warmly and append `[COLOR: <color_name_or_hex>]` at the very end of your response.\n\
             - Desktop PC Control Automation:\n\
             You have native Windows desktop controller integration.\n\
             If the user commands you to perform PC actions like minimizing the window, restoring the window, creating a folder, or opening an application/folder:\n\
             - Minimize window: append `[PC: minimize]`\n\
             - Restore/show window: append `[PC: restore]`\n\
             - Create a new folder (Desktop): append `[PC: create_folder <folder_name>]` (e.g. `[PC: create_folder Project Files]`)\n\
             - Open an app or folder: append `[PC: open <app_name_or_path>]` (e.g. `[PC: open notepad]`, `[PC: open calc]`, `[PC: open desktop]`)\n\
             Keep your spoken replies natural, friendly, and concise (1-2 sentences).\n"
        );
        if !retrieved_context.is_empty() {
            context_str.push_str("\n[Long-term memories retrieved from ChromaDB]:\n");
            for m in &retrieved_context {
                context_str.push_str(&format!("- {}\n", m.text));
            }
        }
        if screen_image_b64.is_some() {
            context_str.push_str("\n[Screen Vision Active]: You have a real-time capture of the user's desktop screen.\n");
        }

        // 6. Generate Response using active LLM (Gemini 2.0 Flash by default)
        let llm_guard = self.llm.lock().await;
        let model_name = llm_guard.config.active_model.display_name();
        crate::logger::log("ORCHESTRATOR", "INFO", &format!("Processing message with model: {}", model_name));

        let raw_reply = if let Some(ref write_req) = pending_write {
            crate::logger::log("SECURITY", "WARN", &format!("Write action guarded: {}", write_req));
            format!(
                "I detected an action that modifies files: '{}'. Under our strict safety policy, please click 'Authorize' on the Golden Orb security prompt to proceed.",
                write_req
            )
        } else {
            match llm_guard
                .generate(
                    user_text,
                    &context_str,
                    screen_image_b64.as_deref(),
                )
                .await
            {
                Ok(llm_reply) if !llm_reply.trim().is_empty() => {
                    crate::logger::log("ORCHESTRATOR", "SUCCESS", &format!("Agent reply ready ({} chars)", llm_reply.len()));
                    llm_reply
                }
                Ok(_) => {
                    crate::logger::log("ORCHESTRATOR", "WARN", "Empty reply from LLM, using fallback acknowledgement.");
                    "I received your message and updated your memory context.".to_string()
                }
                Err(err) => {
                    crate::logger::log("ORCHESTRATOR", "ERROR", &format!("LLM Error: {}", err));
                    "I had a brief connection issue reaching the AI service. Please try asking again in a moment.".to_string()
                }
            }
        };
        drop(llm_guard);

        // 7. Parse and execute automated movement actions
        let mut final_reply = raw_reply;
        let mut movement_action: Option<String> = None;
        let mut color_action: Option<String> = None;
        let mut pc_action: Option<String> = None;

        // Parse [MOVE: ...]
        if let Some(start) = final_reply.find("[MOVE:") {
            if let Some(end) = final_reply[start..].find(']') {
                let tag = &final_reply[start..start + end + 1];
                let action = tag.trim_start_matches("[MOVE:").trim_end_matches(']').trim().to_lowercase();
                movement_action = Some(action);
                final_reply = final_reply.replace(tag, "").trim().to_string();
            }
        }

        // Parse [COLOR: ...]
        if let Some(start) = final_reply.find("[COLOR:") {
            if let Some(end) = final_reply[start..].find(']') {
                let tag = &final_reply[start..start + end + 1];
                let color_val = tag.trim_start_matches("[COLOR:").trim_end_matches(']').trim().to_lowercase();
                color_action = Some(color_val);
                final_reply = final_reply.replace(tag, "").trim().to_string();
            }
        }

        // Parse [PC: ...]
        if let Some(start) = final_reply.find("[PC:") {
            if let Some(end) = final_reply[start..].find(']') {
                let tag = &final_reply[start..start + end + 1];
                let action = tag.trim_start_matches("[PC:").trim_end_matches(']').trim().to_string();
                pc_action = Some(action);
                final_reply = final_reply.replace(tag, "").trim().to_string();
            }
        }

        // Direct fallback keyword matching for movement
        let lower_input = user_text.to_lowercase();
        if movement_action.is_none() {
            if lower_input.contains("top right") || lower_input.contains("top-right") {
                movement_action = Some("top-right".to_string());
            } else if lower_input.contains("bottom right") || lower_input.contains("bottom-right") || lower_input.contains("dock") {
                movement_action = Some("bottom-right".to_string());
            } else if lower_input.contains("top left") || lower_input.contains("top-left") {
                movement_action = Some("top-left".to_string());
            } else if lower_input.contains("bottom left") || lower_input.contains("bottom-left") {
                movement_action = Some("bottom-left".to_string());
            } else if lower_input.contains("center") || lower_input.contains("middle") {
                movement_action = Some("center".to_string());
            } else if lower_input.contains("roam") || lower_input.contains("float") {
                movement_action = Some("roam".to_string());
            } else if lower_input.contains("stop moving") || lower_input.contains("stay still") || lower_input.contains("stop roam") {
                movement_action = Some("stop".to_string());
            }
        }

        // Direct fallback keyword matching for color change
        if color_action.is_none() {
            let colors = ["red", "blue", "purple", "violet", "pink", "green", "emerald", "gold", "yellow", "orange", "cyan", "white", "black"];
            for &c in &colors {
                if lower_input.contains(&format!("change to {}", c))
                    || lower_input.contains(&format!("turn {}", c))
                    || lower_input.contains(&format!("color to {}", c))
                    || lower_input.contains(&format!("make it {}", c))
                    || lower_input.contains(&format!("set color {}", c))
                {
                    color_action = Some(c.to_string());
                    break;
                }
            }
        }

        // Direct fallback keyword matching for PC actions
        if pc_action.is_none() {
            if lower_input.contains("minimize") {
                pc_action = Some("minimize".to_string());
            } else if lower_input.contains("unminimize") || lower_input.contains("restore window") {
                pc_action = Some("restore".to_string());
            } else if lower_input.contains("create folder") || lower_input.contains("make a folder") || lower_input.contains("make a new folder") || lower_input.contains("create a folder") || lower_input.contains("new folder") {
                let name = if let Some(idx) = lower_input.find("named") {
                    user_text[idx + 5..].trim().trim_matches('"').trim_matches('\'').to_string()
                } else if let Some(idx) = lower_input.find("folder") {
                    let sub = user_text[idx + 6..].trim();
                    let candidate = sub.trim_start_matches("called").trim_start_matches("named").trim();
                    if candidate.is_empty() { "New Folder".to_string() } else { candidate.to_string() }
                } else {
                    "New Folder".to_string()
                };
                pc_action = Some(format!("create_folder {}", name));
            } else if lower_input.contains("open notepad") {
                pc_action = Some("open notepad.exe".to_string());
            } else if lower_input.contains("open calculator") || lower_input.contains("open calc") {
                pc_action = Some("open calc.exe".to_string());
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

        let reply = final_reply;

        let now = chrono::Utc::now().timestamp();
        self.conversation_history.push(ChatMessage {
            role: "agent".to_string(),
            content: reply.clone(),
            timestamp: now,
        });

        // Continuously append conversation turn context into ChromaDB vector memory
        let convo_context_item = MemoryItem {
            id: uuid::Uuid::new_v4().to_string(),
            text: format!("Conversation Turn: User said \"{}\" | AURA replied \"{}\"", user_text, reply),
            category: "conversation_context".to_string(),
            timestamp: now,
            confidence: 1.0,
            embedding: None,
        };

        let mem_arc = self.memory.clone();
        tokio::spawn(async move {
            let mut mg = mem_arc.lock().await;
            let _ = mg.store_memory(convo_context_item).await;
        });

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
