use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>, // For SSE transport
    pub transport: String,   // "stdio" or "sse"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfigFile {
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub requires_permission: bool, // Set true if the tool performs a write or mutation
    pub server_name: String,
}

pub struct McpManager {
    pub servers: HashMap<String, McpServerConfig>,
    pub registered_tools: Vec<McpToolDefinition>,
}

impl McpManager {
    pub fn new() -> Self {
        let mut manager = Self {
            servers: HashMap::new(),
            registered_tools: Vec::new(),
        };

        // Register default built-in tools
        manager.register_builtin_tools();
        manager
    }

    /// Load external MCP servers from mcp_servers.json configuration file
    pub fn load_config_file(&mut self, config_path: &str) -> Result<usize, String> {
        let path = Path::new(config_path);
        if !path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(path)
            .map_err(|e| format!("Could not read MCP config: {}", e))?;

        let parsed: McpConfigFile = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid MCP config JSON: {}", e))?;

        let count = parsed.mcp_servers.len();
        for (name, mut config) in parsed.mcp_servers {
            config.name = name.clone();
            self.servers.insert(name.clone(), config);
            println!("[MCP] Registered external MCP server: {}", name);
        }

        Ok(count)
    }

    fn register_builtin_tools(&mut self) {
        // Builtin Read Tool
        self.registered_tools.push(McpToolDefinition {
            name: "fs_read_file".to_string(),
            description: "Read the entire contents of a file on the local filesystem".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Absolute path to the file"}
                },
                "required": ["path"]
            }),
            requires_permission: false, // Safe read
            server_name: "core_builtin".to_string(),
        });

        // Builtin Write Tool (Guarded)
        self.registered_tools.push(McpToolDefinition {
            name: "fs_write_file".to_string(),
            description: "Write content to a file on the local filesystem (Requires User Authorization)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Absolute path to the file"},
                    "content": {"type": "string", "description": "Text content to write"}
                },
                "required": ["path", "content"]
            }),
            requires_permission: true, // Strict permission required!
            server_name: "core_builtin".to_string(),
        });

        // Builtin Web/Browser Search Tool
        self.registered_tools.push(McpToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web for up-to-date information".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query keywords"}
                },
                "required": ["query"]
            }),
            requires_permission: false,
            server_name: "core_builtin".to_string(),
        });
    }

    /// List all currently available MCP and plugin tools
    pub fn list_tools(&self) -> &[McpToolDefinition] {
        &self.registered_tools
    }

    /// Check if a specific tool requires write permission
    pub fn tool_requires_permission(&self, tool_name: &str) -> bool {
        self.registered_tools
            .iter()
            .find(|t| t.name == tool_name)
            .map(|t| t.requires_permission)
            .unwrap_or(true) // Default to safe/conservative: require permission if unknown
    }
}
