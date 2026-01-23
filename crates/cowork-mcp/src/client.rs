//! MCP Client implementation

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::protocol::{methods, JsonRpcRequest, JsonRpcResponse, RequestId};
use crate::transport::Transport;
use crate::{McpResource, McpTool, ServerCapabilities, PROTOCOL_VERSION};

/// MCP Client for connecting to MCP servers
pub struct McpClient<T: Transport> {
    transport: Arc<Mutex<T>>,
    request_id: AtomicI64,
    server_capabilities: Option<ServerCapabilities>,
}

impl<T: Transport> McpClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
            request_id: AtomicI64::new(1),
            server_capabilities: None,
        }
    }

    fn next_id(&self) -> RequestId {
        RequestId::Number(self.request_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Initialize the connection
    pub async fn initialize(&mut self, client_info: ClientInfo) -> Result<ServerInfo, McpError> {
        let params = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": client_info.name,
                "version": client_info.version
            }
        });

        let request = JsonRpcRequest::new(self.next_id(), methods::INITIALIZE)
            .with_params(params);

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            let server_info: InitializeResult = serde_json::from_value(result)
                .map_err(|e| McpError::Protocol(e.to_string()))?;

            self.server_capabilities = Some(server_info.capabilities.clone());

            // Send initialized notification
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": methods::INITIALIZED
            });

            let mut transport = self.transport.lock().await;
            transport.send(notification).await
                .map_err(|e| McpError::Transport(e.to_string()))?;

            Ok(ServerInfo {
                name: server_info.server_info.name,
                version: server_info.server_info.version,
            })
        } else if let Some(error) = response.error {
            Err(McpError::Server(error.message))
        } else {
            Err(McpError::Protocol("Empty response".to_string()))
        }
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Vec<McpTool>, McpError> {
        let request = JsonRpcRequest::new(self.next_id(), methods::TOOLS_LIST);
        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            let tools_result: ToolsListResult = serde_json::from_value(result)
                .map_err(|e| McpError::Protocol(e.to_string()))?;
            Ok(tools_result.tools)
        } else {
            Ok(Vec::new())
        }
    }

    /// Call a tool
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolCallResult, McpError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let request = JsonRpcRequest::new(self.next_id(), methods::TOOLS_CALL)
            .with_params(params);

        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            serde_json::from_value(result)
                .map_err(|e| McpError::Protocol(e.to_string()))
        } else if let Some(error) = response.error {
            Err(McpError::Server(error.message))
        } else {
            Err(McpError::Protocol("Empty response".to_string()))
        }
    }

    /// List resources
    pub async fn list_resources(&self) -> Result<Vec<McpResource>, McpError> {
        let request = JsonRpcRequest::new(self.next_id(), methods::RESOURCES_LIST);
        let response = self.send_request(request).await?;

        if let Some(result) = response.result {
            let resources_result: ResourcesListResult = serde_json::from_value(result)
                .map_err(|e| McpError::Protocol(e.to_string()))?;
            Ok(resources_result.resources)
        } else {
            Ok(Vec::new())
        }
    }

    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let mut transport = self.transport.lock().await;

        let request_value = serde_json::to_value(&request)
            .map_err(|e| McpError::Protocol(e.to_string()))?;

        transport.send(request_value).await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        let response_value = transport.receive().await
            .map_err(|e| McpError::Transport(e.to_string()))?
            .ok_or_else(|| McpError::Transport("Connection closed".to_string()))?;

        serde_json::from_value(response_value)
            .map_err(|e| McpError::Protocol(e.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, serde::Deserialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    #[allow(dead_code)]
    protocol_version: String,
    capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    server_info: ServerInfoInner,
}

#[derive(Debug, serde::Deserialize)]
struct ServerInfoInner {
    name: String,
    version: String,
}

#[derive(Debug, serde::Deserialize)]
struct ToolsListResult {
    tools: Vec<McpTool>,
}

#[derive(Debug, serde::Deserialize)]
struct ResourcesListResult {
    resources: Vec<McpResource>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<ContentItem>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
}

/// MCP errors
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Server error: {0}")]
    Server(String),
}
