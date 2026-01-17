//! Agentic loop integration tests
//!
//! Tests for tool execution and registry.

use cowork_core::tools::{Tool, ToolRegistry, ToolDefinition, ToolOutput};
use cowork_core::tools::filesystem::{ReadFile, WriteFile, GlobFiles, GrepFiles};
use cowork_core::tools::shell::ExecuteCommand;
use serde_json::json;
use tempfile::TempDir;
use std::fs;
use std::sync::Arc;

/// Create a test workspace with sample files
fn setup_workspace() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let base = dir.path();

    fs::create_dir_all(base.join("src")).unwrap();

    fs::write(
        base.join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
}
"#,
    ).unwrap();

    fs::write(
        base.join("README.md"),
        "# Test Project\n\nThis is a test project.\n",
    ).unwrap();

    dir
}

/// Create a tool registry for testing
fn create_tool_registry(workspace: &std::path::Path) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    registry.register(Arc::new(ReadFile::new(workspace.to_path_buf())));
    registry.register(Arc::new(WriteFile::new(workspace.to_path_buf())));
    registry.register(Arc::new(GlobFiles::new(workspace.to_path_buf())));
    registry.register(Arc::new(GrepFiles::new(workspace.to_path_buf())));
    registry.register(Arc::new(ExecuteCommand::new(workspace.to_path_buf())));

    registry
}

mod tool_registry_tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("glob_files").is_some());
        assert!(registry.get("grep_files").is_some());
        assert!(registry.get("execute_command").is_some());
    }

    #[test]
    fn test_list_tools() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tools = registry.list();
        assert!(tools.len() >= 5, "Should have at least 5 tools");

        for tool_def in tools {
            assert!(!tool_def.name.is_empty());
            assert!(!tool_def.description.is_empty());
        }
    }

    #[test]
    fn test_get_nonexistent_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_all_tools() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let all_tools = registry.all();
        assert!(all_tools.len() >= 5);
    }

    #[tokio::test]
    async fn test_execute_tool_directly() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("read_file").unwrap();
        let result = tool.execute(json!({
            "path": "README.md"
        })).await;

        assert!(result.is_ok(), "Read failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }
}

mod tool_output_tests {
    use super::*;

    #[test]
    fn test_success_output() {
        let output = ToolOutput::success(json!({
            "message": "Operation completed"
        }));

        assert!(output.success);
        assert!(output.error.is_none());
    }

    #[test]
    fn test_error_output() {
        let output = ToolOutput::error("Something went wrong");

        assert!(!output.success);
        assert!(output.error.is_some());
        assert!(output.error.unwrap().contains("wrong"));
    }

    #[test]
    fn test_output_with_metadata() {
        let output = ToolOutput::success(json!({"data": "test"}))
            .with_metadata("key", "value")
            .with_metadata("count", 42);

        assert!(output.success);
        assert!(output.metadata.contains_key("key"));
        assert!(output.metadata.contains_key("count"));
    }
}

mod tool_definition_tests {
    use super::*;

    #[test]
    fn test_tool_definition_structure() {
        let def = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string"
                    }
                }
            }),
        };

        assert_eq!(def.name, "test_tool");
        assert!(!def.description.is_empty());
        assert!(def.parameters.is_object());
    }
}

mod tool_execution_tests {
    use super::*;

    #[tokio::test]
    async fn test_glob_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("glob_files").unwrap();
        let result = tool.execute(json!({
            "pattern": "**/*.rs"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_grep_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("grep_files").unwrap();
        let result = tool.execute(json!({
            "pattern": "fn main"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_execute_command_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("execute_command").unwrap();
        let result = tool.execute(json!({
            "command": "echo 'test'"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_tool_error_handling() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("read_file").unwrap();
        let result = tool.execute(json!({
            "path": "nonexistent_file.txt"
        })).await;

        // Should return an error
        assert!(result.is_err(), "Should fail for nonexistent file");
    }
}

mod approval_level_tests {
    use super::*;
    use cowork_core::approval::ApprovalLevel;

    #[test]
    fn test_read_file_approval() {
        let dir = setup_workspace();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let level = tool.approval_level();
        assert!(
            matches!(level, ApprovalLevel::None | ApprovalLevel::Low),
            "Read should have low approval"
        );
    }

    #[test]
    fn test_write_file_approval() {
        let dir = setup_workspace();
        let tool = WriteFile::new(dir.path().to_path_buf());

        let level = tool.approval_level();
        assert!(
            matches!(level, ApprovalLevel::Medium | ApprovalLevel::High),
            "Write should require approval"
        );
    }
}
