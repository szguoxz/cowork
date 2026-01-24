//! Agent execution engine for TaskTool subagents
//!
//! Subagents reuse the shared `AgentLoop` from `crate::session`, configured with
//! scoped tools (via `ToolScope`), trust-all approval, no hooks, and no persistence.
//! This gives subagents automatic tool result truncation, context monitoring,
//! and auto-compaction for free.
//!
//! Dynamic agents from the prompt system take precedence when a matching name
//! is found in the component registry.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::approval::ToolApprovalConfig;
use crate::config::ModelTiers;
use crate::error::Result;
use crate::orchestration::ToolScope;
use crate::prompt::{
    builtin, parse_frontmatter, AgentDefinition, ComponentRegistry, ModelPreference,
};
use crate::provider::ProviderType;
use crate::session::{AgentLoop, SessionConfig, SessionInput, SessionOutput};

/// Maximum result size for subagent output (to prevent context bloat)
/// Results exceeding this will be truncated with a note
const MAX_RESULT_SIZE: usize = 10000;

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
    /// Optional component registry for dynamic agent loading
    pub registry: Option<Arc<ComponentRegistry>>,
}

impl AgentExecutionConfig {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            provider_type: ProviderType::Anthropic,
            api_key: None,
            max_turns: 50,
            model_tiers: ModelTiers::anthropic(),
            registry: None,
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

    pub fn with_registry(mut self, registry: Arc<ComponentRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }
}

/// Get the embedded agent prompt content for a given agent type.
///
/// Parses the built-in `.md` file (stripping YAML frontmatter) to extract
/// the system prompt body. This is the single source of truth for agent prompts.
fn get_builtin_prompt(agent_type: &AgentType) -> String {
    let source = match agent_type {
        AgentType::Bash => builtin::agents::BASH,
        AgentType::Explore => builtin::agents::EXPLORE,
        AgentType::Plan => builtin::agents::PLAN,
        AgentType::GeneralPurpose => builtin::agents::GENERAL,
    };

    parse_frontmatter(source)
        .map(|doc| doc.content)
        .unwrap_or_else(|_| source.to_string())
}

/// Try to get an agent definition from the registry by name
///
/// This allows dynamic agent loading from `.claude/agents/` directories.
/// Returns None if no matching agent is found in the registry.
pub fn get_agent_from_registry<'a>(
    name: &str,
    registry: Option<&'a ComponentRegistry>,
) -> Option<&'a AgentDefinition> {
    registry?.get_agent(name)
}

/// Get the system prompt for an agent, checking registry first
///
/// This function supports both legacy hardcoded agents and dynamic agents
/// loaded from the prompt system. Dynamic agents take precedence.
pub fn get_system_prompt_dynamic(
    agent_type: &AgentType,
    registry: Option<&ComponentRegistry>,
) -> String {
    // First, try to find the agent in the registry by its display name
    let agent_name = match agent_type {
        AgentType::Bash => "Bash",
        AgentType::Explore => "Explore",
        AgentType::Plan => "Plan",
        AgentType::GeneralPurpose => "general-purpose",
    };

    if let Some(agent_def) = get_agent_from_registry(agent_name, registry) {
        return agent_def.system_prompt.clone();
    }

    // Fall back to parsing the embedded .md files directly
    get_builtin_prompt(agent_type)
}

/// Get the model preference for an agent from the registry
///
/// Returns the agent's configured model preference, or Inherit if not found.
pub fn get_agent_model_preference(
    agent_type: &AgentType,
    registry: Option<&ComponentRegistry>,
) -> ModelPreference {
    let agent_name = match agent_type {
        AgentType::Bash => "Bash",
        AgentType::Explore => "Explore",
        AgentType::Plan => "Plan",
        AgentType::GeneralPurpose => "general-purpose",
    };

    if let Some(agent_def) = get_agent_from_registry(agent_name, registry) {
        return agent_def.metadata.model.clone();
    }

    // Default to inherit for legacy agents
    ModelPreference::Inherit
}

/// Truncate a result string if it exceeds the maximum size
///
/// This prevents subagent results from bloating the main conversation context.
/// When truncated, a note is appended indicating the original size.
fn truncate_result(result: &str, max_size: usize) -> String {
    if result.len() <= max_size {
        return result.to_string();
    }

    // Find a safe truncation point (avoid cutting mid-character)
    let truncate_at = result
        .char_indices()
        .take_while(|(i, _)| *i < max_size)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max_size);

    format!(
        "{}...\n\n[Result truncated - {} chars total]",
        &result[..truncate_at],
        result.len()
    )
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

/// Build environment info to append to system prompts
///
/// This includes platform, working directory, and other context the agent needs.
fn build_environment_info(workspace: &Path) -> String {
    let platform = std::env::consts::OS;
    let os_version = get_os_version();
    let is_git_repo = workspace.join(".git").exists();

    format!(
        r#"

## Environment Information
Working directory: {}
Platform: {}
OS Version: {}
Is git repo: {}"#,
        workspace.display(),
        platform,
        os_version,
        if is_git_repo { "Yes" } else { "No" }
    )
}

/// Get OS version (cross-platform)
fn get_os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("Linux {}", s.trim()))
            .unwrap_or_else(|| "Linux".to_string())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("macOS {}", s.trim()))
            .unwrap_or_else(|| "macOS".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Windows".to_string())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        std::env::consts::OS.to_string()
    }
}

/// Map an AgentType to the corresponding ToolScope
fn tool_scope_for(agent_type: &AgentType) -> ToolScope {
    match agent_type {
        AgentType::Bash => ToolScope::Bash,
        AgentType::Explore => ToolScope::Explore,
        AgentType::Plan => ToolScope::Plan,
        AgentType::GeneralPurpose => ToolScope::GeneralPurpose,
    }
}

/// Run a subagent using the shared AgentLoop infrastructure
///
/// This replaces the hand-rolled loop with the same AgentLoop used by the main session,
/// giving subagents automatic tool result truncation, context monitoring, and auto-compaction.
pub async fn run_subagent(
    agent_type: &AgentType,
    model: &ModelTier,
    prompt: &str,
    config: &AgentExecutionConfig,
    registry: Arc<AgentInstanceRegistry>,
    agent_id: &str,
) -> Result<String> {
    let model_str = get_model_for_tier(model, &config.model_tiers);

    // Get system prompt (registry-aware) + environment info
    let base_prompt = get_system_prompt_dynamic(
        agent_type,
        config.registry.as_ref().map(|r| r.as_ref()),
    );
    let env_info = build_environment_info(&config.workspace);
    let system_prompt = format!("{}{}", base_prompt, env_info);

    // Build SessionConfig: trust-all approval, scoped tools, no hooks, no save
    let session_config = SessionConfig::new(config.workspace.clone())
        .with_provider(config.provider_type)
        .with_model(model_str)
        .with_system_prompt(system_prompt)
        .with_approval_config(ToolApprovalConfig::trust_all())
        .with_tool_scope(tool_scope_for(agent_type))
        .with_enable_hooks(false)
        .with_save_session(false);

    let session_config = if let Some(ref key) = config.api_key {
        session_config.with_api_key(key.clone())
    } else {
        session_config
    };

    // Create channels
    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<SessionInput>(32);
    let (output_tx, mut output_rx) =
        tokio::sync::mpsc::channel::<(String, SessionOutput)>(128);

    // Create and spawn agent loop
    let agent_loop = AgentLoop::new(
        agent_id.to_string(),
        input_rx,
        output_tx,
        session_config,
    )
    .await
    .map_err(|e| crate::error::Error::Agent(format!("Failed to create subagent loop: {}", e)))?;

    tokio::spawn(agent_loop.run());

    // Send the prompt
    input_tx
        .send(SessionInput::user_message(prompt))
        .await
        .map_err(|e| crate::error::Error::Agent(format!("Failed to send prompt: {}", e)))?;

    // Collect output until Idle
    let mut last_content = String::new();
    while let Some((_sid, output)) = output_rx.recv().await {
        match output {
            SessionOutput::Idle => break,
            SessionOutput::AssistantMessage { content, .. } => {
                last_content = content;
            }
            SessionOutput::Error { message } => {
                info!("Subagent error: {}", message);
            }
            _ => {} // Ignore ToolStart, ToolDone, Thinking, etc.
        }
    }

    // Drop input_tx to signal shutdown
    drop(input_tx);

    // Truncate and update registry
    let truncated = truncate_result(&last_content, MAX_RESULT_SIZE);
    registry
        .update_status(agent_id, AgentStatus::Completed, Some(truncated.clone()))
        .await;

    Ok(truncated)
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

        // Execute the agent loop using the shared AgentLoop
        let result = run_subagent(
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

        // Status already updated by run_subagent
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_result_small() {
        let small = "Hello, world!";
        assert_eq!(truncate_result(small, 100), small);
    }

    #[test]
    fn test_truncate_result_exact() {
        let exact = "Hello";
        assert_eq!(truncate_result(exact, 5), exact);
    }

    #[test]
    fn test_truncate_result_large() {
        let large = "A".repeat(1000);
        let result = truncate_result(&large, 100);
        assert!(result.len() < 200); // Truncated plus message
        assert!(result.contains("..."));
        assert!(result.contains("[Result truncated - 1000 chars total]"));
    }

    #[test]
    fn test_truncate_result_unicode() {
        // Test with multi-byte characters
        let unicode = "こんにちは世界"; // Hello world in Japanese
        let result = truncate_result(unicode, 10);
        // Should truncate safely without cutting mid-character
        assert!(result.contains("..."));
        // Verify the truncated part is valid UTF-8 (this will panic if not)
        let _ = result.as_str();
    }

    #[test]
    fn test_get_builtin_prompt() {
        let bash_prompt = get_builtin_prompt(&AgentType::Bash);
        assert!(bash_prompt.contains("Bash"));

        let explore_prompt = get_builtin_prompt(&AgentType::Explore);
        assert!(explore_prompt.contains("exploration"));
        assert!(explore_prompt.contains("READ-ONLY"));

        let plan_prompt = get_builtin_prompt(&AgentType::Plan);
        assert!(plan_prompt.contains("architect"));
        assert!(plan_prompt.contains("implementation"));

        let gp_prompt = get_builtin_prompt(&AgentType::GeneralPurpose);
        assert!(gp_prompt.contains("general-purpose"));
    }

    #[test]
    fn test_get_model_for_tier() {
        use crate::provider::model_catalog;

        // Test with Anthropic tiers
        let anthropic_tiers = ModelTiers::anthropic();
        assert_eq!(
            get_model_for_tier(&ModelTier::Balanced, &anthropic_tiers),
            model_catalog::ANTHROPIC_BALANCED.0
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Powerful, &anthropic_tiers),
            model_catalog::ANTHROPIC_POWERFUL.0
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Fast, &anthropic_tiers),
            model_catalog::ANTHROPIC_FAST.0
        );

        // Test with OpenAI tiers
        let openai_tiers = ModelTiers::openai();
        assert_eq!(
            get_model_for_tier(&ModelTier::Balanced, &openai_tiers),
            model_catalog::OPENAI_BALANCED.0
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Fast, &openai_tiers),
            model_catalog::OPENAI_FAST.0
        );

        // Test with DeepSeek tiers
        let deepseek_tiers = ModelTiers::deepseek();
        assert_eq!(
            get_model_for_tier(&ModelTier::Fast, &deepseek_tiers),
            model_catalog::DEEPSEEK_FAST.0
        );
        assert_eq!(
            get_model_for_tier(&ModelTier::Powerful, &deepseek_tiers),
            model_catalog::DEEPSEEK_POWERFUL.0
        );
    }

    #[test]
    fn test_tool_scope_via_builder() {
        use crate::orchestration::ToolRegistryBuilder;

        let workspace = PathBuf::from("/tmp/test");

        // Bash scope (only Bash)
        let bash_registry = ToolRegistryBuilder::new(workspace.clone())
            .with_tool_scope(ToolScope::Bash)
            .build();
        assert!(bash_registry.get("Bash").is_some());
        assert!(bash_registry.get("Read").is_none());
        assert!(bash_registry.get("Write").is_none());
        assert!(bash_registry.get("Glob").is_none());

        // Explore scope (all tools except Task, Edit, Write)
        let explore_registry = ToolRegistryBuilder::new(workspace.clone())
            .with_tool_scope(ToolScope::Explore)
            .build();
        assert!(explore_registry.get("Read").is_some());
        assert!(explore_registry.get("Glob").is_some());
        assert!(explore_registry.get("Grep").is_some());
        assert!(explore_registry.get("LSP").is_some());
        assert!(explore_registry.get("Bash").is_some());
        assert!(explore_registry.get("WebFetch").is_some());
        assert!(explore_registry.get("WebSearch").is_some());
        assert!(explore_registry.get("TodoWrite").is_some());
        assert!(explore_registry.get("Write").is_none());
        assert!(explore_registry.get("Edit").is_none());

        // Plan scope (same as Explore: all except Task, Edit, Write)
        let plan_registry = ToolRegistryBuilder::new(workspace.clone())
            .with_tool_scope(ToolScope::Plan)
            .build();
        assert!(plan_registry.get("Read").is_some());
        assert!(plan_registry.get("Glob").is_some());
        assert!(plan_registry.get("Grep").is_some());
        assert!(plan_registry.get("Bash").is_some());
        assert!(plan_registry.get("WebFetch").is_some());
        assert!(plan_registry.get("WebSearch").is_some());
        assert!(plan_registry.get("LSP").is_some());
        assert!(plan_registry.get("TodoWrite").is_some());
        assert!(plan_registry.get("Write").is_none());
        assert!(plan_registry.get("Edit").is_none());

        // GeneralPurpose scope
        let gp_registry = ToolRegistryBuilder::new(workspace.clone())
            .with_tool_scope(ToolScope::GeneralPurpose)
            .build();
        assert!(gp_registry.get("Read").is_some());
        assert!(gp_registry.get("Write").is_some());
        assert!(gp_registry.get("Edit").is_some());
        assert!(gp_registry.get("Bash").is_some());
        assert!(gp_registry.get("WebFetch").is_some());
        assert!(gp_registry.get("Task").is_none());
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
        assert!(config.registry.is_none());
    }

    #[test]
    fn test_agent_execution_config_with_registry() {
        let registry = Arc::new(ComponentRegistry::with_builtins());
        let config = AgentExecutionConfig::new(PathBuf::from("/workspace"))
            .with_registry(registry.clone());

        assert!(config.registry.is_some());
    }

    #[test]
    fn test_get_system_prompt_dynamic_fallback() {
        // Without registry, should fall back to parsing embedded .md files
        let prompt = get_system_prompt_dynamic(&AgentType::Bash, None);
        assert!(prompt.contains("Bash"));

        let prompt = get_system_prompt_dynamic(&AgentType::Explore, None);
        assert!(prompt.contains("exploration"));
    }

    #[test]
    fn test_get_system_prompt_dynamic_with_registry() {
        // With registry, should use registry prompts if available
        let registry = ComponentRegistry::with_builtins();

        // Check that registry has builtin agents
        let prompt = get_system_prompt_dynamic(&AgentType::Explore, Some(&registry));
        // Should get either registry or hardcoded prompt - both valid
        assert!(!prompt.is_empty());
    }

    #[test]
    fn test_get_agent_from_registry() {
        // Without registry, returns None
        assert!(get_agent_from_registry("Explore", None).is_none());

        // With registry
        let registry = ComponentRegistry::with_builtins();
        // Try to find builtin agent
        if registry.get_agent("Explore").is_some() {
            assert!(get_agent_from_registry("Explore", Some(&registry)).is_some());
        }
    }

    #[test]
    fn test_get_agent_model_preference() {
        // Without registry, should return Inherit
        let pref = get_agent_model_preference(&AgentType::Bash, None);
        assert!(matches!(pref, crate::prompt::ModelPreference::Inherit));

        // With registry
        let registry = ComponentRegistry::with_builtins();
        let pref = get_agent_model_preference(&AgentType::Explore, Some(&registry));
        // Should return either the registry's preference or Inherit
        // (depends on whether Explore is in the builtin registry)
        let _ = pref; // Just verify it doesn't panic
    }
}
