//! MCP Tool Wrapper
//!
//! Bridges MCP server tools to the Cowork tool system.
//! Each MCP tool is wrapped as a `Tool` implementation so it can be
//! used by the agent like any other tool.

use std::sync::Arc;

use serde_json::Value;

use crate::error::ToolError;
use crate::mcp_manager::{McpServerManager, McpToolInfo};
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

/// Wrapper that exposes an MCP tool as a Cowork Tool
pub struct McpToolWrapper {
    /// The MCP tool info (name, description, schema)
    tool_info: McpToolInfo,
    /// Shared reference to the MCP server manager
    manager: Arc<McpServerManager>,
    /// Prefixed name for the tool (mcp__{server}__{tool})
    prefixed_name: String,
}

impl McpToolWrapper {
    /// Create a new MCP tool wrapper
    pub fn new(tool_info: McpToolInfo, manager: Arc<McpServerManager>) -> Self {
        // Create a prefixed name to avoid collisions with built-in tools
        // Format: mcp__{server}__{tool}
        let prefixed_name = format!("mcp__{}__{}",
            tool_info.server.replace('-', "_"),
            tool_info.name.replace('-', "_")
        );

        Self {
            tool_info,
            manager,
            prefixed_name,
        }
    }

    /// Get the server name this tool belongs to
    pub fn server_name(&self) -> &str {
        &self.tool_info.server
    }

    /// Get the original MCP tool name (without prefix)
    pub fn original_name(&self) -> &str {
        &self.tool_info.name
    }
}

impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.prefixed_name
    }

    fn description(&self) -> &str {
        &self.tool_info.description
    }

    fn parameters_schema(&self) -> Value {
        self.tool_info.input_schema.clone()
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            // Call the MCP server through the manager
            let result = self.manager.call_tool(
                &self.tool_info.server,
                &self.tool_info.name,
                params,
            );

            match result {
                Ok(response) => {
                    // MCP tools return a result with "content" array
                    // Extract text content if available
                    let content = if let Some(content_array) = response.get("content").and_then(|c| c.as_array()) {
                        // Concatenate all text content items
                        let texts: Vec<&str> = content_array
                            .iter()
                            .filter_map(|item| {
                                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    item.get("text").and_then(|t| t.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        if texts.is_empty() {
                            response.clone()
                        } else {
                            Value::String(texts.join("\n"))
                        }
                    } else {
                        response.clone()
                    };

                    // Check if the response indicates an error
                    let is_error = response
                        .get("isError")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if is_error {
                        let error_msg = content.as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "MCP tool execution failed".to_string());
                        Ok(ToolOutput::error(error_msg))
                    } else {
                        Ok(ToolOutput::success(content)
                            .with_metadata("mcp_server", Value::String(self.tool_info.server.clone()))
                            .with_metadata("mcp_tool", Value::String(self.tool_info.name.clone())))
                    }
                }
                Err(e) => {
                    Err(ToolError::ExecutionFailed(format!(
                        "MCP tool '{}' on server '{}' failed: {}",
                        self.tool_info.name,
                        self.tool_info.server,
                        e
                    )))
                }
            }
        })
    }
}

/// Create tool wrappers for all tools from all running MCP servers
pub fn create_mcp_tools(manager: Arc<McpServerManager>) -> Vec<Arc<dyn Tool>> {
    let mcp_tools = manager.get_all_tools();

    mcp_tools
        .into_iter()
        .map(|tool_info| {
            Arc::new(McpToolWrapper::new(tool_info, manager.clone())) as Arc<dyn Tool>
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_wrapper_name_format() {
        let manager = Arc::new(McpServerManager::new());

        let tool_info = McpToolInfo {
            name: "browser_click".to_string(),
            description: "Click an element".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string" }
                }
            }),
            server: "playwright".to_string(),
        };

        let wrapper = McpToolWrapper::new(tool_info, manager);

        assert_eq!(wrapper.name(), "mcp__playwright__browser_click");
        assert_eq!(wrapper.original_name(), "browser_click");
        assert_eq!(wrapper.server_name(), "playwright");
    }

    #[test]
    fn test_mcp_tool_wrapper_handles_dashes() {
        let manager = Arc::new(McpServerManager::new());

        let tool_info = McpToolInfo {
            name: "some-tool-name".to_string(),
            description: "A tool".to_string(),
            input_schema: serde_json::json!({}),
            server: "my-server".to_string(),
        };

        let wrapper = McpToolWrapper::new(tool_info, manager);

        // Dashes should be converted to underscores in the prefixed name
        assert_eq!(wrapper.name(), "mcp__my_server__some_tool_name");
    }

    #[test]
    fn test_create_mcp_tools_empty_when_no_servers() {
        let manager = Arc::new(McpServerManager::new());
        let tools = create_mcp_tools(manager);
        assert!(tools.is_empty());
    }

    #[test]
    fn test_mcp_tool_schema_passthrough() {
        let manager = Arc::new(McpServerManager::new());

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" },
                "timeout": { "type": "number", "description": "Timeout in ms" }
            },
            "required": ["url"]
        });

        let tool_info = McpToolInfo {
            name: "navigate".to_string(),
            description: "Navigate to URL".to_string(),
            input_schema: schema.clone(),
            server: "browser".to_string(),
        };

        let wrapper = McpToolWrapper::new(tool_info, manager);

        assert_eq!(wrapper.parameters_schema(), schema);
    }
}
