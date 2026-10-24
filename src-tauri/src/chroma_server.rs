use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub struct ChromaServerProcess {
    child: Arc<StdMutex<Option<Child>>>,
    port: u16,
}

impl ChromaServerProcess {
    pub fn is_port_in_use(port: u16) -> bool {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        TcpStream::connect_timeout(&addr, Duration::from_millis(350)).is_ok()
    }

    /// Locate chroma executable across common Windows and system locations
    pub fn find_chroma_bin() -> Option<PathBuf> {
        // 1. Direct Python 3.12 scripts directory in user profile
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let direct = PathBuf::from(&user_profile)
                .join("AppData")
                .join("Local")
                .join("Programs")
                .join("Python")
                .join("Python312")
                .join("Scripts")
                .join("chroma.exe");
            if direct.exists() {
                return Some(direct);
            }
        }

        if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
            let py_root = PathBuf::from(&local_app).join("Programs").join("Python");
            if let Ok(entries) = std::fs::read_dir(&py_root) {
                for entry in entries.flatten() {
                    let candidate = entry.path().join("Scripts").join("chroma.exe");
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }

        // 2. PATH resolution
        if let Ok(path_var) = std::env::var("PATH") {
            for p in std::env::split_paths(&path_var) {
                let candidate = p.join("chroma.exe");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        None
    }

    /// Spawns the ChromaDB server on port 8000 automatically
    pub fn spawn(app: AppHandle) -> Arc<Self> {
        let port = 8000;
        let child_arc = Arc::new(StdMutex::new(None));
        let process_instance = Arc::new(Self {
            child: child_arc.clone(),
            port,
        });

        // 1. Check if ChromaDB is already running on port 8000
        if Self::is_port_in_use(port) {
            crate::logger::log(
                "CHROMA",
                "SUCCESS",
                &format!("ChromaDB server is already active on http://127.0.0.1:{}.", port),
            );
            return process_instance;
        }

        crate::logger::log(
            "CHROMA",
            "INFO",
            &format!("Initializing ChromaDB vector server on http://127.0.0.1:{}...", port),
        );

        // Ensure persistent chroma_db directory exists in workspace
        let candidates = [
            PathBuf::from("chroma_db"),
            PathBuf::from("../chroma_db"),
            PathBuf::from("src-tauri/chroma_db"),
        ];

        let mut storage_path = PathBuf::from("chroma_db");
        for c in &candidates {
            if c.exists() {
                if let Ok(abs) = std::fs::canonicalize(c) {
                    storage_path = abs;
                    break;
                }
            }
        }
        let _ = std::fs::create_dir_all(&storage_path);

        // Find binary or fallback to python invocation
        let chroma_bin = Self::find_chroma_bin();
        let mut cmd = if let Some(bin) = chroma_bin {
            crate::logger::log("CHROMA", "INFO", &format!("Using Chroma binary: {:?}", bin));
            let mut c = Command::new(bin);
            c.arg("run");
            c.arg("--path");
            c.arg(&storage_path);
            c.arg("--port");
            c.arg(port.to_string());
            c.arg("--host");
            c.arg("127.0.0.1");
            c
        } else {
            crate::logger::log("CHROMA", "INFO", "Using Python chroma runner fallback...");
            let mut c = Command::new("python");
            c.arg("-c");
            c.arg("import chromadb_rust_bindings, sys; chromadb_rust_bindings.cli(sys.argv[1:])");
            c.arg("run");
            c.arg("--path");
            c.arg(&storage_path);
            c.arg("--port");
            c.arg(port.to_string());
            c.arg("--host");
            c.arg("127.0.0.1");
            c
        };

        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                crate::logger::log("CHROMA", "INFO", &format!("Spawned ChromaDB server (PID: {})", pid));
                if let Ok(mut guard) = child_arc.lock() {
                    *guard = Some(child);
                }

                // Asynchronously verify port readiness in background
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let mut online = false;
                    for _ in 0..40 {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        if Self::is_port_in_use(port) {
                            online = true;
                            break;
                        }
                    }

                    if online {
                        crate::logger::log(
                            "CHROMA",
                            "SUCCESS",
                            &format!("ChromaDB Vector Server online and listening on http://127.0.0.1:{}!", port),
                        );
                        let _ = app_handle.emit("chroma-status", serde_json::json!({
                            "status": "online",
                            "port": port,
                            "url": format!("http://127.0.0.1:{}", port)
                        }));
                    } else {
                        crate::logger::log(
                            "CHROMA",
                            "WARN",
                            &format!("ChromaDB spawned but port {} did not respond within 10s timeout.", port),
                        );
                    }
                });
            }
            Err(e) => {
                crate::logger::log("CHROMA", "ERROR", &format!("Failed to spawn ChromaDB server: {}", e));
            }
        }

        process_instance
    }

    /// Cleanly stop the Chroma server and kill the process tree
    pub fn stop(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let pid = child.id();
                crate::logger::log("CHROMA", "INFO", &format!("Terminating ChromaDB server (PID: {})...", pid));

                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    let _ = Command::new("taskkill")
                        .args(&["/F", "/T", "/PID", &pid.to_string()])
                        .creation_flags(0x08000000)
                        .spawn()
                        .and_then(|mut p| p.wait());
                }

                let _ = child.kill();
                let _ = child.wait();
                crate::logger::log("CHROMA", "INFO", "ChromaDB server stopped cleanly.");
            }
        }
    }
}

impl Drop for ChromaServerProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
