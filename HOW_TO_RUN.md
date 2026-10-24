# 🟡 AURA-1 — Personal AI Desktop Agent

## Chalane Ka Tarika (How to Run)

---

### ⚡ Step 1: Gemini API Key Set Karo

Sabse pehle apni **Google Gemini API Key** chahiye. Free key yahan se lo:
👉 https://aistudio.google.com/apikey

Key milne ke baad, **PowerShell** mein ye command run karo:

```powershell
$env:GEMINI_API_KEY="YOUR-API-KEY-YAHAN-PASTE-KARO"
```

> ⚠️ **Quotes ke andar apni actual API key paste karna!** Ye key sirf us terminal session ke liye valid rehti hai.

---

### ⚡ Step 2: EXE Chalao

#### Option A: Direct EXE (Pre-built)
```powershell
# Pehle API key set karo (agar nahi kiya)
$env:GEMINI_API_KEY="your-key-here"

# Phir EXE chalao
& "c:\Users\rafey\OneDrive\Desktop\Project 7\AURA-1.exe"
```

#### Option B: Dev Mode (Live Reload)
```powershell
cd "c:\Users\rafey\OneDrive\Desktop\Project 7"
$env:GEMINI_API_KEY="your-key-here"
npx @tauri-apps/cli dev
```

---

### ⚡ Step 3: AURA-1 Use Karo

| Action | Kaise Kare |
|---|---|
| **Chat Open Karo** | Golden Orb (ball) par click karo |
| **Message Type Karo** | Neeche input box mein type karo, Enter dabao ya Send click karo |
| **Voice Se Bolo** | 🎙️ Listen button par click karo (8 seconds ka window milega) |
| **Real-time Logs Dekho** | Drawer ke upar **📜 Logs** tab par click karo (saare backend events live dikhenge!) |
| **Log File Notepad me Kholo** | Logs tab me **Open File** dabao ya `logs/aura.log` check karo |
| **Chat Band Karo** | × (close) button dabao |

---

### 🔑 API Key Permanently Set Karna (Har Baar Na Dalna Pade)

PowerShell mein ye run karo (sirf ek baar):

```powershell
[System.Environment]::SetEnvironmentVariable("GEMINI_API_KEY", "your-key-here", "User")
```

Iske baad **PowerShell band karke dobara kholo** — ab har jagah key available hogi.

---

### 🔄 Model Switch Karna

AURA-1 by default **Gemini 2.0 Flash** use karta hai. Agar tumhe doosra model use karna hai, toh future update mein model selector aayega. Filhal supported models:

| Model | Provider | Key Variable |
|---|---|---|
| **Gemini 2.0 Flash** (Default) | Google Cloud | `GEMINI_API_KEY` |
| Gemini 1.5 Pro | Google Cloud | `GEMINI_API_KEY` |
| GPT-4o | OpenAI | `OPENAI_API_KEY` |
| Claude 3.5 Sonnet | Anthropic | `ANTHROPIC_API_KEY` |
| Qwen 2.5 (1.5B) | Local Ollama | Koi key nahi chahiye |

---

### 🛠️ Rebuild Karna (Code Change Ke Baad)

Agar koi code change kiya hai toh rebuild zaroori hai:

```powershell
cd "c:\Users\rafey\OneDrive\Desktop\Project 7"
$env:CARGO_TARGET_DIR="C:/Users/rafey/.cargo_cache/project7_target"
npx @tauri-apps/cli build
```

Build hone ke baad naya EXE yahan milega:
```
C:\Users\rafey\.cargo_cache\project7_target\release\AURA-1.exe
```

Isko copy karke Project 7 folder mein rakh lo:
```powershell
Copy-Item "C:\Users\rafey\.cargo_cache\project7_target\release\AURA-1.exe" "c:\Users\rafey\OneDrive\Desktop\Project 7\AURA-1.exe" -Force
```

---

### ❌ Common Issues

| Issue | Fix |
|---|---|
| **"Gemini API key is not configured"** | `$env:GEMINI_API_KEY` set nahi hai, Step 1 dekho |
| **EXE open nahi ho raha** | Windows Defender block kar raha hoga — "More info" → "Run anyway" click karo |
| **Input kaam nahi kar raha** | Orb par click karke drawer kholo, phir type karo |
| **Voice kaam nahi kar raha** | Microphone permissions check karo Windows Settings mein |
| **Build fail ho raha hai** | Rust toolchain installed hai check karo: `rustc --version` |

---

### 📁 Project Structure

```
Project 7/
├── ui/                    ← Frontend (HTML/CSS/JS)
│   ├── index.html         ← Golden Orb + Chat Drawer
│   ├── style.css          ← All styling
│   └── orb.js             ← Frontend logic
├── src-tauri/             ← Rust Backend
│   ├── src/
│   │   ├── main.rs        ← Entry point + Tauri commands
│   │   ├── llm.rs         ← Gemini/GPT/Claude/Ollama engine
│   │   ├── orchestrator.rs← Agent brain (connects all modules)
│   │   ├── memory.rs      ← ChromaDB vector memory
│   │   ├── vision.rs      ← Real screen capture
│   │   ├── speech.rs      ← Windows SAPI TTS/STT
│   │   ├── security.rs    ← File read/write guardrails
│   │   ├── movement.rs    ← Orb autonomous movement
│   │   └── mcp.rs         ← Plugin/MCP tool system
│   └── tauri.conf.json    ← Window config
├── AURA-1.exe             ← Built executable
└── HOW_TO_RUN.md          ← Ye file!
```
