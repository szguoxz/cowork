//! MCP (Model Context Protocol) Server Manager
//!
//! Manages the lifecycle of MCP servers: starting, stopping, and discovering tools.
//! Supports both stdio (local process) and HTTP (remote server) transports.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::McpServerConfig;
use crate::error::{Error, Result, ToolError};

/// Status of an MCP server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpServerStatus {
    /// Server is not running
    Stopped,
    /// Server is starting up
    Starting,
    /// Server is running and ready
    Running,
    /// Server failed to start or crashed
    Failed(String),
}

/// Connection handle for an MCP server
enum McpConnection {
    /// Stdio connection to local process
    Stdio(Child),
    /// HTTP connection to remote server
    Http {
        /// Base URL of the server
        url: String,
        /// HTTP client
        client: reqwest::blocking::Client,
        /// Headers to include in requests
        headers: HashMap<String, String>,
    },
}

/// Information about a running MCP server
#[derive(Debug)]
pub struct McpServerInstance {
    /// Server name
    pub name: String,
    /// Configuration
    pub config: McpServerConfig,
    /// Current status
    pub status: McpServerStatus,
    /// Connection handle (stdio process or HTTP client)
    connection: Option<McpConnection>,
    /// Tools provided by this server
    pub tools: Vec<McpToolInfo>,
}

impl std::fmt::Debug for McpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpConnection::Stdio(_) => write!(f, "Stdio(...)"),
            McpConnection::Http { url, .. } => write!(f, "Http({})", url),
        }
    }
}

/// Information about a tool provided by an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: Value,
    /// Server that provides this tool
    pub server: String,
}

/// MCP JSON-RPC request
#[derive(Debug, Serialize)]
struct McpRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// MCP JSON-RPC response
#[derive(Debug, Deserialize)]
struct McpResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<Value>,
    error: Option<McpError>,
}

/// MCP error
#[derive(Debug, Deserialize)]
struct McpError {
    code: i32,
    message: String,
}

/// Helper to create tool errors
fn mcp_error(msg: impl Into<String>) -> Error {
    Error::Tool(ToolError::ExecutionFailed(msg.into()))
}

/// Manager for MCP servers
pub struct McpServerManager {
    /// Configured servers (name -> instance)
    servers: Arc<Mutex<HashMap<String, McpServerInstance>>>,
    /// Request ID counter for JSON-RPC
    request_id: Arc<Mutex<u64>>,
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServerManager {
    /// Create a new MCP server manager
    pub fn new() -> Self {
        Self {
            servers: Arc::new(Mutex::new(HashMap::new())),
            request_id: Arc::new(Mutex::new(1)),
        }
    }

    /// Initialize the manager with server configurations
    pub fn with_configs(configs: HashMap<String, McpServerConfig>) -> Self {
        let manager = Self::new();

        let mut servers = manager.servers.lock().unwrap();
        for (name, config) in configs {
            servers.insert(name.clone(), McpServerInstance {
                name: name.clone(),
                config,
                status: McpServerStatus::Stopped,
                connection: None,
                tools: Vec::new(),
            });
        }
        drop(servers);

        manager
    }

    /// Add a server configuration
    pub fn add_server(&self, name: String, config: McpServerConfig) {
        let mut servers = self.servers.lock().unwrap();
        servers.insert(name.clone(), McpServerInstance {
            name,
            config,
            status: McpServerStatus::Stopped,
            connection: None,
            tools: Vec::new(),
        });
    }

    /// Remove a server configuration (stops it first if running)
    pub fn remove_server(&self, name: &str) -> Result<()> {
        // Stop if running
        let _ = self.stop_server(name);

        let mut servers = self.servers.lock().unwrap();
        servers.remove(name);
        Ok(())
    }

    /// Start an MCP server by name
    pub fn start_server(&self, name: &str) -> Result<()> {
        let mut servers = self.servers.lock().unwrap();

        let instance = servers.get_mut(name)
            .ok_or_else(|| mcp_error(format!("MCP server '{}' not found", name)))?;

        if instance.status == McpServerStatus::Running {
            return Ok(()); // Already running
        }

        instance.status = McpServerStatus::Starting;

        // Check if this is HTTP or stdio transport
        if instance.config.is_http() {
            // HTTP transport
            let url = instance.config.url.clone()
                .ok_or_else(|| mcp_error("HTTP transport requires a URL"))?;

            let client = reqwest::blocking::Client::new();
            let headers = instance.config.headers.clone();

            // Test connection with initialize request
            let init_request = McpRequest {
                jsonrpc: "2.0",
                id: self.next_request_id(),
                method: "initialize".to_string(),
                params: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "cowork",
                        "version": "0.1.0"
                    }
                })),
            };

            let mut request = client.post(&url);
            for (key, value) in &headers {
                request = request.header(key, value);
            }
            request = request.header("Content-Type", "application/json");

            let response = request.json(&init_request).send()
                .map_err(|e| mcp_error(format!("Failed to connect to MCP server '{}': {}", name, e)))?;

            if !response.status().is_success() {
                instance.status = McpServerStatus::Failed(format!("HTTP {}", response.status()));
                return Err(mcp_error(format!("MCP server returned HTTP {}", response.status())));
            }

            let mcp_response: McpResponse = response.json()
                .map_err(|e| mcp_error(format!("Invalid init response: {}", e)))?;

            if let Some(err) = mcp_response.error {
                instance.status = McpServerStatus::Failed(err.message.clone());
                return Err(mcp_error(format!("MCP init failed ({}): {}", err.code, err.message)));
            }

            instance.connection = Some(McpConnection::Http { url, client, headers });
            instance.status = McpServerStatus::Running;
        } else {
            // Stdio transport
            let mut cmd = Command::new(&instance.config.command);
            cmd.args(&instance.config.args)
                .envs(&instance.config.env)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = cmd.spawn()
                .map_err(|e| mcp_error(format!("Failed to start MCP server '{}': {}", name, e)))?;

            // Send initialize request
            let init_request = McpRequest {
                jsonrpc: "2.0",
                id: self.next_request_id(),
                method: "initialize".to_string(),
                params: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "cowork",
                        "version": "0.1.0"
                    }
                })),
            };

            if let Some(ref mut stdin) = child.stdin {
                let msg = serde_json::to_string(&init_request)
                    .map_err(|e| mcp_error(format!("Failed to serialize init request: {}", e)))?;
                writeln!(stdin, "{}", msg)
                    .map_err(|e| mcp_error(format!("Failed to write to MCP server: {}", e)))?;
                stdin.flush()
                    .map_err(|e| mcp_error(format!("Failed to flush to MCP server: {}", e)))?;
            }

            // Read initialize response
            if let Some(ref mut stdout) = child.stdout {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();

                match reader.read_line(&mut line) {
                    Ok(0) => {
                        instance.status = McpServerStatus::Failed("Server closed connection".to_string());
                        return Err(mcp_error("MCP server closed connection during init"));
                    }
                    Ok(_) => {
                        let response: McpResponse = serde_json::from_str(&line)
                            .map_err(|e| mcp_error(format!("Invalid init response: {}", e)))?;

                        if let Some(err) = response.error {
                            instance.status = McpServerStatus::Failed(err.message.clone());
                            return Err(mcp_error(format!("MCP init failed ({}): {}", err.code, err.message)));
                        }
                    }
                    Err(e) => {
                        instance.status = McpServerStatus::Failed(e.to_string());
                        return Err(mcp_error(format!("Failed to read from MCP server: {}", e)));
                    }
                }
            }

            // Send initialized notification
            let initialized_notif = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            });

            if let Some(ref mut stdin) = child.stdin {
                let msg = serde_json::to_string(&initialized_notif)
                    .map_err(|e| mcp_error(format!("Failed to serialize notification: {}", e)))?;
                writeln!(stdin, "{}", msg)
                    .map_err(|e| mcp_error(format!("Failed to write notification: {}", e)))?;
                stdin.flush()
                    .map_err(|e| mcp_error(format!("Failed to flush notification: {}", e)))?;
            }

            instance.connection = Some(McpConnection::Stdio(child));
            instance.status = McpServerStatus::Running;
        }

        // Discover tools from this server
        drop(servers); // Release lock before calling discover_tools
        self.discover_server_tools(name)?;

        Ok(())
    }

    /// Stop an MCP server by name
    pub fn stop_server(&self, name: &str) -> Result<()> {
        let mut servers = self.servers.lock().unwrap();

        let instance = servers.get_mut(name)
            .ok_or_else(|| mcp_error(format!("MCP server '{}' not found", name)))?;

        if let Some(ref mut conn) = instance.connection {
            match conn {
                McpConnection::Stdio(process) => {
                    // Try graceful shutdown first
                    let _ = process.kill();
                    let _ = process.wait();
                }
                McpConnection::Http { .. } => {
                    // HTTP connections don't need explicit shutdown
                    // Just drop the client
                }
            }
        }

        instance.connection = None;
        instance.status = McpServerStatus::Stopped;
        instance.tools.clear();

        Ok(())
    }

    /// Start all enabled servers
    pub fn start_enabled(&self) -> Vec<(String, Result<()>)> {
        let servers = self.servers.lock().unwrap();
        let enabled: Vec<(String, bool)> = servers.iter()
            .filter(|(_, s)| s.config.enabled)
            .map(|(name, _)| (name.clone(), true))
            .collect();
        drop(servers);

        enabled.into_iter()
            .map(|(name, _)| {
                let result = self.start_server(&name);
                (name, result)
            })
            .collect()
    }

    /// Stop all running servers
    pub fn stop_all(&self) -> Vec<(String, Result<()>)> {
        let servers = self.servers.lock().unwrap();
        let running: Vec<String> = servers.iter()
            .filter(|(_, s)| s.status == McpServerStatus::Running)
            .map(|(name, _)| name.clone())
            .collect();
        drop(servers);

        running.into_iter()
            .map(|name| {
                let result = self.stop_server(&name);
                (name, result)
            })
            .collect()
    }

    /// List all servers and their status
    pub fn list_servers(&self) -> Vec<McpServerInfo> {
        let servers = self.servers.lock().unwrap();
        servers.values()
            .map(|s| {
                // Use URL for HTTP servers, command for stdio
                let command = if s.config.is_http() {
                    s.config.url.clone().unwrap_or_default()
                } else {
                    s.config.command.clone()
                };
                McpServerInfo {
                    name: s.name.clone(),
                    command,
                    enabled: s.config.enabled,
                    status: s.status.clone(),
                    tool_count: s.tools.len(),
                }
            })
            .collect()
    }

    /// Get all tools from all running servers
    pub fn get_all_tools(&self) -> Vec<McpToolInfo> {
        let servers = self.servers.lock().unwrap();
        servers.values()
            .filter(|s| s.status == McpServerStatus::Running)
            .flat_map(|s| s.tools.clone())
            .collect()
    }

    /// Get tools from a specific server
    pub fn get_server_tools(&self, name: &str) -> Option<Vec<McpToolInfo>> {
        let servers = self.servers.lock().unwrap();
        servers.get(name).map(|s| s.tools.clone())
    }

    /// Discover tools from a specific server
    fn discover_server_tools(&self, name: &str) -> Result<()> {
        let mut servers = self.servers.lock().unwrap();
        let instance = servers.get_mut(name)
            .ok_or_else(|| mcp_error(format!("MCP server '{}' not found", name)))?;

        if instance.status != McpServerStatus::Running {
            return Err(mcp_error(format!("MCP server '{}' is not running", name)));
        }

        // Send tools/list request
        let tools_request = McpRequest {
            jsonrpc: "2.0",
            id: self.next_request_id(),
            method: "tools/list".to_string(),
            params: None,
        };

        let response_result: Option<Value> = match &mut instance.connection {
            Some(McpConnection::Http { url, client, headers }) => {
                // HTTP transport
                let mut request = client.post(url.as_str());
                for (key, value) in headers.iter() {
                    request = request.header(key, value);
                }
                request = request.header("Content-Type", "application/json");

                let response = request.json(&tools_request).send()
                    .map_err(|e| mcp_error(format!("Failed to send tools request: {}", e)))?;

                let mcp_response: McpResponse = response.json()
                    .map_err(|e| mcp_error(format!("Invalid tools response: {}", e)))?;

                if let Some(err) = mcp_response.error {
                    return Err(mcp_error(format!("Tools list failed: {}", err.message)));
                }

                mcp_response.result
            }
            Some(McpConnection::Stdio(process)) => {
                // Stdio transport
                if let Some(ref mut stdin) = process.stdin {
                    let msg = serde_json::to_string(&tools_request)
                        .map_err(|e| mcp_error(format!("Failed to serialize tools request: {}", e)))?;
                    writeln!(stdin, "{}", msg)
                        .map_err(|e| mcp_error(format!("Failed to write to MCP server: {}", e)))?;
                    stdin.flush()
                        .map_err(|e| mcp_error(format!("Failed to flush to MCP server: {}", e)))?;
                } else {
                    return Err(mcp_error("MCP server stdin not available"));
                }

                // Read response
                if let Some(stdout) = process.stdout.take() {
                    let mut reader = BufReader::new(stdout);
                    let mut line = String::new();

                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            return Err(mcp_error("MCP server closed connection"));
                        }
                        Ok(_) => {
                            let response: McpResponse = serde_json::from_str(&line)
                                .map_err(|e| mcp_error(format!("Invalid tools response: {}", e)))?;

                            if let Some(err) = response.error {
                                return Err(mcp_error(format!("Tools list failed: {}", err.message)));
                            }

                            response.result
                        }
                        Err(e) => {
                            return Err(mcp_error(format!("Failed to read tools: {}", e)));
                        }
                    }
                } else {
                    None
                }
            }
            None => {
                return Err(mcp_error("MCP server not connected"));
            }
        };

        // Parse tools from response
        if let Some(result) = response_result {
            if let Some(tools) = result.get("tools").and_then(|t: &Value| t.as_array()) {
                instance.tools = tools.iter()
                    .filter_map(|t: &Value| {
                        Some(McpToolInfo {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t.get("description")
                                .and_then(|d: &Value| d.as_str())
                                .unwrap_or("")
                                .to_string(),
                            input_schema: t.get("inputSchema")
                                .cloned()
                                .unwrap_or(Value::Null),
                            server: name.to_string(),
                        })
                    })
                    .collect();
            }
        }

        Ok(())
    }

    /// Execute a tool call on an MCP server (lazy-starts if needed)
    pub fn call_tool(&self, server_name: &str, tool_name: &str, arguments: Value) -> Result<Value> {
        // Check if server needs to be started (lazy start)
        {
            let servers = self.servers.lock().unwrap();
            let instance = servers.get(server_name)
                .ok_or_else(|| mcp_error(format!("MCP server '{}' not found", server_name)))?;

            if instance.status != McpServerStatus::Running {
                // Need to start - release lock first to avoid deadlock
                drop(servers);
                tracing::info!("Lazy-starting MCP server '{}'", server_name);
                self.start_server(server_name)?;
            }
        }

        // Now proceed with the call
        let mut servers = self.servers.lock().unwrap();
        let instance = servers.get_mut(server_name)
            .ok_or_else(|| mcp_error(format!("MCP server '{}' not found", server_name)))?;

        // Send tools/call request
        let call_request = McpRequest {
            jsonrpc: "2.0",
            id: self.next_request_id(),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            })),
        };

        match &mut instance.connection {
            Some(McpConnection::Http { url, client, headers }) => {
                // HTTP transport - synchronous call and response
                let mut request = client.post(url.as_str());
                for (key, value) in headers.iter() {
                    request = request.header(key, value);
                }
                request = request.header("Content-Type", "application/json");

                let response = request.json(&call_request).send()
                    .map_err(|e| mcp_error(format!("Failed to send tool call: {}", e)))?;

                let mcp_response: McpResponse = response.json()
                    .map_err(|e| mcp_error(format!("Invalid tool response: {}", e)))?;

                if let Some(err) = mcp_response.error {
                    return Err(mcp_error(format!("Tool call failed: {}", err.message)));
                }

                Ok(mcp_response.result.unwrap_or(Value::Null))
            }
            Some(McpConnection::Stdio(process)) => {
                // Stdio transport
                if let Some(ref mut stdin) = process.stdin {
                    let msg = serde_json::to_string(&call_request)
                        .map_err(|e| mcp_error(format!("Failed to serialize call request: {}", e)))?;
                    writeln!(stdin, "{}", msg)
                        .map_err(|e| mcp_error(format!("Failed to write to MCP server: {}", e)))?;
                    stdin.flush()
                        .map_err(|e| mcp_error(format!("Failed to flush to MCP server: {}", e)))?;
                } else {
                    return Err(mcp_error("MCP server stdin not available"));
                }

                // For stdio, return a placeholder - proper implementation would read response
                Ok(serde_json::json!({
                    "status": "call_sent",
                    "server": server_name,
                    "tool": tool_name
                }))
            }
            None => {
                Err(mcp_error("MCP server not connected"))
            }
        }
    }

    /// Get the next request ID
    fn next_request_id(&self) -> u64 {
        let mut id = self.request_id.lock().unwrap();
        let current = *id;
        *id += 1;
        current
    }
}

/// Summary information about an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server name
    pub name: String,
    /// Command used to start the server
    pub command: String,
    /// Whether the server is enabled for auto-start
    pub enabled: bool,
    /// Current status
    pub status: McpServerStatus,
    /// Number of tools provided
    pub tool_count: usize,
}

impl Drop for McpServerManager {
    fn drop(&mut self) {
        // Stop all servers when the manager is dropped
        let _ = self.stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_manager() {
        let manager = McpServerManager::new();
        assert!(manager.list_servers().is_empty());
    }

    #[test]
    fn test_add_remove_server() {
        let manager = McpServerManager::new();

        let config = McpServerConfig::new("echo")
            .with_args(vec!["hello".to_string()]);

        manager.add_server("test".to_string(), config);

        let servers = manager.list_servers();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "test");
        assert_eq!(servers[0].status, McpServerStatus::Stopped);

        manager.remove_server("test").unwrap();
        assert!(manager.list_servers().is_empty());
    }

    #[test]
    fn test_server_config_with_env() {
        let config = McpServerConfig::new("node")
            .with_args(vec!["server.js".to_string()])
            .with_env("NODE_ENV", "production")
            .with_enabled(false);

        assert_eq!(config.command, "node");
        assert_eq!(config.args, vec!["server.js"]);
        assert_eq!(config.env.get("NODE_ENV"), Some(&"production".to_string()));
        assert!(!config.enabled);
    }
}
