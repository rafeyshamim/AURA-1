use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as TokioMutex;
use crate::orchestrator::AgentOrchestrator;

pub struct HandTrackerProcess {
    stdin: Arc<StdMutex<Option<ChildStdin>>>,
    child: Arc<StdMutex<Option<Child>>>,
    pub is_tracking: Arc<AtomicBool>,
    pub latest_snapshot_b64: Arc<StdMutex<Option<String>>>,
}

impl HandTrackerProcess {
    pub fn spawn(app: AppHandle, orchestrator: Arc<TokioMutex<AgentOrchestrator>>) -> Arc<Self> {
        crate::logger::log("VISION", "INFO", "Spawning AURA Computer Vision & Hand Tracker process...");

        // Resolve absolute path to hand_tracker.py
        let candidates = [
            std::path::PathBuf::from("hand_tracker.py"),
            std::path::PathBuf::from("../hand_tracker.py"),
            std::path::PathBuf::from("src-tauri/hand_tracker.py"),
        ];

        let mut script_path = std::path::PathBuf::from("hand_tracker.py");
        for c in &candidates {
            if c.exists() {
                if let Ok(abs) = std::fs::canonicalize(c) {
                    script_path = abs;
                    break;
                }
            }
        }

        let script_dir = script_path.parent().unwrap_or(std::path::Path::new("."));
        crate::logger::log("VISION", "INFO", &format!("Hand tracker script path: {:?}", script_path));

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
                crate::logger::log("VISION", "ERROR", &format!("Failed to start hand_tracker.py: {}", e));
                return Arc::new(Self {
                    stdin: Arc::new(StdMutex::new(None)),
                    child: Arc::new(StdMutex::new(None)),
                    is_tracking: Arc::new(AtomicBool::new(false)),
                    latest_snapshot_b64: Arc::new(StdMutex::new(None)),
                });
            }
        };

        let child_stdin = child.stdin.take();
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let stdin_arc = Arc::new(StdMutex::new(child_stdin));
        let stdin_writer = stdin_arc.clone();
        let child_arc = Arc::new(StdMutex::new(Some(child)));
        let is_tracking_flag = Arc::new(AtomicBool::new(false));
        let is_tracking_reader = is_tracking_flag.clone();
        let snapshot_arc = Arc::new(StdMutex::new(None));
        let snapshot_writer = snapshot_arc.clone();

        // Stderr reader thread
        if let Some(err_pipe) = child_stderr {
            std::thread::spawn(move || {
                let reader = BufReader::new(err_pipe);
                for line in reader.lines().flatten() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.contains("oneDNN") && !trimmed.contains("absl") {
                        crate::logger::log("VISION", "WARN", trimmed);
                    }
                }
            });
        }

        // Stdout reader thread
        if let Some(out_pipe) = child_stdout {
            let app_handle = app.clone();
            let orch_arc = orchestrator.clone();
            let is_tracking_for_stdout = is_tracking_reader.clone();
            let snapshot_receiver = snapshot_writer.clone();

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
                            "ready" => {
                                crate::logger::log("VISION", "SUCCESS", "AURA Computer Vision & Hand Tracker is ready.");
                            }
                            "status" => {
                                let tracking = val.get("tracking").and_then(|v| v.as_bool()).unwrap_or(false);
                                is_tracking_for_stdout.store(tracking, Ordering::SeqCst);
                                let msg = val.get("message").and_then(|v| v.as_str()).unwrap_or("");
                                crate::logger::log("VISION", "INFO", &format!("Hand tracking status: {} ({})", tracking, msg));
                                let _ = app_handle.emit("aura-hand-tracking-status", serde_json::json!({
                                    "active": tracking,
                                    "message": msg
                                }));
                            }
                            "webcam_snapshot" => {
                                let success = val.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                                if success {
                                    let b64 = val.get("image_base64").and_then(|v| v.as_str()).unwrap_or("");
                                    if !b64.is_empty() {
                                        if let Ok(mut guard) = snapshot_receiver.lock() {
                                            *guard = Some(b64.to_string());
                                        }
                                        crate::logger::log("VISION", "SUCCESS", "Webcam snapshot captured successfully.");
                                    }
                                } else {
                                    let msg = val.get("message").and_then(|v| v.as_str()).unwrap_or("Failed");
                                    crate::logger::log("VISION", "WARN", &format!("Webcam snapshot error: {}", msg));
                                }
                            }
                            "hand_move" => {
                                let detected = val.get("detected").and_then(|v| v.as_bool()).unwrap_or(false);
                                if detected && is_tracking_for_stdout.load(Ordering::SeqCst) {
                                    let nx = val.get("norm_x").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                                    let ny = val.get("norm_y").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;

                                    // Update AURA window position smoothly in real time
                                    let orch_clone = orch_arc.clone();
                                    tauri::async_runtime::spawn(async move {
                                        let orch = orch_clone.lock().await;
                                        let mut mov = orch.movement.lock().await;
                                        mov.update_hand_position(nx, ny);
                                    });

                                    let _ = app_handle.emit("aura-hand-position", serde_json::json!({
                                        "x": nx,
                                        "y": ny,
                                        "detected": true
                                    }));
                                } else {
                                    let _ = app_handle.emit("aura-hand-position", serde_json::json!({
                                        "detected": false
                                    }));
                                }
                            }
                            "gesture" => {
                                let gesture = val.get("gesture").and_then(|v| v.as_str()).unwrap_or("");
                                crate::logger::log("VISION", "SUCCESS", &format!("Detected gesture: '{}'", gesture));
                                
                                let _ = app_handle.emit("aura-gesture", serde_json::json!({
                                    "gesture": gesture
                                }));

                                let orch_clone = orch_arc.clone();
                                let gesture_str = gesture.to_string();
                                let app_h = app_handle.clone();
                                tauri::async_runtime::spawn(async move {
                                    let orch = orch_clone.lock().await;
                                    let mut mov = orch.movement.lock().await;
                                    match gesture_str.as_str() {
                                        "swipe_right" => {
                                            crate::logger::log("MOVEMENT", "INFO", "Gesture: Swipe Right -> Gliding directly to Right");
                                            let _ = mov.move_to_preset("middle-right").await;
                                        }
                                        "swipe_left" => {
                                            crate::logger::log("MOVEMENT", "INFO", "Gesture: Swipe Left -> Gliding directly to Left");
                                            let _ = mov.move_to_preset("middle-left").await;
                                        }
                                        "swipe_up" => {
                                            crate::logger::log("MOVEMENT", "INFO", "Gesture: Swipe Up -> Gliding directly Up");
                                            let _ = mov.move_to_preset("top-center").await;
                                        }
                                        "swipe_down" => {
                                            crate::logger::log("MOVEMENT", "INFO", "Gesture: Swipe Down -> Gliding directly Down");
                                            let _ = mov.move_to_preset("bottom-center").await;
                                        }
                                        "open_palm" => {
                                            crate::logger::log("MOVEMENT", "INFO", "Gesture: Open Palm -> Gliding to Center");
                                            let _ = mov.move_to_preset("center").await;
                                        }
                                        "fist" => {
                                            crate::logger::log("MOVEMENT", "INFO", "Gesture: Fist -> Halting window movement");
                                            mov.stop_roaming();
                                        }
                                        "pinch" => {
                                            crate::logger::log("PC_ACTION", "INFO", "Gesture: Pinch -> Minimizing AURA window");
                                            let _ = crate::pc_controller::PcController::minimize_window(&app_h);
                                        }
                                        "peace" => {
                                            crate::logger::log("THEME", "INFO", "Gesture: Peace V-Sign -> Cycling UI theme to Purple");
                                            let _ = app_h.emit("aura-color", serde_json::json!({ "color": "purple" }));
                                        }
                                        "thumbs_up" => {
                                            crate::logger::log("THEME", "INFO", "Gesture: Thumbs Up -> Affirmative Emerald glow");
                                            let _ = app_h.emit("aura-color", serde_json::json!({ "color": "emerald" }));
                                        }
                                        _ => {}
                                    }
                                });
                            }
                            "log" => {
                                let lvl = val.get("level").and_then(|v| v.as_str()).unwrap_or("INFO");
                                let msg = val.get("message").and_then(|v| v.as_str()).unwrap_or("");
                                crate::logger::log("VISION", lvl, msg);
                            }
                            _ => {}
                        }
                    }
                }
            });
        }

        Arc::new(Self {
            stdin: stdin_writer,
            child: child_arc,
            is_tracking: is_tracking_flag,
            latest_snapshot_b64: snapshot_arc,
        })
    }

    fn write_cmd(&self, cmd: &str) {
        if let Ok(mut guard) = self.stdin.lock() {
            if let Some(stdin) = guard.as_mut() {
                let payload = serde_json::json!({ "command": cmd });
                let line = format!("{}\n", payload);
                let _ = stdin.write_all(line.as_bytes());
                let _ = stdin.flush();
            }
        }
    }

    pub fn start_tracking(&self) {
        crate::logger::log("VISION", "INFO", "Sending command: start hand tracking");
        self.is_tracking.store(true, Ordering::SeqCst);
        self.write_cmd("start");
    }

    pub fn stop_tracking(&self) {
        crate::logger::log("VISION", "INFO", "Sending command: stop hand tracking and release camera");
        self.is_tracking.store(false, Ordering::SeqCst);
        self.write_cmd("stop");
    }

    pub fn is_active(&self) -> bool {
        self.is_tracking.load(Ordering::SeqCst)
    }

    /// Request a live photo from the user's webcam and return JPEG as base64 string
    pub async fn request_snapshot(&self) -> Option<String> {
        // Clear previous snapshot
        if let Ok(mut guard) = self.latest_snapshot_b64.lock() {
            *guard = None;
        }

        crate::logger::log("VISION", "INFO", "Requesting snapshot from webcam...");
        self.write_cmd("snapshot");

        // Wait up to 1.5s for snapshot to arrive from python
        for _ in 0..30 {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            if let Ok(guard) = self.latest_snapshot_b64.lock() {
                if let Some(ref b64) = *guard {
                    return Some(b64.clone());
                }
            }
        }

        // Fallback: check if webcam_snapshot.jpg was written to disk
        let candidates = [
            std::path::PathBuf::from("webcam_snapshot.jpg"),
            std::path::PathBuf::from("../webcam_snapshot.jpg"),
        ];
        for c in &candidates {
            if c.exists() {
                if let Ok(bytes) = std::fs::read(c) {
                    use base64::engine::general_purpose::STANDARD as BASE64;
                    use base64::Engine;
                    let b64 = BASE64.encode(&bytes);
                    if !b64.is_empty() {
                        return Some(b64);
                    }
                }
            }
        }

        None
    }

    pub fn kill(&self) {
        self.write_cmd("exit");
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
            }
        }
    }
}

impl Drop for HandTrackerProcess {
    fn drop(&mut self) {
        self.kill();
    }
}
