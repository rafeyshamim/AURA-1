# AURA-1 : Personal AI Desktop Agent

A cross-platform personal AI agent built with **Rust** and **Tauri v2**, featuring an interactive **Golden Hovering Ball GUI**, **ChromaDB vector memory**, a **strict write-permission guardrail**, and **Model Context Protocol (MCP)** extensibility.

---

## 🌟 Key Features

1. **🟡 Interactive Golden Hovering Orb GUI:**
   - 3D GPU-accelerated glowing sphere with floating physics and soundwave ripples.
   - Automatically adapts its background to Windows 11 **Mica / Acrylic blur** and system **Dark / Light mode**.
   - Reactive states: *Idle*, *Listening*, *Querying ChromaDB*, *Speaking*, and *Security Authorization Alert*.

2. **🧠 ChromaDB Long-Term Memory:**
   - Background conversation extractor in Rust parses dialog for key personal facts, habits, and preferences.
   - Embeds facts into local ChromaDB (`/api/v1/collections/user_personal_memory`).
   - Automatically retrieves semantic context before responding.

3. **🛡️ OS Command Safety Guardrail:**
   - **Safe Reads**: File reading, directory listings, and system inspection execute seamlessly.
   - **Guarded Writes**: Any file creation, file modification, deletion, or shell command **suspends execution** and prompts you via the Golden Orb's authorization modal.

4. **🔌 MCP (Model Context Protocol) & Plugin Support:**
   - Pre-configured `mcp_servers.json` allows connecting external MCP servers (Filesystem, Brave Search, GitHub, Postgres) or custom Python/Rust scripts.

---

## 📂 Project Structure

```
Project 7/
├── ai_agent_plan_and_preview.html   # Full architectural plan with interactive live demo
├── mcp_servers.json                 # MCP external tools & plugins config
├── package.json                     # Tauri CLI runner configuration
├── ui/                              # Golden Hovering Orb Desktop Frontend
│   ├── index.html                   # Borderless draggable desktop widget
│   ├── style.css                    # 3D orb shaders, mica glass, and states
│   └── orb.js                       # State machine, IPC bridge, permission resolver
└── src-tauri/                       # Native Rust Core Engine
    ├── Cargo.toml                   # Dependencies (Tauri v2, Tokio, Reqwest, Serde)
    ├── tauri.conf.json              # Borderless transparent window config
    └── src/
        ├── main.rs                  # Entry point & Tauri IPC handlers
        ├── orchestrator.rs          # Dialogue loop & ChromaDB context injector
        ├── memory.rs                # ChromaDB vector client & detail extractor
        ├── security.rs              # Safe read vs guarded write permission engine
        └── mcp.rs                   # Model Context Protocol plugin hub
```

---

## 🚀 Quick Start

### 1. Instant UI Preview (Zero Installation)
Simply double-click or open either of these in any browser:
* **[ai_agent_plan_and_preview.html](file:///c:/Users/rafey/OneDrive/Desktop/Project%207/ai_agent_plan_and_preview.html)**: Interactive architecture plan + live Golden Orb simulator + theme tester.
* **[ui/index.html](file:///c:/Users/rafey/OneDrive/Desktop/Project%207/ui/index.html)**: The standalone Golden Orb desktop widget.

### 2. Setting Up ChromaDB (Vector Memory)
Install and run ChromaDB in Python:
```powershell
pip install chromadb
chroma run --path ./chroma_data --port 8000
```

### 3. Setting Up Rust & Compiling the Native Desktop Binary
Install Rust via winget:
```powershell
winget install Rustlang.Rustup
```
Restart your PowerShell, then run the desktop application:
```powershell
npx @tauri-apps/cli dev
```
