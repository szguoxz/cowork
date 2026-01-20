//! Agent execution engine for TaskTool subagents
//!
//! This module provides the core agentic loop that powers subagent execution.
//! Each agent type gets a specific set of tools and a tailored system prompt.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::config::ModelTiers;
use crate::error::Result;
use crate::provider::{create_provider, CompletionResult, LlmMessage, ProviderType};
use crate::tools::filesystem::{
    EditFile, GlobFiles, GrepFiles, ListDirectory, ReadFile, SearchFiles, WriteFile,
};
use crate::tools::lsp::LspTool;
use crate::tools::shell::ExecuteCommand;
use crate::tools::task::TodoWrite;
use crate::tools::web::{WebFetch, WebSearch};
use crate::tools::ToolRegistry;

use super::{AgentInstanceRegistry, AgentStatus, AgentType, ModelTier};

/// Configuration for agent execution
#[derive(Debug, Clone)]
pub struct AgentExecutionConfig {
    /// Workspace root directory
    pub workspace: PathBuf,
    /// LLM provider type
    pub provider_type: ProviderType,
    /// Optional API key (uses environment variable if None)
    pub api_key: Option<String>,
    /// Maximum number of agentic turns before stopping
    pub max_turns: u64,
    /// Model tiers for selecting models (config-driven or defaults)
    pub model_tiers: ModelTiers,
}

impl AgentExecutionConfig {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            provider_type: ProviderType::Anthropic,
            api_key: None,
            max_turns: 50,
            model_tiers: ModelTiers::anthropic(),
        }
    }

    pub fn with_provider(mut self, provider_type: ProviderType) -> Self {
        self.provider_type = provider_type;
        // Update model tiers to match provider defaults
        self.model_tiers = ModelTiers::for_provider(&provider_type.to_string());
        self
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    pub fn with_max_turns(mut self, max_turns: u64) -> Self {
        self.max_turns = max_turns;
        self
    }

    pub fn with_model_tiers(mut self, model_tiers: ModelTiers) -> Self {
        self.model_tiers = model_tiers;
        self
    }
}

/// System prompt for Bash agent
const BASH_SYSTEM_PROMPT: &str = r#"You are a Bash command execution specialist. Your role is to help execute shell commands efficiently and safely.

## Your Capabilities
- Execute shell commands using execute_command
- Read files to understand context
- Write files when needed
- List directories to explore the filesystem

## Guidelines
- Be careful with destructive commands (rm, mv with overwrite, etc.)
- Prefer read-only operations when exploring
- Chain commands efficiently with && or ||
- Always verify important operations
- Report command output clearly and concisely

## Response Format
When you complete a task:
1. Summarize what commands were executed
2. Report the outcome
3. Note any errors or warnings"#;

/// System prompt for Explore agent
const EXPLORE_SYSTEM_PROMPT: &str = r#"You are a fast codebase exploration specialist. Your role is to quickly find and analyze code patterns, files, and structures.

## Your Capabilities
- Search for files using glob patterns (glob)
- Search file contents using regex (grep)
- Read files to examine code
- List directories to understand structure
- Use LSP for code intelligence (definitions, references, hover info)
- Search files by name or content (search_files)

## Guidelines
- You are READ-ONLY - do not modify any files
- Use efficient search patterns
- Start broad, then narrow down
- Combine multiple tools for thorough analysis
- Report findings clearly with file paths and line numbers

## Response Format
Provide a clear summary of your findings including:
- Relevant file paths
- Code snippets when helpful
- Patterns and structures discovered"#;

/// System prompt for Plan agent
const PLAN_SYSTEM_PROMPT: &str = r#"You are a software architect and implementation planner. Your role is to explore codebases and design implementation plans.

## Your Capabilities
- All exploration tools (glob, grep, read_file, list_directory, search_files, lsp)
- Task tracking with todo_write

## Guidelines
- Thoroughly understand existing code before planning
- Consider edge cases and error handling
- Identify dependencies and potential conflicts
- Create clear, actionable implementation steps
- Note any architectural concerns

## Response Format
Provide a structured implementation plan:
1. Current State Analysis
2. Proposed Changes
3. Step-by-Step Implementation
4. Testing Strategy
5. Potential Risks"#;

/// System prompt for GeneralPurpose agent
const GENERAL_PURPOSE_SYSTEM_PROMPT: &str = r#"You are a general-purpose AI coding assistant with full capabilities. You can research, modify code, and execute commands.

## Your Capabilities
- File operations: read, write, edit, delete, move
- Code search: glob, grep, search_files
- Shell execution: execute_command
- Web access: web_fetch, web_search
- Code intelligence: lsp
- Task tracking: todo_write

## Guidelines
- Understand before modifying - read files first
- Use the edit tool for surgical changes (preferred over write_file)
- Verify changes after making them
- Test when possible
- Keep the user informed of progress

## Response Format
1. Explain what you're going to do
2. Execute the necessary operations
3. Summarize the results
4. Note any issues or follow-up needed"#;

/// Get the system prompt for an agent type
pub fn get_system_prompt(agent_type: &AgentType) -> &'static str {
    match agent_type {
        AgentType::Bash => BASH_SYSTEM_PROMPT,
        AgentType::Explore => EXPLORE_SYSTEM_PROMPT,
        AgentType::Plan => PLAN_SYSTEM_PROMPT,
        AgentType::GeneralPurpose => GENERAL_PURPOSE_SYSTEM_PROMPT,
    }
}

/// Get the model string for a model tier using config-driven tiers
///
/// This function uses the ModelTiers configuration to select the appropriate
/// model for the given tier, allowing provider-specific customization.
pub fn get_model_for_tier(tier: &ModelTier, model_tiers: &ModelTiers) -> String {
    match tier {
        ModelTier::Fast => model_tiers.fast.clone(),
        ModelTier::Balanced => model_tiers.balanced.clone(),
        ModelTier::Powerful => model_tiers.powerful.clone(),
    }
}

/// Create a tool registry for a specific agent type
pub fn create_agent_tool_registry(agent_type: &AgentType, workspace: &Path) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let workspace = workspace.to_path_buf();

    match agent_type {
        AgentType::Bash => {
            // Bash agent: execute_command, read_file, write_file, list_directory
            registry.register(Arc::new(ExecuteCommand::new(workspace.clone())));
            registry.register(Arc::new(ReadFile::new(workspace.clone())));
            registry.register(Arc::new(WriteFile::new(workspace.clone())));
            registry.register(Arc::new(ListDirectory::new(workspace.clone())));
        }
        AgentType::Explore => {
            // Explore agent: read-only tools
            registry.register(Arc::new(ReadFile::new(workspace.clone())));
            registry.register(Arc::new(GlobFiles::new(workspace.clone())));
            registry.register(Arc::new(GrepFiles::new(workspace.clone())));
            registry.register(Arc::new(ListDirectory::new(workspace.clone())));
            registry.register(Arc::new(SearchFiles::new(workspace.clone())));
            registry.register(Arc::new(LspTool::new(workspace.clone())));
        }
        AgentType::Plan => {
            // Plan agent: same as Explore + todo_write
            registry.register(Arc::new(ReadFile::new(workspace.clone())));
            registry.register(Arc::new(GlobFiles::new(workspace.clone())));
            registry.register(Arc::new(GrepFiles::new(workspace.clone())));
            registry.register(Arc::new(ListDirectory::new(workspace.clone())));
            registry.register(Arc::new(SearchFiles::new(workspace.clone())));
            registry.register(Arc::new(LspTool::new(workspace.clone())));
            registry.register(Arc::new(TodoWrite::new()));
        }
        AgentType::GeneralPurpose => {
            // GeneralPurpose agent: all tools except nested TaskTool
            registry.register(Arc::new(ReadFile::new(workspace.clone())));
            registry.register(Arc::new(WriteFile::new(workspace.clone())));
            registry.register(Arc::new(EditFile::new(workspace.clone())));
            registry.register(Arc::new(GlobFiles::new(workspace.clone())));
            registry.register(Arc::new(GrepFiles::new(workspace.clone())));
            registry.register(Arc::new(ListDirectory::new(workspace.clone())));
            registry.register(Arc::new(SearchFiles::new(workspace.clone())));
            registry.register(Arc::new(ExecuteCommand::new(workspace.clone())));
            registry.register(Arc::new(WebFetch::new()));
            registry.register(Arc::new(WebSearch::new()));
            registry.register(Arc::new(LspTool::new(workspace.clone())));
            registry.register(Arc::new(TodoWrite::new()));
        }
    }

    registry
}

/// Execute the main agentic loop for a subagent
///
/// This runs the agent until it completes (returns a message without tool calls)
/// or reaches the maximum number of turns.
pub async fn execute_agent_loop(
    agent_type: &AgentType,
    model: &ModelTier,
    prompt: &str,
    config: &AgentExecutionConfig,
    registry: Arc<AgentInstanceRegistry>,
    agent_id: &str,
) -> Result<String> {
    // Create provider with appropriate model (config-driven)
    let model_str = get_model_for_tier(model, &config.model_tiers);
    let system_prompt = get_system_prompt(agent_type);

    let provider = create_provider(
        config.provider_type,
        config.api_key.as_deref(),
        Some(&model_str),
        Some(system_prompt),
    )?;

    // Create tool registry for this agent type
    let tool_registry = create_agent_tool_registry(agent_type, &config.workspace);
    let tool_definitions = tool_registry.list();

    // Initialize conversation with the prompt
    let mut messages: Vec<LlmMessage> = vec![LlmMessage::user(prompt)];

    let mut turns = 0u64;
    let final_result: String;

    // Agentic loop
    loop {
        if turns >= config.max_turns {
            final_result = format!(
                "Agent reached maximum turns limit ({}). Last state: partial completion.",
                config.max_turns
            );
            break;
        }

        turns += 1;

        // Call the provider
        let result = provider
            .chat(messages.clone(), Some(tool_definitions.clone()))
            .await;

        match result {
            Ok(CompletionResult::Message(text)) => {
                // Agent completed with a final message
                final_result = text;
                break;
            }
            Ok(CompletionResult::ToolCalls(calls)) => {
                // Execute tool calls and continue
                let mut tool_results = Vec::new();

                for call in &calls {
                    let tool_result = if let Some(tool) = tool_registry.get(&call.name) {
                        match tool.execute(call.arguments.clone()).await {
                            Ok(output) => {
                                if output.success {
                                    output.content.to_string()
                                } else {
                                    format!(
                                        "Tool error: {}",
                                        output.error.unwrap_or_else(|| "Unknown error".to_string())
                                    )
                                }
                            }
                            Err(e) => format!("Tool execution failed: {}", e),
                        }
                    } else {
                        format!("Unknown tool: {}", call.name)
                    };

                    tool_results.push((call.name.clone(), tool_result));
                }

                // Format tool results as a message for the conversation
                let results_summary: Vec<String> = tool_results
                    .iter()
                    .map(|(name, result)| {
                        format!("[Tool '{}' result]\n{}", name, result)
                    })
                    .collect();

                messages.push(LlmMessage::user(format!(
                    "Tool execution results:\n\n{}\n\nContinue with the task.",
                    results_summary.join("\n\n")
                )));
            }
            Err(e) => {
                final_result = format!("Agent error: {}", e);
                registry
                    .update_status(agent_id, AgentStatus::Failed, Some(final_result.clone()))
                    .await;
                return Err(e);
            }
        }
    }

    // Update registry with completed status
    registry
        .update_status(agent_id, AgentStatus::Completed, Some(final_result.clone()))
        .await;

    Ok(final_result)
}

/// Execute an agent in the background
///
/// Spawns the agent loop as a tokio task and writes output to a file.
pub fn execute_agent_background(
    agent_type: AgentType,
    model: ModelTier,
    prompt: String,
    config: AgentExecutionConfig,
    registry: Arc<AgentInstanceRegistry>,
    agent_id: String,
    output_file: String,
) {
    tokio::spawn(async move {
        // Open output file for writing progress
        let mut file = match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&output_file)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to open output file {}: {}", output_file, e);
                registry
                    .update_status(
                        &agent_id,
                        AgentStatus::Failed,
                        Some(format!("Failed to open output file: {}", e)),
                    )
                    .await;
                return;
            }
        };

        // Write header
        let header = format!(
            "=== Agent Execution Log ===\n\
             Agent ID: {}\n\
             Type: {}\n\
             Started: {}\n\
             Prompt: {}\n\
             ===========================\n\n",
            agent_id,
            agent_type,
            chrono::Utc::now(),
            prompt
        );
        let _ = file.write_all(header.as_bytes()).await;

        // Execute the agent loop
        let result = execute_agent_loop(
            &agent_type,
            &model,
            &prompt,
            &config,
            registry.clone(),
            &agent_id,
        )
        .await;

        // Write result
        let result_text = match &result {
            Ok(output) => format!("\n=== Completed ===\n{}\n", output),
            Err(e) => format!("\n=== Failed ===\nError: {}\n", e),
        };
        let _ = file.write_all(result_text.as_bytes()).await;

        // Status already updated by execute_agent_loop
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_prompt() {
        let bash_prompt = get_system_prompt(&AgentType::Bash);
        assert!(bash_prompt.contains("Bash"));
        assert!(bash_prompt.contains("execute_command"));

        let explore_prompt = get_system_prompt(&AgentType::Explore);
        assert!(explore_prompt.contains("exploration"));
        assert!(explore_prompt.contains("READ-ONLY"));

        let plan_prompt = get_system_prompt(&AgentType::Plan);
        assert!(plan_prompt.contains("architect"));
        assert!(plan_prompt.contains("todo_write"));

        let gp_prompt = get_system_prompt(&AgentType::GeneralPurpose);
        assert!(gp_prompt.contains("general-purpose"));
    }

    #[test]
    fn test_get_model_for_tier() {
        // Test with Anthropic tiers
        let anthropic_tiers = ModelTiers::anthropic();
        assert_eq!(
            get_model_for_tier(&ModelTier::Balanced, &anthropic_tiers),
            "claude-opus-4-20250514"
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Powerful, &anthropic_tiers),
            "claude-opus-4-20250514"
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Fast, &anthropic_tiers),
            "claude-3-5-haiku-20241022"
        );

        // Test with OpenAI tiers
        let openai_tiers = ModelTiers::openai();
        assert_eq!(
            get_model_for_tier(&ModelTier::Balanced, &openai_tiers),
            "gpt-5"
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Fast, &openai_tiers),
            "gpt-4o-mini"
        );

        // Test with DeepSeek tiers
        let deepseek_tiers = ModelTiers::deepseek();
        assert_eq!(
            get_model_for_tier(&ModelTier::Fast, &deepseek_tiers),
            "deepseek-chat"
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Powerful, &deepseek_tiers),
            "deepseek-reasoner"
        );
    }

    #[test]
    fn test_create_agent_tool_registry() {
        let workspace = PathBuf::from("/tmp/test");

        // Bash agent tools
        let bash_registry = create_agent_tool_registry(&AgentType::Bash, &workspace);
        assert!(bash_registry.get("execute_command").is_some());
        assert!(bash_registry.get("read_file").is_some());
        assert!(bash_registry.get("write_file").is_some());
        assert!(bash_registry.get("list_directory").is_some());
        assert!(bash_registry.get("glob").is_none()); // Bash doesn't have glob

        // Explore agent tools (read-only)
        let explore_registry = create_agent_tool_registry(&AgentType::Explore, &workspace);
        assert!(explore_registry.get("read_file").is_some());
        assert!(explore_registry.get("glob").is_some());
        assert!(explore_registry.get("grep").is_some());
        assert!(explore_registry.get("lsp").is_some());
        assert!(explore_registry.get("write_file").is_none()); // Explore can't write
        assert!(explore_registry.get("execute_command").is_none()); // Explore can't execute

        // Plan agent tools
        let plan_registry = create_agent_tool_registry(&AgentType::Plan, &workspace);
        assert!(plan_registry.get("read_file").is_some());
        assert!(plan_registry.get("glob").is_some());
        assert!(plan_registry.get("todo_write").is_some());
        assert!(plan_registry.get("write_file").is_none()); // Plan can't write

        // GeneralPurpose agent tools
        let gp_registry = create_agent_tool_registry(&AgentType::GeneralPurpose, &workspace);
        assert!(gp_registry.get("read_file").is_some());
        assert!(gp_registry.get("write_file").is_some());
        assert!(gp_registry.get("edit").is_some());
        assert!(gp_registry.get("execute_command").is_some());
        assert!(gp_registry.get("web_fetch").is_some());
        assert!(gp_registry.get("task").is_none()); // No recursive task tool
    }

    #[test]
    fn test_agent_execution_config() {
        let config = AgentExecutionConfig::new(PathBuf::from("/workspace"))
            .with_provider(ProviderType::OpenAI)
            .with_api_key("test-key".to_string())
            .with_max_turns(100);

        assert_eq!(config.workspace, PathBuf::from("/workspace"));
        assert_eq!(config.provider_type, ProviderType::OpenAI);
        assert_eq!(config.api_key, Some("test-key".to_string()));
        assert_eq!(config.max_turns, 100);
    }
}
