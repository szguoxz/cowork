//! Hook System for Prompt Injection
//!
//! This module implements event-based prompt injection mechanisms:
//! - Hook events for lifecycle points (SessionStart, PreToolUse, etc.)
//! - Hook handlers for different action types (Command, Prompt, McpTool)
//! - Hook matchers for tool-specific hooks
//! - Hook results for controlling behavior

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Hook event types corresponding to different lifecycle points
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    /// Session starts - fired once when a new session begins
    SessionStart,
    /// User submits a prompt - fired for each user message
    UserPromptSubmit,
    /// Before tool execution - can block or modify tool calls
    PreToolUse,
    /// After tool execution - can add context based on results
    PostToolUse,
    /// Main agent stops
    Stop,
    /// Subagent stops
    SubagentStop,
    /// Before context compaction - inject important context to preserve
    PreCompact,
    /// Notification event
    Notification,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookEvent::SessionStart => write!(f, "SessionStart"),
            HookEvent::UserPromptSubmit => write!(f, "UserPromptSubmit"),
            HookEvent::PreToolUse => write!(f, "PreToolUse"),
            HookEvent::PostToolUse => write!(f, "PostToolUse"),
            HookEvent::Stop => write!(f, "Stop"),
            HookEvent::SubagentStop => write!(f, "SubagentStop"),
            HookEvent::PreCompact => write!(f, "PreCompact"),
            HookEvent::Notification => write!(f, "Notification"),
        }
    }
}

/// Matcher for tool-specific hooks
///
/// Used with PreToolUse/PostToolUse to filter which tool calls trigger the hook.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookMatcher {
    /// Match all tools
    #[default]
    All,
    /// Match by exact tool name
    ToolName(String),
    /// Match by tool name with pattern: "Bash(git:*)", "Write(src/*:*)"
    Pattern(String),
}

impl HookMatcher {
    /// Parse a matcher from a string
    ///
    /// - "*" → All
    /// - "Bash" → ToolName("Bash")
    /// - "Bash(git:*)" → Pattern("Bash(git:*)")
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s == "*" {
            HookMatcher::All
        } else if s.contains('(') && s.ends_with(')') {
            HookMatcher::Pattern(s.to_string())
        } else {
            HookMatcher::ToolName(s.to_string())
        }
    }

    /// Check if this matcher matches a tool invocation
    pub fn matches(&self, tool_name: &str, args: &Value) -> bool {
        match self {
            HookMatcher::All => true,
            HookMatcher::ToolName(name) => name == tool_name,
            HookMatcher::Pattern(pattern) => {
                // Use ToolSpec for pattern matching
                use super::types::ToolSpec;
                let spec = ToolSpec::parse(pattern);
                spec.matches(tool_name, args)
            }
        }
    }
}


impl fmt::Display for HookMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookMatcher::All => write!(f, "*"),
            HookMatcher::ToolName(name) => write!(f, "{}", name),
            HookMatcher::Pattern(pattern) => write!(f, "{}", pattern),
        }
    }
}

/// Hook handler types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookHandler {
    /// Execute a shell command
    Command {
        /// The command to execute
        command: String,
        /// Optional timeout in milliseconds (defaults to 30000)
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    /// Inline prompt content injection
    Prompt {
        /// Content to inject into the prompt
        content: String,
    },
    /// MCP tool invocation
    McpTool {
        /// MCP server name
        server: String,
        /// Tool name on the server
        tool: String,
        /// Arguments to pass to the tool
        #[serde(default)]
        args: Value,
    },
}

/// Result returned from hook execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// The event that triggered this hook
    #[serde(rename = "hookEventName")]
    pub hook_event_name: HookEvent,

    /// Additional context to inject into the system prompt
    #[serde(default, rename = "additionalContext")]
    pub additional_context: Option<String>,

    /// Whether to block the action (for Pre* hooks)
    #[serde(default)]
    pub block: bool,

    /// Human-readable reason for blocking
    #[serde(default, rename = "blockReason")]
    pub block_reason: Option<String>,

    /// Modified tool arguments (for PreToolUse only)
    #[serde(default, rename = "modifiedArgs")]
    pub modified_args: Option<Value>,
}

impl HookResult {
    /// Create a simple result with just context injection
    pub fn with_context(event: HookEvent, context: String) -> Self {
        Self {
            hook_event_name: event,
            additional_context: Some(context),
            block: false,
            block_reason: None,
            modified_args: None,
        }
    }

    /// Create a blocking result
    pub fn blocked(event: HookEvent, reason: impl Into<String>) -> Self {
        Self {
            hook_event_name: event,
            additional_context: None,
            block: true,
            block_reason: Some(reason.into()),
            modified_args: None,
        }
    }

    /// Create a result that modifies tool arguments
    pub fn with_modified_args(event: HookEvent, args: Value) -> Self {
        Self {
            hook_event_name: event,
            additional_context: None,
            block: false,
            block_reason: None,
            modified_args: Some(args),
        }
    }

    /// Create an empty result (no-op)
    pub fn empty(event: HookEvent) -> Self {
        Self {
            hook_event_name: event,
            additional_context: None,
            block: false,
            block_reason: None,
            modified_args: None,
        }
    }
}

/// Single hook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// The handler to execute
    #[serde(flatten)]
    pub handler: HookHandler,
}

/// Hook registration for an event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRegistration {
    /// Optional description for this group of hooks
    #[serde(default)]
    pub description: Option<String>,
    /// Tool matcher (for PreToolUse/PostToolUse)
    #[serde(default)]
    pub matcher: Option<String>,
    /// List of hooks to execute
    pub hooks: Vec<HookDefinition>,
}

/// Complete hooks configuration loaded from hooks.json
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HooksConfig {
    /// Session start hooks
    #[serde(default)]
    pub session_start: Vec<HookRegistration>,
    /// User prompt submit hooks
    #[serde(default)]
    pub user_prompt_submit: Vec<HookRegistration>,
    /// Pre tool use hooks
    #[serde(default)]
    pub pre_tool_use: Vec<HookRegistration>,
    /// Post tool use hooks
    #[serde(default)]
    pub post_tool_use: Vec<HookRegistration>,
    /// Stop hooks
    #[serde(default)]
    pub stop: Vec<HookRegistration>,
    /// Subagent stop hooks
    #[serde(default)]
    pub subagent_stop: Vec<HookRegistration>,
    /// Pre compact hooks
    #[serde(default)]
    pub pre_compact: Vec<HookRegistration>,
    /// Notification hooks
    #[serde(default)]
    pub notification: Vec<HookRegistration>,
}

impl HooksConfig {
    /// Create empty config
    pub fn new() -> Self {
        Self::default()
    }

    /// Get hooks for a specific event
    pub fn get_hooks(&self, event: HookEvent) -> &[HookRegistration] {
        match event {
            HookEvent::SessionStart => &self.session_start,
            HookEvent::UserPromptSubmit => &self.user_prompt_submit,
            HookEvent::PreToolUse => &self.pre_tool_use,
            HookEvent::PostToolUse => &self.post_tool_use,
            HookEvent::Stop => &self.stop,
            HookEvent::SubagentStop => &self.subagent_stop,
            HookEvent::PreCompact => &self.pre_compact,
            HookEvent::Notification => &self.notification,
        }
    }

    /// Merge another config into this one (additive)
    pub fn merge(&mut self, other: HooksConfig) {
        self.session_start.extend(other.session_start);
        self.user_prompt_submit.extend(other.user_prompt_submit);
        self.pre_tool_use.extend(other.pre_tool_use);
        self.post_tool_use.extend(other.post_tool_use);
        self.stop.extend(other.stop);
        self.subagent_stop.extend(other.subagent_stop);
        self.pre_compact.extend(other.pre_compact);
        self.notification.extend(other.notification);
    }

    /// Check if config has any hooks defined
    pub fn is_empty(&self) -> bool {
        self.session_start.is_empty()
            && self.user_prompt_submit.is_empty()
            && self.pre_tool_use.is_empty()
            && self.post_tool_use.is_empty()
            && self.stop.is_empty()
            && self.subagent_stop.is_empty()
            && self.pre_compact.is_empty()
            && self.notification.is_empty()
    }

    /// Count total hooks
    pub fn total_hooks(&self) -> usize {
        self.session_start.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.user_prompt_submit.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.pre_tool_use.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.post_tool_use.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.stop.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.subagent_stop.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.pre_compact.iter().map(|r| r.hooks.len()).sum::<usize>()
            + self.notification.iter().map(|r| r.hooks.len()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    mod hook_event_tests {
        use super::*;

        #[test]
        fn test_display() {
            assert_eq!(HookEvent::SessionStart.to_string(), "SessionStart");
            assert_eq!(HookEvent::PreToolUse.to_string(), "PreToolUse");
            assert_eq!(HookEvent::PostToolUse.to_string(), "PostToolUse");
        }

        #[test]
        fn test_serde() {
            let event = HookEvent::SessionStart;
            let json = serde_json::to_string(&event).unwrap();
            assert_eq!(json, "\"SessionStart\"");

            let parsed: HookEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, event);
        }

        #[test]
        fn test_all_events() {
            let events = vec![
                HookEvent::SessionStart,
                HookEvent::UserPromptSubmit,
                HookEvent::PreToolUse,
                HookEvent::PostToolUse,
                HookEvent::Stop,
                HookEvent::SubagentStop,
                HookEvent::PreCompact,
                HookEvent::Notification,
            ];

            for event in events {
                let json = serde_json::to_string(&event).unwrap();
                let parsed: HookEvent = serde_json::from_str(&json).unwrap();
                assert_eq!(parsed, event);
            }
        }
    }

    mod hook_matcher_tests {
        use super::*;

        #[test]
        fn test_parse_all() {
            assert_eq!(HookMatcher::parse("*"), HookMatcher::All);
        }

        #[test]
        fn test_parse_tool_name() {
            assert_eq!(
                HookMatcher::parse("Bash"),
                HookMatcher::ToolName("Bash".to_string())
            );
        }

        #[test]
        fn test_parse_pattern() {
            assert_eq!(
                HookMatcher::parse("Bash(git:*)"),
                HookMatcher::Pattern("Bash(git:*)".to_string())
            );
        }

        #[test]
        fn test_matches_all() {
            let matcher = HookMatcher::All;
            assert!(matcher.matches("Bash", &json!({})));
            assert!(matcher.matches("Write", &json!({})));
        }

        #[test]
        fn test_matches_tool_name() {
            let matcher = HookMatcher::ToolName("Bash".to_string());
            assert!(matcher.matches("Bash", &json!({})));
            assert!(!matcher.matches("Write", &json!({})));
        }

        #[test]
        fn test_matches_pattern() {
            let matcher = HookMatcher::Pattern("Bash(git:*)".to_string());
            assert!(matcher.matches("Bash", &json!({"command": "git status"})));
            assert!(!matcher.matches("Bash", &json!({"command": "npm install"})));
            assert!(!matcher.matches("Write", &json!({"command": "git status"})));
        }

        #[test]
        fn test_display() {
            assert_eq!(HookMatcher::All.to_string(), "*");
            assert_eq!(HookMatcher::ToolName("Bash".to_string()).to_string(), "Bash");
            assert_eq!(
                HookMatcher::Pattern("Bash(git:*)".to_string()).to_string(),
                "Bash(git:*)"
            );
        }
    }

    mod hook_handler_tests {
        use super::*;

        #[test]
        fn test_command_handler_serde() {
            let handler = HookHandler::Command {
                command: "echo hello".to_string(),
                timeout_ms: Some(5000),
            };

            let json = serde_json::to_string(&handler).unwrap();
            assert!(json.contains("\"type\":\"command\""));
            assert!(json.contains("\"command\":\"echo hello\""));

            let parsed: HookHandler = serde_json::from_str(&json).unwrap();
            match parsed {
                HookHandler::Command { command, timeout_ms } => {
                    assert_eq!(command, "echo hello");
                    assert_eq!(timeout_ms, Some(5000));
                }
                _ => panic!("Expected Command handler"),
            }
        }

        #[test]
        fn test_prompt_handler_serde() {
            let handler = HookHandler::Prompt {
                content: "Remember to check tests".to_string(),
            };

            let json = serde_json::to_string(&handler).unwrap();
            assert!(json.contains("\"type\":\"prompt\""));

            let parsed: HookHandler = serde_json::from_str(&json).unwrap();
            match parsed {
                HookHandler::Prompt { content } => {
                    assert_eq!(content, "Remember to check tests");
                }
                _ => panic!("Expected Prompt handler"),
            }
        }

        #[test]
        fn test_mcp_tool_handler_serde() {
            let handler = HookHandler::McpTool {
                server: "my-server".to_string(),
                tool: "my-tool".to_string(),
                args: json!({"key": "value"}),
            };

            let json = serde_json::to_string(&handler).unwrap();
            assert!(json.contains("\"type\":\"mcptool\""));

            let parsed: HookHandler = serde_json::from_str(&json).unwrap();
            match parsed {
                HookHandler::McpTool { server, tool, args } => {
                    assert_eq!(server, "my-server");
                    assert_eq!(tool, "my-tool");
                    assert_eq!(args, json!({"key": "value"}));
                }
                _ => panic!("Expected McpTool handler"),
            }
        }
    }

    mod hook_result_tests {
        use super::*;

        #[test]
        fn test_with_context() {
            let result = HookResult::with_context(
                HookEvent::SessionStart,
                "Project context".to_string(),
            );
            assert_eq!(result.hook_event_name, HookEvent::SessionStart);
            assert_eq!(result.additional_context, Some("Project context".to_string()));
            assert!(!result.block);
        }

        #[test]
        fn test_blocked() {
            let result = HookResult::blocked(HookEvent::PreToolUse, "Security violation");
            assert!(result.block);
            assert_eq!(result.block_reason, Some("Security violation".to_string()));
        }

        #[test]
        fn test_with_modified_args() {
            let result = HookResult::with_modified_args(
                HookEvent::PreToolUse,
                json!({"command": "safe-command"}),
            );
            assert_eq!(result.modified_args, Some(json!({"command": "safe-command"})));
            assert!(!result.block);
        }

        #[test]
        fn test_empty() {
            let result = HookResult::empty(HookEvent::Stop);
            assert_eq!(result.hook_event_name, HookEvent::Stop);
            assert!(result.additional_context.is_none());
            assert!(!result.block);
            assert!(result.modified_args.is_none());
        }

        #[test]
        fn test_serde() {
            let result = HookResult::with_context(
                HookEvent::UserPromptSubmit,
                "Some context".to_string(),
            );

            let json = serde_json::to_string(&result).unwrap();
            let parsed: HookResult = serde_json::from_str(&json).unwrap();

            assert_eq!(parsed.hook_event_name, result.hook_event_name);
            assert_eq!(parsed.additional_context, result.additional_context);
        }
    }

    mod hooks_config_tests {
        use super::*;

        #[test]
        fn test_new_is_empty() {
            let config = HooksConfig::new();
            assert!(config.is_empty());
            assert_eq!(config.total_hooks(), 0);
        }

        #[test]
        fn test_get_hooks() {
            let mut config = HooksConfig::new();
            config.session_start.push(HookRegistration {
                description: None,
                matcher: None,
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt {
                        content: "test".to_string(),
                    },
                }],
            });

            let hooks = config.get_hooks(HookEvent::SessionStart);
            assert_eq!(hooks.len(), 1);

            let hooks = config.get_hooks(HookEvent::PreToolUse);
            assert!(hooks.is_empty());
        }

        #[test]
        fn test_merge() {
            let mut config1 = HooksConfig::new();
            config1.session_start.push(HookRegistration {
                description: None,
                matcher: None,
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt {
                        content: "first".to_string(),
                    },
                }],
            });

            let mut config2 = HooksConfig::new();
            config2.session_start.push(HookRegistration {
                description: None,
                matcher: None,
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt {
                        content: "second".to_string(),
                    },
                }],
            });

            config1.merge(config2);
            assert_eq!(config1.session_start.len(), 2);
        }

        #[test]
        fn test_total_hooks() {
            let mut config = HooksConfig::new();
            config.session_start.push(HookRegistration {
                description: None,
                matcher: None,
                hooks: vec![
                    HookDefinition {
                        description: None,
                        handler: HookHandler::Prompt { content: "1".to_string() },
                    },
                    HookDefinition {
                        description: None,
                        handler: HookHandler::Prompt { content: "2".to_string() },
                    },
                ],
            });
            config.pre_tool_use.push(HookRegistration {
                description: None,
                matcher: Some("Bash".to_string()),
                hooks: vec![HookDefinition {
                    description: None,
                    handler: HookHandler::Prompt { content: "3".to_string() },
                }],
            });

            assert_eq!(config.total_hooks(), 3);
            assert!(!config.is_empty());
        }

        #[test]
        fn test_serde_from_json() {
            let json = r#"{
                "SessionStart": [
                    {
                        "description": "Add project context",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "echo hello"
                            }
                        ]
                    }
                ],
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [
                            {
                                "type": "prompt",
                                "content": "Be careful with bash"
                            }
                        ]
                    }
                ]
            }"#;

            let config: HooksConfig = serde_json::from_str(json).unwrap();
            assert_eq!(config.session_start.len(), 1);
            assert_eq!(config.pre_tool_use.len(), 1);
            assert_eq!(config.pre_tool_use[0].matcher, Some("Bash".to_string()));
        }
    }
}
