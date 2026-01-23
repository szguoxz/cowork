//! MCP Server implementation

use std::collections::HashMap;
use std::sync::Arc;

use crate::protocol::{methods, JsonRpcError, JsonRpcRequest, JsonRpcResponse, RequestId};
use crate::{McpPrompt, McpResource, McpTool, ServerCapabilities, PROTOCOL_VERSION};

/// Handler for MCP requests
#[allow(async_fn_in_trait)]
pub trait McpHandler: Send + Sync {
    /// List available tools
    async fn list_tools(&self) -> Vec<McpTool>;

    /// Call a tool
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String>;

    /// List available resources
    async fn list_resources(&self) -> Vec<McpResource>;

    /// Read a resource
    async fn read_resource(&self, uri: &str) -> Result<ResourceContent, String>;

    /// List available prompts
    async fn list_prompts(&self) -> Vec<McpPrompt>;

    /// Get a prompt
    async fn get_prompt(
        &self,
        name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<PromptContent, String>;
}

/// Resource content
#[derive(Debug, Clone)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<Vec<u8>>,
}

/// Prompt content
#[derive(Debug, Clone)]
pub struct PromptContent {
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

#[derive(Debug, Clone)]
pub struct PromptMessage {
    pub role: String,
    pub content: String,
}

/// MCP Server
pub struct McpServer<H: McpHandler> {
    handler: Arc<H>,
    capabilities: ServerCapabilities,
    server_name: String,
    server_version: String,
}

impl<H: McpHandler> McpServer<H> {
    pub fn new(handler: Arc<H>) -> Self {
        Self {
            handler,
            capabilities: ServerCapabilities {
                tools: Some(crate::ToolsCapability { list_changed: false }),
                resources: Some(crate::ResourcesCapability {
                    subscribe: false,
                    list_changed: false,
                }),
                prompts: Some(crate::PromptsCapability { list_changed: false }),
            },
            server_name: "cowork-mcp".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = name.into();
        self
    }

    /// Handle a JSON-RPC request
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            methods::INITIALIZE => self.handle_initialize(request.id).await,
            methods::TOOLS_LIST => self.handle_tools_list(request.id).await,
            methods::TOOLS_CALL => self.handle_tools_call(request.id, request.params).await,
            methods::RESOURCES_LIST => self.handle_resources_list(request.id).await,
            methods::RESOURCES_READ => self.handle_resources_read(request.id, request.params).await,
            methods::PROMPTS_LIST => self.handle_prompts_list(request.id).await,
            methods::PROMPTS_GET => self.handle_prompts_get(request.id, request.params).await,
            _ => JsonRpcResponse::error(request.id, JsonRpcError::method_not_found()),
        }
    }

    async fn handle_initialize(&self, id: RequestId) -> JsonRpcResponse {
        let result = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": self.capabilities,
            "serverInfo": {
                "name": self.server_name,
                "version": self.server_version
            }
        });

        JsonRpcResponse::success(id, result)
    }

    async fn handle_tools_list(&self, id: RequestId) -> JsonRpcResponse {
        let tools = self.handler.list_tools().await;
        JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
    }

    async fn handle_tools_call(
        &self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => return JsonRpcResponse::error(id, JsonRpcError::invalid_params()),
        };

        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return JsonRpcResponse::error(id, JsonRpcError::invalid_params()),
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default()));

        match self.handler.call_tool(name, arguments).await {
            Ok(result) => {
                let content = vec![serde_json::json!({
                    "type": "text",
                    "text": result.to_string()
                })];
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": content,
                        "isError": false
                    }),
                )
            }
            Err(e) => {
                let content = vec![serde_json::json!({
                    "type": "text",
                    "text": e
                })];
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": content,
                        "isError": true
                    }),
                )
            }
        }
    }

    async fn handle_resources_list(&self, id: RequestId) -> JsonRpcResponse {
        let resources = self.handler.list_resources().await;
        JsonRpcResponse::success(id, serde_json::json!({ "resources": resources }))
    }

    async fn handle_resources_read(
        &self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => return JsonRpcResponse::error(id, JsonRpcError::invalid_params()),
        };

        let uri = match params.get("uri").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return JsonRpcResponse::error(id, JsonRpcError::invalid_params()),
        };

        match self.handler.read_resource(uri).await {
            Ok(content) => {
                let contents = vec![serde_json::json!({
                    "uri": content.uri,
                    "mimeType": content.mime_type,
                    "text": content.text
                })];
                JsonRpcResponse::success(id, serde_json::json!({ "contents": contents }))
            }
            Err(e) => JsonRpcResponse::error(
                id,
                JsonRpcError::new(-32000, format!("Resource error: {}", e)),
            ),
        }
    }

    async fn handle_prompts_list(&self, id: RequestId) -> JsonRpcResponse {
        let prompts = self.handler.list_prompts().await;
        JsonRpcResponse::success(id, serde_json::json!({ "prompts": prompts }))
    }

    async fn handle_prompts_get(
        &self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => return JsonRpcResponse::error(id, JsonRpcError::invalid_params()),
        };

        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return JsonRpcResponse::error(id, JsonRpcError::invalid_params()),
        };

        let arguments: HashMap<String, String> = params
            .get("arguments")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        match self.handler.get_prompt(name, arguments).await {
            Ok(content) => {
                let messages: Vec<_> = content
                    .messages
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "role": m.role,
                            "content": { "type": "text", "text": m.content }
                        })
                    })
                    .collect();

                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "description": content.description,
                        "messages": messages
                    }),
                )
            }
            Err(e) => JsonRpcResponse::error(
                id,
                JsonRpcError::new(-32000, format!("Prompt error: {}", e)),
            ),
        }
    }
}
