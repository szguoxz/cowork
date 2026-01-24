//! Integration tests for the Prompt System
//!
//! These tests verify:
//! - ComponentRegistry initialization and loading
//! - PromptBuilder composition
//! - Hook execution flow
//! - Agent and Command definitions
//! - Full integration with SessionConfig

use std::sync::Arc;
use tempfile::TempDir;

use cowork_core::config::PromptSystemConfig;
use cowork_core::prompt::{
    ComponentRegistry, HookContext, HookEvent, HookExecutor, HooksConfig,
    HookHandler, HookDefinition, HookRegistration,
    PromptBuilder, parse_agent, parse_command, parse_command_named, Scope, ModelPreference,
    ToolRestrictions, ToolSpec, builtin,
};

// ==========================================================================
// ComponentRegistry Tests
// ==========================================================================

mod component_registry_tests {
    use super::*;

    #[test]
    fn test_registry_with_builtins() {
        let registry = ComponentRegistry::with_builtins();

        // Check agents are loaded
        assert!(registry.get_agent("Explore").is_some());
        assert!(registry.get_agent("Plan").is_some());
        assert!(registry.get_agent("Bash").is_some());
        assert!(registry.get_agent("general-purpose").is_some());

        // Check commands are loaded (6 official Claude Code plugin commands)
        assert!(registry.get_command("commit").is_some());
        assert!(registry.get_command("commit-push-pr").is_some());
        assert!(registry.get_command("review-pr").is_some());
    }

    #[test]
    fn test_registry_register_agent() {
        let mut registry = ComponentRegistry::with_builtins();
        let initial_count = registry.agent_count();

        // Parse and register a custom agent
        let agent = parse_agent(r#"---
name: CustomAgent
description: "A custom project agent"
model: haiku
tools: Read, Glob
---

You are a custom agent for this project.
"#, None, Scope::Project).unwrap();

        registry.register_agent(agent);

        // Should have the custom agent
        let agent = registry.get_agent("CustomAgent");
        assert!(agent.is_some());
        assert_eq!(agent.unwrap().description(), "A custom project agent");
        assert_eq!(registry.agent_count(), initial_count + 1);
    }
}

// ==========================================================================
// Agent Definition Tests
// ==========================================================================

mod agent_definition_tests {
    use super::*;

    #[test]
    fn test_parse_agent_with_all_fields() {
        let content = r#"---
name: TestAgent
description: "A test agent for integration testing"
model: haiku
color: cyan
tools: Read, Glob, Grep
context: fork
max_turns: 50
---

# Test Agent

You are a specialized test agent.

## Capabilities
- Reading files
- Searching with glob patterns
- Grepping content
"#;
        let agent = parse_agent(content, None, Scope::Project).unwrap();

        assert_eq!(agent.name(), "TestAgent");
        assert_eq!(agent.description(), "A test agent for integration testing");
        assert_eq!(*agent.model(), ModelPreference::Haiku);
        assert_eq!(agent.max_turns(), Some(50));
        assert!(agent.system_prompt.contains("specialized test agent"));
    }

    #[test]
    fn test_agent_tool_restrictions() {
        let content = r#"---
name: ReadOnly
tools: Read, Glob, Grep
---

Read-only agent.
"#;
        let agent = parse_agent(content, None, Scope::Project).unwrap();
        let restrictions = agent.tool_restrictions();

        assert!(restrictions.is_allowed("Read", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Glob", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Grep", &serde_json::json!({})));
        assert!(!restrictions.is_allowed("Write", &serde_json::json!({})));
        assert!(!restrictions.is_allowed("Bash", &serde_json::json!({})));
    }

    #[test]
    fn test_agent_wildcard_tools() {
        let content = r#"---
name: AllTools
tools: "*"
---

Full access agent.
"#;
        let agent = parse_agent(content, None, Scope::Project).unwrap();
        let restrictions = agent.tool_restrictions();

        // Wildcard should allow all tools
        assert!(restrictions.is_allowed("AnyTool", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Read", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Bash", &serde_json::json!({})));
    }

    #[test]
    fn test_builtin_explore_agent() {
        let agent = parse_agent(builtin::agents::EXPLORE, None, Scope::Builtin).unwrap();

        assert_eq!(agent.name(), "Explore");
        assert_eq!(*agent.model(), ModelPreference::Haiku);
        assert!(agent.max_turns().is_some());

        // Check tool restrictions
        let restrictions = agent.tool_restrictions();
        assert!(restrictions.is_allowed("Glob", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Grep", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Read", &serde_json::json!({})));
        assert!(!restrictions.is_allowed("Write", &serde_json::json!({})));
    }
}

// ==========================================================================
// Command Definition Tests
// ==========================================================================

mod command_definition_tests {
    use super::*;

    #[test]
    fn test_parse_command_basic() {
        let content = r#"---
name: test-cmd
description: "A test command"
---

This is a test command.
"#;
        let cmd = parse_command(content, None, Scope::Project).unwrap();

        assert_eq!(cmd.name(), "test-cmd");
        assert_eq!(cmd.description(), "A test command");
    }

    #[test]
    fn test_builtin_commit_command() {
        let cmd = parse_command_named(builtin::commands::COMMIT, "commit", None, Scope::Builtin).unwrap();

        assert_eq!(cmd.name(), "commit");
        assert!(!cmd.description().is_empty());
    }
}

// ==========================================================================
// Hook System Tests
// ==========================================================================

mod hook_system_tests {
    use super::*;

    #[test]
    fn test_hooks_config_construction() {
        let mut config = HooksConfig::new();

        config.session_start.push(HookRegistration {
            description: Some("Session start hook".to_string()),
            matcher: None,
            hooks: vec![
                HookDefinition {
                    description: Some("Add reminder".to_string()),
                    handler: HookHandler::Prompt {
                        content: "Remember to test carefully".to_string(),
                    },
                },
            ],
        });

        assert!(!config.is_empty());
        assert_eq!(config.get_hooks(HookEvent::SessionStart).len(), 1);
        assert_eq!(config.get_hooks(HookEvent::PreToolUse).len(), 0);
    }

    #[test]
    fn test_hook_executor_prompt_hook() {
        let temp = TempDir::new().unwrap();
        let executor = HookExecutor::new(temp.path().to_path_buf());

        let hook = HookDefinition {
            description: None,
            handler: HookHandler::Prompt {
                content: "Test context".to_string(),
            },
        };

        let mut config = HooksConfig::new();
        config.session_start.push(HookRegistration {
            description: None,
            matcher: None,
            hooks: vec![hook],
        });

        let context = HookContext::session_start("test-session");
        let results = executor.execute(HookEvent::SessionStart, &config, &context);

        assert_eq!(results.len(), 1);
        let result = results[0].as_ref().unwrap();
        assert_eq!(result.additional_context, Some("Test context".to_string()));
        assert!(!result.block);
    }

    #[test]
    fn test_hook_matcher() {
        let temp = TempDir::new().unwrap();
        let executor = HookExecutor::new(temp.path().to_path_buf());

        let mut config = HooksConfig::new();

        // Hook that only matches Bash tool
        config.pre_tool_use.push(HookRegistration {
            description: None,
            matcher: Some("Bash".to_string()),
            hooks: vec![HookDefinition {
                description: None,
                handler: HookHandler::Prompt {
                    content: "Bash context".to_string(),
                },
            }],
        });

        // Hook that matches Write tool
        config.pre_tool_use.push(HookRegistration {
            description: None,
            matcher: Some("Write".to_string()),
            hooks: vec![HookDefinition {
                description: None,
                handler: HookHandler::Prompt {
                    content: "Write context".to_string(),
                },
            }],
        });

        // Test Bash tool
        let bash_ctx = HookContext::pre_tool_use(
            "test-session",
            "Bash",
            serde_json::json!({"command": "ls"}),
        );
        let results = executor.execute(HookEvent::PreToolUse, &config, &bash_ctx);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].as_ref().unwrap().additional_context,
            Some("Bash context".to_string())
        );

        // Test Write tool
        let write_ctx = HookContext::pre_tool_use(
            "test-session",
            "Write",
            serde_json::json!({"file_path": "/test.txt"}),
        );
        let results = executor.execute(HookEvent::PreToolUse, &config, &write_ctx);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].as_ref().unwrap().additional_context,
            Some("Write context".to_string())
        );

        // Test Read tool (should match neither)
        let read_ctx = HookContext::pre_tool_use(
            "test-session",
            "Read",
            serde_json::json!({"file_path": "/test.txt"}),
        );
        let results = executor.execute(HookEvent::PreToolUse, &config, &read_ctx);
        assert!(results.is_empty());
    }

    #[test]
    fn test_hook_command_execution() {
        let temp = TempDir::new().unwrap();
        let executor = HookExecutor::new(temp.path().to_path_buf());

        let hook = HookDefinition {
            description: None,
            handler: HookHandler::Command {
                command: "echo 'hello from hook'".to_string(),
                timeout_ms: None,
            },
        };

        let mut config = HooksConfig::new();
        config.session_start.push(HookRegistration {
            description: None,
            matcher: None,
            hooks: vec![hook],
        });

        let context = HookContext::session_start("test-session");
        let results = executor.execute(HookEvent::SessionStart, &config, &context);

        assert_eq!(results.len(), 1);
        let result = results[0].as_ref().unwrap();
        assert_eq!(result.additional_context, Some("hello from hook".to_string()));
    }

    #[test]
    fn test_hook_block_result() {
        let temp = TempDir::new().unwrap();
        let executor = HookExecutor::new(temp.path().to_path_buf());

        // Hook that outputs a blocking JSON result
        let hook = HookDefinition {
            description: None,
            handler: HookHandler::Command {
                command: r#"echo '{"hookEventName":"PreToolUse","block":true,"blockReason":"Security policy"}'"#.to_string(),
                timeout_ms: None,
            },
        };

        let mut config = HooksConfig::new();
        config.pre_tool_use.push(HookRegistration {
            description: None,
            matcher: None,
            hooks: vec![hook],
        });

        let context = HookContext::pre_tool_use(
            "test-session",
            "Bash",
            serde_json::json!({"command": "rm -rf /"}),
        );
        let results = executor.execute(HookEvent::PreToolUse, &config, &context);

        assert_eq!(results.len(), 1);
        let result = results[0].as_ref().unwrap();
        assert!(result.block);
        assert_eq!(result.block_reason, Some("Security policy".to_string()));
    }
}

// ==========================================================================
// PromptBuilder Tests
// ==========================================================================

mod prompt_builder_tests {
    use super::*;

    #[test]
    fn test_prompt_builder_basic() {
        let builder = PromptBuilder::new(builtin::SYSTEM_PROMPT);
        let result = builder.build();

        assert!(!result.system_prompt.is_empty());
    }

    #[test]
    fn test_prompt_builder_with_hook_context() {
        let builder = PromptBuilder::new(builtin::SYSTEM_PROMPT)
            .with_hook_context("Additional context from hooks");

        let result = builder.build();

        assert!(result.system_prompt.contains("Additional context from hooks"));
    }

    #[test]
    fn test_prompt_builder_with_agent() {
        let agent = parse_agent(builtin::agents::EXPLORE, None, Scope::Builtin).unwrap();

        let builder = PromptBuilder::new(builtin::SYSTEM_PROMPT)
            .with_agent(agent);

        let result = builder.build();

        // Agent prompt should be included
        assert!(!result.system_prompt.is_empty());
    }

    #[test]
    fn test_prompt_builder_empty() {
        let builder = PromptBuilder::empty();
        let result = builder.build();

        // Empty builder should produce empty prompt
        assert!(result.system_prompt.is_empty());
    }
}

// ==========================================================================
// Tool Restrictions Tests
// ==========================================================================

mod tool_restrictions_tests {
    use super::*;

    #[test]
    fn test_allow_only_specific_tools() {
        let restrictions = ToolRestrictions::allow_only(vec![
            ToolSpec::Name("Read".to_string()),
            ToolSpec::Name("Glob".to_string()),
        ]);

        assert!(restrictions.is_allowed("Read", &serde_json::json!({})));
        assert!(restrictions.is_allowed("Glob", &serde_json::json!({})));
        assert!(!restrictions.is_allowed("Write", &serde_json::json!({})));
    }

    #[test]
    fn test_empty_restrictions() {
        let restrictions = ToolRestrictions::new();

        // Empty restrictions allow everything
        assert!(restrictions.is_empty());
        assert!(restrictions.is_allowed("AnyTool", &serde_json::json!({})));
    }

    #[test]
    fn test_tool_spec_name() {
        let spec = ToolSpec::Name("Read".to_string());

        assert!(spec.matches("Read", &serde_json::json!({})));
        assert!(!spec.matches("Write", &serde_json::json!({})));
    }
}

// ==========================================================================
// SessionConfig Integration Tests
// ==========================================================================

mod session_config_tests {
    use super::*;
    use cowork_core::session::SessionConfig;
    use cowork_core::provider::ProviderType;

    #[test]
    fn test_session_config_with_prompt_system() {
        let registry = Arc::new(ComponentRegistry::with_builtins());

        let config = SessionConfig::new("/test/workspace")
            .with_provider(ProviderType::Anthropic)
            .with_prompt_config(PromptSystemConfig {
                enable_hooks: true,
                enable_plugins: true,
                ..Default::default()
            })
            .with_component_registry(registry.clone());

        assert!(config.component_registry.is_some());
        assert!(config.prompt_config.enable_hooks);
    }

    #[test]
    fn test_session_config_defaults() {
        let config = SessionConfig::default();

        // PromptSystemConfig has default values (hooks enabled by default)
        // ComponentRegistry is None by default
        assert!(config.component_registry.is_none());
    }
}

// ==========================================================================
// End-to-end Integration Tests
// ==========================================================================

mod e2e_tests {
    use super::*;

    #[test]
    fn test_full_prompt_system_initialization() {
        // Create a custom agent programmatically
        let reviewer = parse_agent(r#"---
name: Reviewer
description: "Code review agent"
model: sonnet
tools: Read, Glob, Grep
---

You are a code review specialist.
"#, None, Scope::Project).unwrap();

        // Initialize registry with discovery
        let mut registry = ComponentRegistry::with_builtins();

        // Register the custom agent
        registry.register_agent(reviewer);

        // Verify custom agent was loaded
        let reviewer = registry.get_agent("Reviewer");
        assert!(reviewer.is_some());
        assert_eq!(reviewer.unwrap().description(), "Code review agent");
    }

    #[test]
    fn test_agent_executor_uses_registry() {
        use cowork_core::tools::task::executor::{
            AgentExecutionConfig, get_system_prompt_dynamic, get_agent_model_preference
        };
        use cowork_core::tools::task::AgentType;

        let temp = TempDir::new().unwrap();
        let registry = ComponentRegistry::with_builtins();

        // Get system prompt for Explore agent from registry
        let prompt = get_system_prompt_dynamic(&AgentType::Explore, Some(&registry));
        assert!(!prompt.is_empty());

        // Get model preference
        let pref = get_agent_model_preference(&AgentType::Explore, Some(&registry));
        // Should return the registry's preference or Inherit
        let _ = pref;

        // Verify config can include registry
        let config = AgentExecutionConfig::new(temp.path().to_path_buf())
            .with_registry(Arc::new(registry));

        assert!(config.registry.is_some());
    }
}
