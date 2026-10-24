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

                                    // Watchdog: force-recover if stuck for more than 12 seconds
                                    let watchdog_busy = is_busy.clone();
                                    let watchdog_app = app_handle.clone();
                                    tauri::async_runtime::spawn(async move {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(12)).await;
                                        if watchdog_busy.swap(false, std::sync::atomic::Ordering::SeqCst) {
                                            crate::logger::log("SPEECH", "WARN", "Speech watchdog: auto-recovered agent from stuck busy state.");
                                            let _ = watchdog_app.emit("aura-state", serde_json::json!({
                                                "state": "listening",
                                                "text": ""
                                            }));
                                        }
                                    });

                                    tauri::async_runtime::spawn(async move {
                                        let mut orch_guard = orch.lock().await;
                                        let res = orch_guard.process_user_message(&user_text, false).await;
                                        crate::logger::log("LLM", "SUCCESS", &format!("Reply: '{}'", res.reply));

                                        // Color and PC tools are executed atomically and concurrently inside orchestrator
                                        if let Some(ref color) = res.color_action {
                                            let _ = app_h.emit("aura-color", serde_json::json!({
                                                "color": color
                                            }));
                                        }

                                        let _ = app_h.emit("aura-agent-speech", serde_json::json!({
                                            "text": res.reply
                                        }));

                                        if res.reply.trim().is_empty() {
                                            busy_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                                            let _ = app_h.emit("aura-state", serde_json::json!({
                                                "state": "listening",
                                                "text": ""
                                            }));
                                        } else {
                                            Self::write_command(&writer, "speak", &res.reply);
                                        }
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
