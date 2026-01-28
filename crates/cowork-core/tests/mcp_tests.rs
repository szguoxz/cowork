//! MCP Tool Integration Tests
//!
//! Tests for the MCP (Model Context Protocol) tool wrapper and integration
//! with the tool registry system.

use std::sync::Arc;

use cowork_core::mcp_manager::{McpServerManager, McpToolInfo};
use cowork_core::orchestration::ToolRegistryBuilder;
use cowork_core::tools::mcp::{create_mcp_tools, McpToolWrapper};
use cowork_core::tools::Tool;

use tempfile::tempdir;

#[test]
fn test_mcp_tool_wrapper_name_format() {
    let manager = Arc::new(McpServerManager::new());

    let tool_info = McpToolInfo {
        name: "browser_navigate".to_string(),
        description: "Navigate to a URL".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" }
            },
            "required": ["url"]
        }),
        server: "playwright".to_string(),
    };

    let wrapper = McpToolWrapper::new(tool_info, manager);

    // Verify the prefixed name format
    assert_eq!(wrapper.name(), "mcp__playwright__browser_navigate");
    assert_eq!(wrapper.original_name(), "browser_navigate");
    assert_eq!(wrapper.server_name(), "playwright");
}

#[test]
fn test_mcp_tool_wrapper_handles_dashes_in_names() {
    let manager = Arc::new(McpServerManager::new());

    let tool_info = McpToolInfo {
        name: "file-upload".to_string(),
        description: "Upload a file".to_string(),
        input_schema: serde_json::json!({}),
        server: "my-custom-server".to_string(),
    };

    let wrapper = McpToolWrapper::new(tool_info, manager);

    // Dashes should be converted to underscores in the prefixed name
    assert_eq!(wrapper.name(), "mcp__my_custom_server__file_upload");
}

#[test]
fn test_mcp_tool_wrapper_preserves_schema() {
    let manager = Arc::new(McpServerManager::new());

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "selector": { "type": "string", "description": "CSS selector" },
            "timeout": { "type": "number", "description": "Timeout in milliseconds" },
            "force": { "type": "boolean", "description": "Force click" }
        },
        "required": ["selector"]
    });

    let tool_info = McpToolInfo {
        name: "click".to_string(),
        description: "Click an element".to_string(),
        input_schema: schema.clone(),
        server: "browser".to_string(),
    };

    let wrapper = McpToolWrapper::new(tool_info, manager);

    // Schema should be passed through unchanged
    assert_eq!(wrapper.parameters_schema(), schema);
}

#[test]
fn test_mcp_tool_wrapper_description() {
    let manager = Arc::new(McpServerManager::new());

    let description = "Take a screenshot of the current page or a specific element";

    let tool_info = McpToolInfo {
        name: "screenshot".to_string(),
        description: description.to_string(),
        input_schema: serde_json::json!({}),
        server: "playwright".to_string(),
    };

    let wrapper = McpToolWrapper::new(tool_info, manager);

    assert_eq!(wrapper.description(), description);
}

#[test]
fn test_create_mcp_tools_empty_when_no_servers() {
    let manager = Arc::new(McpServerManager::new());
    let tools = create_mcp_tools(manager);

    // No servers = no tools
    assert!(tools.is_empty());
}

#[test]
fn test_tool_registry_builder_with_empty_mcp_manager() {
    let temp_dir = tempdir().unwrap();
    let manager = Arc::new(McpServerManager::new());

    let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf())
        .with_mcp_manager(manager)
        .build();

    // Should have built-in tools but no MCP tools
    assert!(registry.get("Read").is_some());
    assert!(registry.get("Bash").is_some());

    // No MCP tools registered (no servers running)
    let all_tools = registry.list();
    let mcp_tools: Vec<_> = all_tools.iter().filter(|t| t.name.starts_with("mcp__")).collect();
    assert!(mcp_tools.is_empty());
}

#[test]
fn test_tool_registry_builder_mcp_disabled() {
    let temp_dir = tempdir().unwrap();
    let manager = Arc::new(McpServerManager::new());

    let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf())
        .with_mcp_manager(manager)
        .with_mcp(false) // Explicitly disabled
        .build();

    // Should have built-in tools
    assert!(registry.get("Read").is_some());

    // No MCP tools even if manager was provided
    let all_tools = registry.list();
    let mcp_tools: Vec<_> = all_tools.iter().filter(|t| t.name.starts_with("mcp__")).collect();
    assert!(mcp_tools.is_empty());
}

#[test]
fn test_mcp_tool_approval_level() {
    use cowork_core::approval::ApprovalLevel;

    let manager = Arc::new(McpServerManager::new());

    let tool_info = McpToolInfo {
        name: "execute_script".to_string(),
        description: "Execute arbitrary JavaScript".to_string(),
        input_schema: serde_json::json!({}),
        server: "browser".to_string(),
    };

    let wrapper = McpToolWrapper::new(tool_info, manager);

    // MCP tools should require approval (Low level by default)
    assert_eq!(wrapper.approval_level(), ApprovalLevel::Low);
}

#[test]
fn test_mcp_tool_definition_conversion() {
    let manager = Arc::new(McpServerManager::new());

    let tool_info = McpToolInfo {
        name: "type".to_string(),
        description: "Type text into an element".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string" },
                "text": { "type": "string" }
            }
        }),
        server: "playwright".to_string(),
    };

    let wrapper = McpToolWrapper::new(tool_info, manager);
    let definition = wrapper.to_definition();

    assert_eq!(definition.name, "mcp__playwright__type");
    assert_eq!(definition.description, Some("Type text into an element".to_string()));
    assert!(definition.schema.as_ref().unwrap().get("properties").is_some());
}

// Test that simulates what happens when MCP tools are discovered
#[test]
fn test_mcp_tools_from_simulated_discovery() {
    let manager = Arc::new(McpServerManager::new());

    // Simulate the tool info that would come from a real MCP server
    let playwright_tools = vec![
        McpToolInfo {
            name: "browser_navigate".to_string(),
            description: "Navigate to a URL".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "required": ["url"]
            }),
            server: "playwright".to_string(),
        },
        McpToolInfo {
            name: "browser_click".to_string(),
            description: "Click an element".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "string" }
                },
                "required": ["ref"]
            }),
            server: "playwright".to_string(),
        },
        McpToolInfo {
            name: "browser_snapshot".to_string(),
            description: "Get accessibility snapshot".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            server: "playwright".to_string(),
        },
    ];

    // Create wrappers manually (simulating what create_mcp_tools does)
    let wrapped_tools: Vec<McpToolWrapper> = playwright_tools
        .into_iter()
        .map(|info| McpToolWrapper::new(info, manager.clone()))
        .collect();

    // Verify all tools have correct names
    let tool_names: Vec<&str> = wrapped_tools.iter().map(|t| t.name()).collect();
    assert!(tool_names.contains(&"mcp__playwright__browser_navigate"));
    assert!(tool_names.contains(&"mcp__playwright__browser_click"));
    assert!(tool_names.contains(&"mcp__playwright__browser_snapshot"));

    // All should have the same server
    for tool in &wrapped_tools {
        assert_eq!(tool.server_name(), "playwright");
    }
}

// Test multiple MCP servers
#[test]
fn test_mcp_tools_multiple_servers() {
    let manager = Arc::new(McpServerManager::new());

    let tools_from_different_servers = vec![
        McpToolInfo {
            name: "navigate".to_string(),
            description: "Navigate browser".to_string(),
            input_schema: serde_json::json!({}),
            server: "playwright".to_string(),
        },
        McpToolInfo {
            name: "query".to_string(),
            description: "Query database".to_string(),
            input_schema: serde_json::json!({}),
            server: "postgres".to_string(),
        },
        McpToolInfo {
            name: "create_issue".to_string(),
            description: "Create GitHub issue".to_string(),
            input_schema: serde_json::json!({}),
            server: "github".to_string(),
        },
    ];

    let wrapped_tools: Vec<McpToolWrapper> = tools_from_different_servers
        .into_iter()
        .map(|info| McpToolWrapper::new(info, manager.clone()))
        .collect();

    // Verify each tool has the correct server prefix
    assert_eq!(wrapped_tools[0].name(), "mcp__playwright__navigate");
    assert_eq!(wrapped_tools[1].name(), "mcp__postgres__query");
    assert_eq!(wrapped_tools[2].name(), "mcp__github__create_issue");
}
