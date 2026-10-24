use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub text: String,
    pub category: String, // e.g. "conversation_context", "preference", "fact", "instruction"
    pub timestamp: i64,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaAddRequest {
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub embeddings: Vec<Vec<f32>>,
    pub documents: Vec<String>,
    pub metadatas: Vec<HashMap<String, String>>,
}

/// Calculate cosine similarity between two float vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// ChromaDB Vector Memory Connector with Google Gemini Embedding & Local Persistence
pub struct ChromaMemoryEngine {
    pub base_url: String,
    pub collection_name: String,
    client: reqwest::Client,
    local_cache: Vec<MemoryItem>,
    storage_dir: PathBuf,
}

impl ChromaMemoryEngine {
    pub fn new(base_url: Option<String>, collection_name: Option<String>) -> Self {
        let storage_dir = PathBuf::from("chroma_db");
        let _ = create_dir_all(&storage_dir);
        let cache_file = storage_dir.join("memories.json");

        // Load existing memories from local persistence if available
        let mut loaded_memories: Vec<MemoryItem> = Vec::new();
        if cache_file.exists() {
            if let Ok(mut file) = File::open(&cache_file) {
                let mut content = String::new();
                if file.read_to_string(&mut content).is_ok() {
                    if let Ok(items) = serde_json::from_str::<Vec<MemoryItem>>(&content) {
                        println!("[ChromaDB] Loaded {} existing memories from disk.", items.len());
                        loaded_memories = items;
                    }
                }
            }
        }

        if loaded_memories.is_empty() {
            loaded_memories = vec![
                MemoryItem {
                    id: "init-1".to_string(),
                    text: "User prefers high-performance Rust core".to_string(),
                    category: "preference".to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    confidence: 0.98,
                    embedding: None,
                },
                MemoryItem {
                    id: "init-2".to_string(),
                    text: "User requested Pure Golden Orb UI with Always-On listening".to_string(),
                    category: "ui".to_string(),
                    timestamp: chrono::Utc::now().timestamp(),
                    confidence: 0.95,
                    embedding: None,
                },
            ];
        }

        Self {
            base_url: base_url.unwrap_or_else(|| "http://localhost:8000/api/v1".to_string()),
            collection_name: collection_name.unwrap_or_else(|| "aura_conversation_memory".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(4))
                .connect_timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            local_cache: loaded_memories,
            storage_dir,
        }
    }

    /// Retrieve the dedicated Embedding API key (or fallback to GEMINI_API_KEY)
    fn get_embedding_key() -> Option<String> {
        std::env::var("GEMINI_EMBEDDING_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
            .or_else(|| {
                std::env::var("GEMINI_API_KEY")
                    .ok()
                    .filter(|k| !k.trim().is_empty())
            })
    }

    /// Generate real vector embeddings using Google Gemini's embedding model
    pub async fn generate_embedding(&self, text: &str) -> Option<Vec<f32>> {
        let api_key = Self::get_embedding_key()?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-001:embedContent?key={}",
            api_key
        );

        let body = json!({
            "content": {
                "parts": [{ "text": text }]
            }
        });

        match self.client.post(&url).json(&body).send().await {
            Ok(res) if res.status().is_success() => {
                if let Ok(json_val) = res.json::<Value>().await {
                    if let Some(arr) = json_val["embedding"]["values"].as_array() {
                        let vec: Vec<f32> = arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                        if !vec.is_empty() {
                            return Some(vec);
                        }
                    }
                }
            }
            Ok(res) => {
                let status = res.status();
                let err = res.text().await.unwrap_or_default();
                crate::logger::log("CHROMA", "WARN", &format!("Embedding generation failed (HTTP {}): {}", status, err));
            }
            Err(e) => {
                crate::logger::log("CHROMA", "WARN", &format!("Embedding network error: {}", e));
            }
        }
        None
    }

    /// Save current in-memory cache to disk
    fn persist_to_disk(&self) {
        let cache_file = self.storage_dir.join("memories.json");
        if let Ok(json_str) = serde_json::to_string_pretty(&self.local_cache) {
            let _ = std::fs::write(&cache_file, json_str);
        }
    }

    /// Automatically extracts key details, habits, preferences, and facts from conversation text
    pub fn extract_facts_from_convo(&self, text: &str) -> Vec<MemoryItem> {
        let mut extracted = Vec::new();
        let lower = text.to_lowercase();
        let now = chrono::Utc::now().timestamp();

        if lower.contains("i like") || lower.contains("i love") || lower.contains("i prefer") || lower.contains("my favorite") {
            extracted.push(MemoryItem {
                id: Uuid::new_v4().to_string(),
                text: format!("User preference: {}", text.trim()),
                category: "preference".to_string(),
                timestamp: now,
                confidence: 0.95,
                embedding: None,
            });
        }

        if lower.contains("i am working on") || lower.contains("my project") || lower.contains("project 7") {
            extracted.push(MemoryItem {
                id: Uuid::new_v4().to_string(),
                text: format!("User project context: {}", text.trim()),
                category: "project".to_string(),
                timestamp: now,
                confidence: 0.90,
                embedding: None,
            });
        }

        if lower.contains("always") || lower.contains("never") || lower.contains("rule") || lower.contains("remember to") {
            extracted.push(MemoryItem {
                id: Uuid::new_v4().to_string(),
                text: format!("User rule/instruction: {}", text.trim()),
                category: "instruction".to_string(),
                timestamp: now,
                confidence: 0.95,
                embedding: None,
            });
        }

        extracted
    }

    /// Store a memory item into ChromaDB and append to local persistent vector store
    pub async fn store_memory(&mut self, mut item: MemoryItem) -> Result<(), String> {
        // Generate Google Gemini embedding if not present
        if item.embedding.is_none() {
            item.embedding = self.generate_embedding(&item.text).await;
        }

        // Cache locally and persist to disk
        self.local_cache.push(item.clone());
        self.persist_to_disk();
        crate::logger::log("MEMORY", "INFO", &format!("Appended context to ChromaDB (Total: {} entries)", self.local_cache.len()));

        // Also post to ChromaDB REST service if running
        let endpoint = format!("{}/collections/{}/add", self.base_url, self.collection_name);
        let mut meta = HashMap::new();
        meta.insert("category".to_string(), item.category);
        meta.insert("timestamp".to_string(), item.timestamp.to_string());

        let embeddings_payload = match &item.embedding {
            Some(vec) => vec![vec.clone()],
            None => Vec::new(),
        };

        let req_body = ChromaAddRequest {
            ids: vec![item.id],
            embeddings: embeddings_payload,
            documents: vec![item.text],
            metadatas: vec![meta],
        };

        let _ = self.client.post(&endpoint).json(&req_body).send().await;
        Ok(())
    }

    /// Query relevant memory for a given prompt using semantic vector search and keyword match
    pub async fn query_relevant_memories(&self, prompt: &str, top_k: usize) -> Vec<MemoryItem> {
        let query_emb = self.generate_embedding(prompt).await;

        if let Some(q_vec) = query_emb {
            // Rank memories by cosine similarity with Gemini embeddings
            let mut scored: Vec<(f32, MemoryItem)> = self.local_cache
                .iter()
                .filter_map(|m| {
                    m.embedding.as_ref().map(|emb| {
                        let sim = cosine_similarity(&q_vec, emb);
                        (sim, m.clone())
                    })
                })
                .collect();

            // Sort descending by similarity
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            let results: Vec<MemoryItem> = scored.into_iter().take(top_k).map(|s| s.1).collect();

            if !results.is_empty() {
                return results;
            }
        }

        // Fallback keyword & recency search
        let mut matches: Vec<MemoryItem> = self.local_cache
            .iter()
            .filter(|m| {
                let prompt_words: Vec<&str> = prompt.split_whitespace().collect();
                prompt_words.iter().any(|w| w.len() > 3 && m.text.to_lowercase().contains(&w.to_lowercase()))
            })
            .cloned()
            .collect();

        if matches.is_empty() {
            matches = self.local_cache.iter().rev().take(top_k).cloned().collect();
        }

        matches.truncate(top_k);
        matches
    }

    pub fn get_all_memories(&self) -> Vec<MemoryItem> {
        self.local_cache.clone()
    }
}
