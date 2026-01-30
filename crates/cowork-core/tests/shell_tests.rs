//! Shell tool tests
//!
//! Tests for ExecuteCommand and KillShell tools.

use cowork_core::tools::{Tool, ToolExecutionContext};
use cowork_core::tools::shell::{ExecuteCommand, KillShell, ShellProcessRegistry, ShellConfig, BackgroundShell, ShellStatus};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Create a test context that auto-approves Bash commands
fn test_ctx() -> ToolExecutionContext {
    ToolExecutionContext::test_auto_approve("test", "test")
}

/// Create a test workspace
fn setup_workspace() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");
    std::fs::write(dir.path().join("test.txt"), "Hello, World!").unwrap();
    dir
}

mod execute_command_tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_echo() {
        let dir = setup_workspace();
        let tool = ExecuteCommand::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "command": "echo 'Hello, Test!'"
        }), test_ctx()).await;

        assert!(result.is_ok(), "Echo command failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_pwd_command() {
        let dir = setup_workspace();
        let tool = ExecuteCommand::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "command": "pwd"
        }), test_ctx()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_ls_command() {
        let dir = setup_workspace();
        let tool = ExecuteCommand::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "command": "ls -la"
        }), test_ctx()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_cat_command() {
        let dir = setup_workspace();
        let tool = ExecuteCommand::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "command": "cat test.txt"
        }), test_ctx()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_piped_commands() {
        let dir = setup_workspace();
        let tool = ExecuteCommand::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "command": "echo 'line1\nline2\nline3' | wc -l"
        }), test_ctx()).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_failed_command() {
        let dir = setup_workspace();
        let tool = ExecuteCommand::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "command": "cat nonexistent_file.txt"
        }), test_ctx()).await;

        assert!(result.is_ok(), "Should return result even for failed command");
        let output = result.unwrap();
        // Command failed but tool execution succeeded
        assert!(!output.success || output.content.to_string().contains("No such file"));
    }

    #[tokio::test]
    async fn test_blocked_command() {
        let dir = setup_workspace();
        let config = ShellConfig::default(); // Has "sudo" blocked by default
        let tool = ExecuteCommand::new(dir.path().to_path_buf()).with_config(config);

        let result = tool.execute(json!({
            "command": "sudo ls"
        }), test_ctx()).await;

        assert!(result.is_err(), "sudo should be blocked");
    }
}

mod background_execution_tests {
    use super::*;

    #[tokio::test]
    async fn test_background_execution() {
        let dir = setup_workspace();
        let registry = Arc::new(ShellProcessRegistry::new());
        let tool = ExecuteCommand::new(dir.path().to_path_buf()).with_registry(registry.clone());

        let result = tool.execute(json!({
            "command": "sleep 1 && echo 'done'",
            "run_in_background": true
        }), test_ctx()).await;

        assert!(result.is_ok(), "Background execution failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_kill_background_shell() {
        let dir = setup_workspace();
        let registry = Arc::new(ShellProcessRegistry::new());
        let exec_tool = ExecuteCommand::new(dir.path().to_path_buf()).with_registry(registry.clone());
        let kill_tool = KillShell::new(registry.clone());

        // Start a long-running background process
        let result = exec_tool.execute(json!({
            "command": "sleep 60",
            "run_in_background": true
        }), test_ctx()).await;

        assert!(result.is_ok());
        let output = result.unwrap();

        // Extract shell_id from content
        if let Some(shell_id) = output.content.get("shell_id").and_then(|v| v.as_str()) {
            // Kill it
            let kill_result = kill_tool.execute(json!({
                "shell_id": shell_id
            }), test_ctx()).await;

            assert!(kill_result.is_ok(), "Kill failed: {:?}", kill_result.err());
        }
    }
}

mod shell_config_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_default_config() {
        let config = ShellConfig::default();

        assert!(config.blocked_commands.contains("sudo"));
        assert!(config.blocked_commands.contains("rm -rf /"));
        assert_eq!(config.timeout_seconds, 30);
    }

    #[test]
    fn test_custom_config() {
        let mut blocked = HashSet::new();
        blocked.insert("dangerous_command".to_string());

        let config = ShellConfig {
            allowed_commands: HashSet::new(),
            blocked_commands: blocked,
            timeout_seconds: 60,
            working_dir: Some(std::path::PathBuf::from("/tmp")),
        };

        assert!(config.blocked_commands.contains("dangerous_command"));
        assert_eq!(config.timeout_seconds, 60);
    }
}

mod process_registry_tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let registry = ShellProcessRegistry::new();

        let shell = BackgroundShell {
            id: "test-123".to_string(),
            command: "echo test".to_string(),
            child: None,
            started_at: chrono::Utc::now(),
            status: ShellStatus::Running,
            output: None,
        };

        registry.register(shell).await;

        let status = registry.get("test-123").await;
        assert!(status.is_some());
        assert_eq!(status.unwrap(), ShellStatus::Running);
    }

    #[tokio::test]
    async fn test_registry_list_running() {
        let registry = ShellProcessRegistry::new();

        // Register multiple shells
        for i in 0..3 {
            let shell = BackgroundShell {
                id: format!("shell-{}", i),
                command: format!("command-{}", i),
                child: None,
                started_at: chrono::Utc::now(),
                status: ShellStatus::Running,
                output: None,
            };
            registry.register(shell).await;
        }

        let running = registry.list_running().await;
        assert_eq!(running.len(), 3);
    }

    #[tokio::test]
    async fn test_registry_nonexistent() {
        let registry = ShellProcessRegistry::new();

        let status = registry.get("nonexistent").await;
        assert!(status.is_none());
    }
}
