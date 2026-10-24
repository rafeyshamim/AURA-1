use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelChoice {
    #[serde(rename = "gemini-2.0-flash")]
    Gemini20Flash,
    #[serde(rename = "gemini-1.5-pro")]
    Gemini15Pro,
    #[serde(rename = "qwen2.5:1.5b")]
    OllamaQwen25,
    #[serde(rename = "claude-3-5-sonnet")]
    Claude35Sonnet,
    #[serde(rename = "gpt-4o")]
    Gpt4o,
    #[serde(rename = "custom")]
    Custom(String),
}

impl ModelChoice {
    pub fn from_str_id(id: &str) -> Self {
        match id.to_lowercase().as_str() {
            "gemini-2.0-flash" | "gemini-flash" | "gemini" | "google" => ModelChoice::Gemini20Flash,
            "gemini-1.5-pro" | "gemini-pro" => ModelChoice::Gemini15Pro,
            "qwen2.5:1.5b" | "qwen" | "local" | "ollama" => ModelChoice::OllamaQwen25,
            "claude-3-5-sonnet" | "claude" | "anthropic" => ModelChoice::Claude35Sonnet,
            "gpt-4o" | "openai" | "gpt" => ModelChoice::Gpt4o,
            other => ModelChoice::Custom(other.to_string()),
        }
    }

    pub fn to_str_id(&self) -> String {
        match self {
            ModelChoice::Gemini20Flash => "gemini-2.0-flash".to_string(),
            ModelChoice::Gemini15Pro => "gemini-1.5-pro".to_string(),
            ModelChoice::OllamaQwen25 => "qwen2.5:1.5b".to_string(),
            ModelChoice::Claude35Sonnet => "claude-3-5-sonnet".to_string(),
            ModelChoice::Gpt4o => "gpt-4o".to_string(),
            ModelChoice::Custom(s) => s.clone(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            ModelChoice::Gemini20Flash => "Google Gemini 2.0 Flash [Default - Fast Cloud]".to_string(),
            ModelChoice::Gemini15Pro => "Google Gemini 1.5 Pro [Long Context Cloud]".to_string(),
            ModelChoice::OllamaQwen25 => "Qwen 2.5 (1.5B) [Local Ollama - Offline]".to_string(),
            ModelChoice::Claude35Sonnet => "Anthropic Claude 3.5 Sonnet [Vision + Computer Use]".to_string(),
            ModelChoice::Gpt4o => "OpenAI GPT-4o [Multimodal Cloud]".to_string(),
            ModelChoice::Custom(s) => format!("Custom Model: {}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub is_local: bool,
    pub supports_vision: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub active_model: ModelChoice,
    pub ollama_url: String,
    pub anthropic_key: Option<String>,
    pub openai_key: Option<String>,
    pub gemini_key: Option<String>,
}

fn load_env_file() {
    let candidates = [
        std::path::PathBuf::from(".env"),
        std::path::PathBuf::from("../.env"),
    ];
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') || !trimmed.contains('=') {
                    continue;
                }
                let mut parts = trimmed.splitn(2, '=');
                if let (Some(key), Some(val)) = (parts.next(), parts.next()) {
                    let k = key.trim();
                    let v = val.trim().trim_matches('"').trim_matches('\'');
                    if std::env::var(k).is_err() {
                        std::env::set_var(k, v);
                    }
                }
            }
            break;
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        load_env_file();
        let gemini_key = env::var("GEMINI_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty());

        Self {
            // DEFAULT: Gemini Flash (Cloud) — user explicitly requested this
            active_model: ModelChoice::Gemini20Flash,
            ollama_url: "http://localhost:11434".to_string(),
            anthropic_key: env::var("ANTHROPIC_API_KEY").ok(),
            openai_key: env::var("OPENAI_API_KEY").ok(),
            gemini_key,
        }
    }
}

pub struct LlmEngine {
    pub config: LlmConfig,
    client: reqwest::Client,
}

impl LlmEngine {
    pub fn new(config: Option<LlmConfig>) -> Self {
        let cfg = config.unwrap_or_default();
        println!("[LLM Engine] Initialized with model: {}", cfg.active_model.display_name());
        println!("[LLM Engine] Gemini API Key present: {}", cfg.gemini_key.is_some());
        Self {
            config: cfg,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// List all supported models including local and cloud
    pub fn list_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                name: "Gemini 2.0 Flash (Default)".to_string(),
                provider: "Google (Cloud)".to_string(),
                is_local: false,
                supports_vision: true,
            },
            ModelInfo {
                id: "gemini-1.5-pro".to_string(),
                name: "Gemini 1.5 Pro".to_string(),
                provider: "Google (Cloud)".to_string(),
                is_local: false,
                supports_vision: true,
            },
            ModelInfo {
                id: "qwen2.5:1.5b".to_string(),
                name: "Qwen 2.5 (1.5B)".to_string(),
                provider: "Ollama (Local)".to_string(),
                is_local: true,
                supports_vision: false,
            },
            ModelInfo {
                id: "claude-3-5-sonnet".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                provider: "Anthropic (Cloud)".to_string(),
                is_local: false,
                supports_vision: true,
            },
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                provider: "OpenAI (Cloud)".to_string(),
                is_local: false,
                supports_vision: true,
            },
        ]
    }

    pub fn set_model(&mut self, model_str: &str) {
        self.config.active_model = ModelChoice::from_str_id(model_str);
        println!("[LLM Engine] Switched active model to: {}", self.config.active_model.display_name());
    }

    /// Unified generation function handling both local Ollama and cloud providers
    pub async fn generate(
        &self,
        prompt: &str,
        system_context: &str,
        vision_image_base64: Option<&str>,
    ) -> Result<String, String> {
        println!("[LLM Engine] Generating with model: {:?}", self.config.active_model.to_str_id());
        
        let result = match &self.config.active_model {
            ModelChoice::Gemini20Flash => {
                self.call_gemini("gemini-2.0-flash", prompt, system_context, vision_image_base64).await
            }
            ModelChoice::Gemini15Pro => {
                self.call_gemini("gemini-1.5-pro", prompt, system_context, vision_image_base64).await
            }
            ModelChoice::OllamaQwen25 => {
                self.call_ollama("qwen2.5:1.5b", prompt, system_context).await
            }
            ModelChoice::Claude35Sonnet => {
                self.call_claude(prompt, system_context, vision_image_base64).await
            }
            ModelChoice::Gpt4o => {
                self.call_openai(prompt, system_context, vision_image_base64).await
            }
            ModelChoice::Custom(name) => {
                // Attempt custom local model on Ollama
                self.call_ollama(name, prompt, system_context).await
            }
        };

        match &result {
            Ok(text) => println!("[LLM Engine] Response received ({} chars)", text.len()),
            Err(e) => println!("[LLM Engine] ERROR: {}", e),
        }

        result
    }

    /// 1. Call Local Ollama (qwen2.5:1.5b)
    async fn call_ollama(
        &self,
        model_name: &str,
        prompt: &str,
        system_context: &str,
    ) -> Result<String, String> {
        let url = format!("{}/api/generate", self.config.ollama_url);
        let full_prompt = if system_context.is_empty() {
            prompt.to_string()
        } else {
            format!("System Context:\n{}\n\nUser: {}\nAssistant:", system_context, prompt)
        };

        let req_body = json!({
            "model": model_name,
            "prompt": full_prompt,
            "stream": false
        });

        let res = self.client.post(&url)
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to local Ollama at {}: {}", self.config.ollama_url, e))?;

        if !res.status().is_success() {
            return Err(format!("Ollama error status: {}", res.status()));
        }

        let body: Value = res.json().await
            .map_err(|e| format!("Invalid JSON from Ollama: {}", e))?;

        let response_text = body["response"].as_str().unwrap_or("").to_string();
        Ok(response_text)
    }

    /// 2. Call Anthropic Claude 3.5 Sonnet (Cloud + Vision)
    async fn call_claude(
        &self,
        prompt: &str,
        system_context: &str,
        vision_image_base64: Option<&str>,
    ) -> Result<String, String> {
        let api_key = match &self.config.anthropic_key {
            Some(key) if !key.is_empty() => key.clone(),
            _ => return Err("Anthropic API key is not configured. Set ANTHROPIC_API_KEY environment variable or select Gemini 2.0 Flash.".to_string()),
        };

        let mut content_blocks = Vec::new();

        // If screen capture is present, attach image block
        if let Some(img_b64) = vision_image_base64 {
            content_blocks.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/jpeg",
                    "data": img_b64
                }
            }));
        }

        content_blocks.push(json!({
            "type": "text",
            "text": prompt
        }));

        let req_body = json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 1024,
            "system": system_context,
            "messages": [
                {
                    "role": "user",
                    "content": content_blocks
                }
            ]
        });

        let res = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        if !res.status().is_success() {
            let err_body = res.text().await.unwrap_or_default();
            return Err(format!("Anthropic API error: {}", err_body));
        }

        let body: Value = res.json().await
            .map_err(|e| format!("Invalid JSON from Anthropic: {}", e))?;

        let reply = body["content"][0]["text"].as_str().unwrap_or("").to_string();
        Ok(reply)
    }

    /// 3. Call OpenAI GPT-4o (Cloud + Vision)
    async fn call_openai(
        &self,
        prompt: &str,
        system_context: &str,
        vision_image_base64: Option<&str>,
    ) -> Result<String, String> {
        let api_key = match &self.config.openai_key {
            Some(key) if !key.is_empty() => key.clone(),
            _ => return Err("OpenAI API key is not configured. Set OPENAI_API_KEY environment variable or select Gemini 2.0 Flash.".to_string()),
        };

        let mut user_content = Vec::new();
        user_content.push(json!({
            "type": "text",
            "text": prompt
        }));

        if let Some(img_b64) = vision_image_base64 {
            user_content.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/jpeg;base64,{}", img_b64)
                }
            }));
        }

        let req_body = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": system_context},
                {"role": "user", "content": user_content}
            ]
        });

        let res = self.client.post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {}", e))?;

        if !res.status().is_success() {
            let err_body = res.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error: {}", err_body));
        }

        let body: Value = res.json().await
            .map_err(|e| format!("Invalid JSON from OpenAI: {}", e))?;

        let reply = body["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
        Ok(reply)
    }

    /// 4. Call Google Gemini (Cloud + Vision) — with automatic model fallback
    async fn call_gemini(
        &self,
        requested_model: &str,
        prompt: &str,
        system_context: &str,
        vision_image_base64: Option<&str>,
    ) -> Result<String, String> {
        // Enforce strictly serialized single-flight dispatches to Google (no simultaneous requests)
        static GOOGLE_DISPATCH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _dispatch_lock = GOOGLE_DISPATCH_LOCK.lock().await;

        let api_key = match &self.config.gemini_key {
            Some(key) if !key.trim().is_empty() => key.clone(),
            _ => env::var("GEMINI_API_KEY").unwrap_or_default(),
        };

        if api_key.trim().is_empty() {
            return Err("GEMINI_API_KEY is not set. Please add it to your .env file.".to_string());
        }

        // Determine candidate models in priority order (flash-lite has high limits and ultra-fast latency)
        let models_to_try: Vec<&str> = match requested_model {
            "gemini-2.0-flash" | "gemini-flash" | "gemini-3.5-flash" => {
                vec!["gemini-3.5-flash-lite", "gemini-3.5-flash", "gemini-3.6-flash"]
            }
            "gemini-1.5-pro" | "gemini-pro" => {
                vec!["gemini-3.5-flash-lite", "gemini-3.5-flash"]
            }
            other => {
                vec![other, "gemini-3.5-flash-lite", "gemini-3.5-flash"]
            }
        };

        let mut parts = Vec::new();
        if let Some(img_b64) = vision_image_base64 {
            parts.push(json!({
                "inline_data": {
                    "mime_type": "image/jpeg",
                    "data": img_b64
                }
            }));
        }

        let combined_prompt = if system_context.is_empty() {
            prompt.to_string()
        } else {
            format!("{}\n\nUser: {}", system_context, prompt)
        };
        parts.push(json!({ "text": combined_prompt }));

        let req_body = json!({
            "contents": [{ "parts": parts }],
            "generationConfig": {
                "temperature": 0.7,
                "maxOutputTokens": 1024,
                "topP": 0.95
            }
        });

        let mut last_error = String::new();

        for model in models_to_try {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model, api_key
            );

            crate::logger::log("GEMINI", "INFO", &format!("Dispatching query to Google Cloud ({})", model));

            let res = match self.client.post(&url)
                .header("Content-Type", "application/json")
                .json(&req_body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_error = format!("Network error connecting to {}: {}", model, e);
                    crate::logger::log("GEMINI", "WARN", &last_error);
                    continue;
                }
            };

            let status = res.status();
            if !status.is_success() {
                let err_body = res.text().await.unwrap_or_default();
                last_error = format!("Gemini ({}) HTTP {}: {}", model, status, err_body);
                crate::logger::log("GEMINI", "WARN", &format!("Model {} returned HTTP {}. Trying next fallback...", model, status));
                continue;
            }

            let body: Value = match res.json().await {
                Ok(b) => b,
                Err(e) => {
                    last_error = format!("JSON parse error from {}: {}", model, e);
                    crate::logger::log("GEMINI", "ERROR", &last_error);
                    continue;
                }
            };

            let mut reply = String::new();
            if let Some(parts_arr) = body["candidates"][0]["content"]["parts"].as_array() {
                for p in parts_arr {
                    if let Some(txt) = p["text"].as_str() {
                        reply.push_str(txt);
                    }
                }
            }

            if reply.trim().is_empty() {
                let block_reason = body["candidates"][0]["finishReason"].as_str().unwrap_or("unknown");
                if block_reason == "SAFETY" {
                    crate::logger::log("GEMINI", "WARN", "Response blocked by Gemini safety filter.");
                    return Err("Gemini blocked this response due to safety filters.".to_string());
                }
                crate::logger::log("GEMINI", "WARN", &format!("Empty text response from {}. Trying next model...", model));
                continue;
            }

            crate::logger::log("GEMINI", "SUCCESS", &format!("Response received from {} ({} chars)", model, reply.len()));
            return Ok(reply);
        }

        crate::logger::log("GEMINI", "ERROR", &format!("All models failed. Last error: {}", last_error));
        Err(format!("All Gemini model endpoints failed. Last error: {}", last_error))
    }
}
