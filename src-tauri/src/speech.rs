use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as TokioMutex;
use crate::orchestrator::AgentOrchestrator;

pub struct SpeechProcess {
    stdin: Arc<StdMutex<Option<ChildStdin>>>,
}

impl SpeechProcess {
    pub fn spawn(app: AppHandle, orchestrator: Arc<TokioMutex<AgentOrchestrator>>) -> Arc<Self> {
        crate::logger::log("SPEECH", "INFO", "Spawning AURA offline speech engine sidecar...");

        // Resolve absolute path to speech_engine.py
        let candidates = [
            std::path::PathBuf::from("speech_engine.py"),
            std::path::PathBuf::from("../speech_engine.py"),
            std::path::PathBuf::from("src-tauri/speech_engine.py"),
        ];

        let mut script_path = std::path::PathBuf::from("speech_engine.py");
        for c in &candidates {
            if c.exists() {
                if let Ok(abs) = std::fs::canonicalize(c) {
                    script_path = abs;
                    break;
                }
            }
        }

        let script_dir = script_path.parent().unwrap_or(std::path::Path::new("."));
        crate::logger::log("SPEECH", "INFO", &format!("Speech engine script: {:?}", script_path));

        let mut cmd = Command::new("python");
        cmd.arg(&script_path);
        cmd.current_dir(script_dir);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                crate::logger::log("SPEECH", "ERROR", &format!("Failed to start speech_engine.py: {}", e));
                return Arc::new(Self {
                    stdin: Arc::new(StdMutex::new(None)),
                });
            }
        };

        let child_stdin = child.stdin.take();
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let stdin_arc = Arc::new(StdMutex::new(child_stdin));
        let stdin_for_reader = stdin_arc.clone();

        // Stderr reader thread
        if let Some(err_pipe) = child_stderr {
            std::thread::spawn(move || {
                let reader = BufReader::new(err_pipe);
                for line in reader.lines().flatten() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        crate::logger::log("PYTHON", "WARN", trimmed);
                    }
                }
            });
        }

        // Stdout reader thread
        if let Some(out_pipe) = child_stdout {
            let app_handle = app.clone();
            let orch_arc = orchestrator.clone();
            let stdin_writer = stdin_for_reader.clone();

            let is_busy = Arc::new(std::sync::atomic::AtomicBool::new(false));

            std::thread::spawn(move || {
                let reader = BufReader::new(out_pipe);
                for line in reader.lines().flatten() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                        let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match msg_type {
                            "log" => {
                                let cat = val.get("category").and_then(|v| v.as_str()).unwrap_or("SPEECH");
                                let lvl = val.get("level").and_then(|v| v.as_str()).unwrap_or("INFO");
                                let msg = val.get("message").and_then(|v| v.as_str()).unwrap_or("");
                                crate::logger::log(cat, lvl, msg);
                            }
                            "ready" => {
                                crate::logger::log("SPEECH", "SUCCESS", "Python Speech Engine is ready and active!");
                                let _ = app_handle.emit("aura-state", serde_json::json!({
                                    "state": "speaking",
                                    "text": "AURA-1 online. I am listening."
                                }));
                                Self::write_command(&stdin_writer, "speak", "AURA-1 online. I am listening.");
                            }
                            "state" => {
                                let state = val.get("state").and_then(|v| v.as_str()).unwrap_or("idle");
                                let text = val.get("text").and_then(|v| v.as_str()).unwrap_or("");
                                let _ = app_handle.emit("aura-state", serde_json::json!({
                                    "state": state,
                                    "text": text
                                }));
                            }
                            "speech_done" => {
                                is_busy.store(false, std::sync::atomic::Ordering::SeqCst);
                                let _ = app_handle.emit("aura-state", serde_json::json!({
                                    "state": "listening",
                                    "text": ""
                                }));
                            }
                            "transcription" => {
                                let user_text = val.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                if !user_text.is_empty() {
                                    // Prevent overlapping query processing if already working or speaking
                                    if is_busy.compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst).is_err() {
                                        crate::logger::log("SPEECH", "WARN", &format!("Dropping overlapping input while busy: '{}'", user_text));
                                        continue;
                                    }

                                    crate::logger::log("SPEECH", "SUCCESS", &format!("Transcribed: '{}'", user_text));
                                    let _ = app_handle.emit("aura-user-speech", serde_json::json!({
                                        "text": user_text
                                    }));
                                    let _ = app_handle.emit("aura-state", serde_json::json!({
                                        "state": "thinking",
                                        "text": format!("\"{}\"", user_text)
                                    }));

                                    let app_h = app_handle.clone();
                                    let orch = orch_arc.clone();
                                    let writer = stdin_writer.clone();
                                    let busy_flag = is_busy.clone();

                                    tauri::async_runtime::spawn(async move {
                                        let mut orch_guard = orch.lock().await;
                                        let res = orch_guard.process_user_message(&user_text, false).await;
                                        crate::logger::log("LLM", "SUCCESS", &format!("Reply: '{}'", res.reply));

                                        // 1. Emit Color Theme change if present
                                        if let Some(ref color) = res.color_action {
                                            crate::logger::log("THEME", "INFO", &format!("Applying dynamic color theme: '{}'", color));
                                            let _ = app_h.emit("aura-color", serde_json::json!({
                                                "color": color
                                            }));
                                        }

                                        // 2. Execute PC automation action if present
                                        if let Some(ref pc_cmd) = res.pc_action {
                                            crate::logger::log("PC_ACTION", "INFO", &format!("Executing PC voice command: '{}'", pc_cmd));
                                            let action_res = if pc_cmd == "minimize" {
                                                crate::pc_controller::PcController::minimize_window(&app_h)
                                            } else if pc_cmd == "restore" || pc_cmd == "unminimize" {
                                                crate::pc_controller::PcController::unminimize_window(&app_h)
                                            } else if let Some(folder_name) = pc_cmd.strip_prefix("create_folder ") {
                                                crate::pc_controller::PcController::create_folder(folder_name, None)
                                            } else if let Some(target) = pc_cmd.strip_prefix("open ") {
                                                crate::pc_controller::PcController::open_path_or_app(target)
                                            } else {
                                                crate::pc_controller::PcController::execute_voice_command(&app_h, pc_cmd)
                                                    .unwrap_or(crate::pc_controller::PcActionResult {
                                                        success: true,
                                                        message: "Command processed".to_string(),
                                                        action_type: "custom".to_string(),
                                                    })
                                            };
                                            crate::logger::log("PC_ACTION", if action_res.success { "SUCCESS" } else { "ERROR" }, &action_res.message);
                                        }

                                        let _ = app_h.emit("aura-agent-speech", serde_json::json!({
                                            "text": res.reply
                                        }));

                                        Self::write_command(&writer, "speak", &res.reply);
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
            });
        }

        Arc::new(Self { stdin: stdin_arc })
    }

    fn write_command(stdin_mutex: &Arc<StdMutex<Option<ChildStdin>>>, command: &str, text: &str) {
        if let Ok(mut guard) = stdin_mutex.lock() {
            if let Some(stdin) = guard.as_mut() {
                let payload = serde_json::json!({
                    "command": command,
                    "text": text
                });
                let line = format!("{}\n", payload);
                let _ = stdin.write_all(line.as_bytes());
                let _ = stdin.flush();
            }
        }
    }

    pub fn speak(&self, text: &str) {
        Self::write_command(&self.stdin, "speak", text);
    }

    pub fn stop(&self) {
        Self::write_command(&self.stdin, "stop", "");
    }
}
