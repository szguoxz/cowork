//! Hook Executor for running hooks at lifecycle points
//!
//! This module provides:
//! - Execution of shell command hooks
//! - Execution of prompt injection hooks
//! - Timeout handling and error recovery
//! - Environment variable injection for hooks

use super::hooks::{HookDefinition, HookEvent, HookHandler, HookMatcher, HookResult, HooksConfig};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;

/// Default timeout for hook commands (30 seconds)
pub const DEFAULT_HOOK_TIMEOUT_MS: u64 = 30_000;

/// Maximum output size from hook commands
pub const MAX_HOOK_OUTPUT_SIZE: usize = 100_000;

/// Error type for hook execution
#[derive(Debug, Error)]
pub enum HookError {
    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Hook timeout after {0}ms")]
    Timeout(u64),

    #[error("Invalid hook output: {0}")]
    InvalidOutput(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("MCP tool not available: {server}/{tool}")]
    McpNotAvailable { server: String, tool: String },
}

/// Context passed to hooks during execution
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    /// Tool name (for PreToolUse/PostToolUse)
    pub tool_name: Option<String>,
    /// Tool arguments (for PreToolUse/PostToolUse)
    pub tool_args: Option<Value>,
    /// Tool result (for PostToolUse)
    pub tool_result: Option<String>,
    /// User prompt (for UserPromptSubmit)
    pub user_prompt: Option<String>,
    /// Session ID
    pub session_id: String,
}

impl HookContext {
    /// Create context for SessionStart
    pub fn session_start(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            ..Default::default()
        }
    }

    /// Create context for UserPromptSubmit
    pub fn user_prompt(session_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            user_prompt: Some(prompt.into()),
            ..Default::default()
        }
    }

    /// Create context for PreToolUse
    pub fn pre_tool_use(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        tool_args: Value,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: Some(tool_name.into()),
            tool_args: Some(tool_args),
            ..Default::default()
        }
    }

    /// Create context for PostToolUse
    pub fn post_tool_use(
        session_id: impl Into<String>,
        tool_name: impl Into<String>,
        tool_args: Value,
        tool_result: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            tool_name: Some(tool_name.into()),
            tool_args: Some(tool_args),
            tool_result: Some(tool_result.into()),
            ..Default::default()
        }
    }
}

/// Executor for running hooks
pub struct HookExecutor {
    /// Working directory for commands
    workspace: PathBuf,
    /// Plugin root for variable expansion
    plugin_root: Option<PathBuf>,
    /// Default timeout for commands
    default_timeout: Duration,
}

impl HookExecutor {
    /// Create a new HookExecutor
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            plugin_root: None,
            default_timeout: Duration::from_millis(DEFAULT_HOOK_TIMEOUT_MS),
        }
    }

    /// Set the plugin root for variable expansion
    pub fn with_plugin_root(mut self, root: PathBuf) -> Self {
        self.plugin_root = Some(root);
        self
    }

    /// Set default timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Execute all hooks for an event
    pub fn execute(
        &self,
        event: HookEvent,
        config: &HooksConfig,
        context: &HookContext,
    ) -> Vec<Result<HookResult, HookError>> {
        let registrations = config.get_hooks(event);
        let mut results = Vec::new();

        for registration in registrations {
            // Check matcher for tool-related events
            if let Some(matcher_str) = &registration.matcher
                && !self.matches_context(matcher_str, context)
            {
                continue;
            }

            // Execute each hook in the registration
            for hook_def in &registration.hooks {
                let result = self.execute_hook(event, hook_def, context);
                results.push(result);
            }
        }

        results
    }

    /// Execute a single hook definition
    fn execute_hook(
        &self,
        event: HookEvent,
        hook: &HookDefinition,
        context: &HookContext,
    ) -> Result<HookResult, HookError> {
        match &hook.handler {
            HookHandler::Command { command, timeout_ms } => {
                self.execute_command_hook(event, command, *timeout_ms, context)
            }
            HookHandler::Prompt { content } => {
                Ok(HookResult::with_context(event, content.clone()))
            }
            HookHandler::McpTool { server, tool, args: _ } => {
                // MCP tools require async runtime integration
                // For now, return an error indicating MCP is not available in sync context
                Err(HookError::McpNotAvailable {
                    server: server.clone(),
                    tool: tool.clone(),
                })
            }
        }
    }

    /// Execute a command hook
    fn execute_command_hook(
        &self,
        event: HookEvent,
        command: &str,
        timeout_ms: Option<u64>,
        context: &HookContext,
    ) -> Result<HookResult, HookError> {
        // Expand variables in command
        let expanded_command = self.expand_variables(command);

        // Build environment variables
        let env = self.build_environment(event, context);

        let _timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(self.default_timeout);

        // Determine shell
        let (shell, shell_arg) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        // Execute command
        let output = std::process::Command::new(shell)
            .arg(shell_arg)
            .arg(&expanded_command)
            .current_dir(&self.workspace)
            .envs(env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(HookError::CommandFailed(stderr.to_string()));
        }

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stdout = if stdout.len() > MAX_HOOK_OUTPUT_SIZE {
            &stdout[..MAX_HOOK_OUTPUT_SIZE]
        } else {
            &stdout
        };

        // Try to parse as JSON HookResult
        if let Ok(result) = serde_json::from_str::<HookResult>(stdout.trim()) {
            return Ok(result);
        }

        // If not JSON, treat non-empty output as additional context
        if stdout.trim().is_empty() {
            Ok(HookResult::empty(event))
        } else {
            Ok(HookResult::with_context(event, stdout.trim().to_string()))
        }
    }

    /// Expand variables in command string
    fn expand_variables(&self, command: &str) -> String {
        let mut result = command.to_string();

        if let Some(root) = &self.plugin_root {
            result = result.replace("${CLAUDE_PLUGIN_ROOT}", &root.to_string_lossy());
        }

        result = result.replace("${WORKSPACE}", &self.workspace.to_string_lossy());

        result
    }

    /// Build environment variables for hook execution
    fn build_environment(&self, event: HookEvent, context: &HookContext) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = std::env::vars().collect();

        // Standard hook variables
        env.insert("CLAUDE_HOOK_EVENT".to_string(), event.to_string());
        env.insert("CLAUDE_SESSION_ID".to_string(), context.session_id.clone());
        env.insert("CLAUDE_WORKSPACE".to_string(), self.workspace.to_string_lossy().to_string());

        // Tool-specific variables
        if let Some(tool_name) = &context.tool_name {
            env.insert("CLAUDE_TOOL_NAME".to_string(), tool_name.clone());
        }
        if let Some(tool_args) = &context.tool_args {
            env.insert("CLAUDE_TOOL_ARGS".to_string(), tool_args.to_string());
        }
        if let Some(tool_result) = &context.tool_result {
            // Truncate large results
            let truncated = if tool_result.len() > 10000 {
                format!("{}...[truncated]", &tool_result[..10000])
            } else {
                tool_result.clone()
            };
            env.insert("CLAUDE_TOOL_RESULT".to_string(), truncated);
        }

        // User prompt variable
        if let Some(prompt) = &context.user_prompt {
            env.insert("CLAUDE_USER_PROMPT".to_string(), prompt.clone());
        }

        // Plugin root if available
        if let Some(root) = &self.plugin_root {
            env.insert("CLAUDE_PLUGIN_ROOT".to_string(), root.to_string_lossy().to_string());
        }

        env
    }

    /// Check if a matcher matches the current context
    fn matches_context(&self, matcher_str: &str, context: &HookContext) -> bool {
        let matcher = HookMatcher::parse(matcher_str);

        match (&context.tool_name, &context.tool_args) {
            (Some(name), Some(args)) => matcher.matches(name, args),
            _ => matches!(matcher, HookMatcher::All),
        }
    }
}

/// Load hooks configuration from a file path
pub fn load_hooks_config(path: &std::path::Path) -> Result<HooksConfig, HookError> {
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|e| HookError::InvalidOutput(format!("Failed to parse hooks.json: {}", e)))
}

/// Load hooks configuration from multiple paths (merges them)
pub fn load_hooks_from_paths(paths: &[PathBuf]) -> HooksConfig {
    let mut config = HooksConfig::new();

    for path in paths {
        let hooks_file = path.join("hooks").join("hooks.json");
        if hooks_file.exists()
            && let Ok(loaded) = load_hooks_config(&hooks_file)
        {
            config.merge(loaded);
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::hooks::HookRegistration;
    use std::fs;
    use tempfile::TempDir;

    mod hook_context_tests {
        use super::*;

        #[test]
        fn test_session_start() {
            let ctx = HookContext::session_start("session-123");
            assert_eq!(ctx.session_id, "session-123");
            assert!(ctx.tool_name.is_none());
        }

        #[test]
        fn test_user_prompt() {
            let ctx = HookContext::user_prompt("session-123", "Hello world");
            assert_eq!(ctx.session_id, "session-123");
            assert_eq!(ctx.user_prompt, Some("Hello world".to_string()));
        }

        #[test]
        fn test_pre_tool_use() {
            let ctx = HookContext::pre_tool_use(
                "session-123",
                "Bash",
                serde_json::json!({"command": "ls"}),
            );
            assert_eq!(ctx.tool_name, Some("Bash".to_string()));
            assert!(ctx.tool_args.is_some());
        }

        #[test]
        fn test_post_tool_use() {
            let ctx = HookContext::post_tool_use(
                "session-123",
                "Bash",
                serde_json::json!({"command": "ls"}),
                "file1.txt\nfile2.txt",
            );
            assert_eq!(ctx.tool_result, Some("file1.txt\nfile2.txt".to_string()));
        }
    }

    mod hook_executor_tests {
        use super::*;

        fn create_executor(dir: &TempDir) -> HookExecutor {
            HookExecutor::new(dir.path().to_path_buf())
        }

        #[test]
        fn test_new() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);
            assert_eq!(executor.workspace, dir.path());
        }

        #[test]
        fn test_expand_variables() {
            let dir = TempDir::new().unwrap();
            let executor = HookExecutor::new(dir.path().to_path_buf())
                .with_plugin_root(PathBuf::from("/plugins/my-plugin"));

            let expanded = executor.expand_variables("cd ${WORKSPACE} && ${CLAUDE_PLUGIN_ROOT}/run.sh");
            assert!(expanded.contains(dir.path().to_str().unwrap()));
            assert!(expanded.contains("/plugins/my-plugin"));
        }

        #[test]
        fn test_execute_prompt_hook() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let hook = HookDefinition {
                description: None,
                handler: HookHandler::Prompt {
                    content: "Remember to test".to_string(),
                },
            };

            let ctx = HookContext::session_start("test");
            let result = executor.execute_hook(HookEvent::SessionStart, &hook, &ctx).unwrap();

            assert_eq!(result.additional_context, Some("Remember to test".to_string()));
            assert!(!result.block);
        }

        #[test]
        fn test_execute_command_hook_simple() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let hook = HookDefinition {
                description: None,
                handler: HookHandler::Command {
                    command: "echo 'hello world'".to_string(),
                    timeout_ms: None,
                },
            };

            let ctx = HookContext::session_start("test");
            let result = executor.execute_hook(HookEvent::SessionStart, &hook, &ctx).unwrap();

            assert_eq!(result.additional_context, Some("hello world".to_string()));
        }

        #[test]
        fn test_execute_command_hook_json_output() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let hook = HookDefinition {
                description: None,
                handler: HookHandler::Command {
                    command: r#"echo '{"hookEventName":"SessionStart","additionalContext":"from json","block":false}'"#.to_string(),
                    timeout_ms: None,
                },
            };

            let ctx = HookContext::session_start("test");
            let result = executor.execute_hook(HookEvent::SessionStart, &hook, &ctx).unwrap();

            assert_eq!(result.additional_context, Some("from json".to_string()));
        }

        #[test]
        fn test_execute_command_hook_block() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let hook = HookDefinition {
                description: None,
                handler: HookHandler::Command {
                    command: r#"echo '{"hookEventName":"PreToolUse","block":true,"blockReason":"Security policy"}'"#.to_string(),
                    timeout_ms: None,
                },
            };

            let ctx = HookContext::pre_tool_use("test", "Bash", serde_json::json!({}));
            let result = executor.execute_hook(HookEvent::PreToolUse, &hook, &ctx).unwrap();

            assert!(result.block);
            assert_eq!(result.block_reason, Some("Security policy".to_string()));
        }

        #[test]
        fn test_execute_command_hook_failure() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let hook = HookDefinition {
                description: None,
                handler: HookHandler::Command {
                    command: "exit 1".to_string(),
                    timeout_ms: None,
                },
            };

            let ctx = HookContext::session_start("test");
            let result = executor.execute_hook(HookEvent::SessionStart, &hook, &ctx);

            assert!(matches!(result, Err(HookError::CommandFailed(_))));
        }

        #[test]
        fn test_execute_with_config() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let mut config = HooksConfig::new();
            config.session_start.push(HookRegistration {
                description: None,
                matcher: None,
                hooks: vec![
                    HookDefinition {
                        description: None,
                        handler: HookHandler::Prompt {
                            content: "First hook".to_string(),
                        },
                    },
                    HookDefinition {
                        description: None,
                        handler: HookHandler::Prompt {
                            content: "Second hook".to_string(),
                        },
                    },
                ],
            });

            let ctx = HookContext::session_start("test");
            let results = executor.execute(HookEvent::SessionStart, &config, &ctx);

            assert_eq!(results.len(), 2);
            assert!(results[0].is_ok());
            assert!(results[1].is_ok());
        }

        #[test]
        fn test_execute_with_matcher() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let mut config = HooksConfig::new();
            config.pre_tool_use.push(HookRegistration {
                description: None,
                matcher: Some("Bash".to_string()),
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt {
                        content: "Bash hook".to_string(),
                    },
                }],
            });
            config.pre_tool_use.push(HookRegistration {
                description: None,
                matcher: Some("Write".to_string()),
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt {
                        content: "Write hook".to_string(),
                    },
                }],
            });

            // Should match Bash hook
            let ctx = HookContext::pre_tool_use("test", "Bash", serde_json::json!({}));
            let results = executor.execute(HookEvent::PreToolUse, &config, &ctx);
            assert_eq!(results.len(), 1);
            let result = results[0].as_ref().unwrap();
            assert_eq!(result.additional_context, Some("Bash hook".to_string()));

            // Should match Write hook
            let ctx = HookContext::pre_tool_use("test", "Write", serde_json::json!({}));
            let results = executor.execute(HookEvent::PreToolUse, &config, &ctx);
            assert_eq!(results.len(), 1);
            let result = results[0].as_ref().unwrap();
            assert_eq!(result.additional_context, Some("Write hook".to_string()));
        }

        #[test]
        fn test_execute_with_pattern_matcher() {
            let dir = TempDir::new().unwrap();
            let executor = create_executor(&dir);

            let mut config = HooksConfig::new();
            config.pre_tool_use.push(HookRegistration {
                description: None,
                matcher: Some("Bash(git:*)".to_string()),
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt {
                        content: "Git hook".to_string(),
                    },
                }],
            });

            // Should match git command
            let ctx = HookContext::pre_tool_use(
                "test",
                "Bash",
                serde_json::json!({"command": "git status"}),
            );
            let results = executor.execute(HookEvent::PreToolUse, &config, &ctx);
            assert_eq!(results.len(), 1);

            // Should not match npm command
            let ctx = HookContext::pre_tool_use(
                "test",
                "Bash",
                serde_json::json!({"command": "npm install"}),
            );
            let results = executor.execute(HookEvent::PreToolUse, &config, &ctx);
            assert!(results.is_empty());
        }

        #[test]
        fn test_environment_variables() {
            let dir = TempDir::new().unwrap();
            let executor = HookExecutor::new(dir.path().to_path_buf())
                .with_plugin_root(PathBuf::from("/test/plugin"));

            let ctx = HookContext::pre_tool_use(
                "session-abc",
                "Bash",
                serde_json::json!({"command": "test"}),
            );

            let env = executor.build_environment(HookEvent::PreToolUse, &ctx);

            assert_eq!(env.get("CLAUDE_HOOK_EVENT"), Some(&"PreToolUse".to_string()));
            assert_eq!(env.get("CLAUDE_SESSION_ID"), Some(&"session-abc".to_string()));
            assert_eq!(env.get("CLAUDE_TOOL_NAME"), Some(&"Bash".to_string()));
            assert_eq!(env.get("CLAUDE_PLUGIN_ROOT"), Some(&"/test/plugin".to_string()));
        }
    }

    mod hooks_loader_tests {
        use super::*;

        #[test]
        fn test_load_hooks_config() {
            let dir = TempDir::new().unwrap();
            let hooks_dir = dir.path().join("hooks");
            fs::create_dir_all(&hooks_dir).unwrap();

            let hooks_json = r#"{
                "SessionStart": [
                    {
                        "hooks": [
                            {"type": "prompt", "content": "Hello"}
                        ]
                    }
                ]
            }"#;

            let hooks_file = hooks_dir.join("hooks.json");
            fs::write(&hooks_file, hooks_json).unwrap();

            let config = load_hooks_config(&hooks_file).unwrap();
            assert_eq!(config.session_start.len(), 1);
        }

        #[test]
        fn test_load_hooks_from_paths() {
            let dir1 = TempDir::new().unwrap();
            let dir2 = TempDir::new().unwrap();

            // Create hooks in dir1
            let hooks_dir1 = dir1.path().join("hooks");
            fs::create_dir_all(&hooks_dir1).unwrap();
            fs::write(
                hooks_dir1.join("hooks.json"),
                r#"{"SessionStart": [{"hooks": [{"type": "prompt", "content": "Hook1"}]}]}"#,
            ).unwrap();

            // Create hooks in dir2
            let hooks_dir2 = dir2.path().join("hooks");
            fs::create_dir_all(&hooks_dir2).unwrap();
            fs::write(
                hooks_dir2.join("hooks.json"),
                r#"{"SessionStart": [{"hooks": [{"type": "prompt", "content": "Hook2"}]}]}"#,
            ).unwrap();

            let paths = vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()];
            let config = load_hooks_from_paths(&paths);

            // Should have merged both hooks
            assert_eq!(config.session_start.len(), 2);
        }

        #[test]
        fn test_load_nonexistent_file() {
            let config = load_hooks_from_paths(&[PathBuf::from("/nonexistent/path")]);
            assert!(config.is_empty());
        }
    }
}
