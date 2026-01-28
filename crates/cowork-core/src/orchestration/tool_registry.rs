//! Tool Registry Factory Module
//!
//! Shared tool registry creation for both CLI and UI.
//! Centralizes tool registration and provides builder pattern for customization.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::config::{ModelTiers, WebSearchConfig};
use crate::mcp_manager::McpServerManager;
use crate::provider::ProviderType;
use crate::session::{SessionOutput, SessionRegistry};
use crate::tools::filesystem::{EditFile, GlobFiles, GrepFiles, ReadFile, WriteFile};
use crate::tools::interaction::AskUserQuestion;
use crate::tools::lsp::LspTool;
use crate::tools::mcp::create_mcp_tools;
use crate::tools::notebook::NotebookEdit;
use crate::tools::planning::{EnterPlanMode, ExitPlanMode, PlanModeState};
use crate::tools::shell::{ExecuteCommand, KillShell, ShellProcessRegistry};
use crate::tools::skill::SkillTool;
use crate::tools::task::{AgentInstanceRegistry, TaskOutputTool, TaskTool, TodoWrite};
use crate::tools::web::{supports_native_search, WebFetch, WebSearch};
use crate::tools::ToolRegistry;
use crate::skills::SkillRegistry;

/// Defines which subset of tools a subagent should have access to
#[derive(Debug, Clone)]
pub enum ToolScope {
    /// Bash, Read, Write
    Bash,
    /// Read-only exploration: Read, Glob, Grep, LSP
    Explore,
    /// Explore + TodoWrite
    Plan,
    /// Everything except TaskTool and AskUserQuestion
    GeneralPurpose,
}

/// Builder for creating a tool registry with customizable options
pub struct ToolRegistryBuilder {
    workspace: PathBuf,
    provider_type: Option<ProviderType>,
    api_key: Option<String>,
    model_tiers: Option<ModelTiers>,
    web_search_config: Option<WebSearchConfig>,
    include_filesystem: bool,
    include_shell: bool,
    include_web: bool,
    include_notebook: bool,
    include_lsp: bool,
    include_task: bool,
    include_planning: bool,
    include_interaction: bool,
    include_mcp: bool,
    tool_scope: Option<ToolScope>,
    skill_registry: Option<Arc<SkillRegistry>>,
    plan_mode_state: Option<Arc<tokio::sync::RwLock<PlanModeState>>>,
    /// Parent output channel for subagent progress forwarding
    progress_tx: Option<mpsc::Sender<(String, SessionOutput)>>,
    /// Parent session ID for progress forwarding
    progress_session_id: Option<String>,
    /// Shared session registry for subagent approval routing
    session_registry: Option<SessionRegistry>,
    /// MCP server manager for external tool integration
    mcp_manager: Option<Arc<McpServerManager>>,
}

impl ToolRegistryBuilder {
    /// Create a new builder with the given workspace path
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            provider_type: None,
            api_key: None,
            model_tiers: None,
            web_search_config: None,
            include_filesystem: true,
            include_shell: true,
            include_web: true,
            include_notebook: true,
            include_lsp: true,
            include_task: true,
            include_planning: true,
            include_interaction: true,
            include_mcp: true,
            tool_scope: None,
            skill_registry: None,
            plan_mode_state: None,
            progress_tx: None,
            progress_session_id: None,
            session_registry: None,
            mcp_manager: None,
        }
    }

    /// Set the shared session registry for subagent approval routing
    pub fn with_session_registry(mut self, registry: SessionRegistry) -> Self {
        self.session_registry = Some(registry);
        self
    }

    /// Set a shared PlanModeState — used by the agent loop to share state
    /// between the planning tools and the tool filtering logic
    pub fn with_plan_mode_state(mut self, state: Arc<tokio::sync::RwLock<PlanModeState>>) -> Self {
        self.plan_mode_state = Some(state);
        self
    }

    /// Set a tool scope — when set, `build()` will use scoped tool registration
    pub fn with_tool_scope(mut self, scope: ToolScope) -> Self {
        self.tool_scope = Some(scope);
        self
    }

    /// Set the progress channel for subagent activity forwarding
    pub fn with_progress_channel(
        mut self,
        tx: mpsc::Sender<(String, SessionOutput)>,
        session_id: String,
    ) -> Self {
        self.progress_tx = Some(tx);
        self.progress_session_id = Some(session_id);
        self
    }

    /// Set the provider type (required for task tools)
    pub fn with_provider(mut self, provider_type: ProviderType) -> Self {
        self.provider_type = Some(provider_type);
        self
    }

    /// Set the API key (required for task tools)
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Set the model tiers for task tools
    pub fn with_model_tiers(mut self, tiers: ModelTiers) -> Self {
        self.model_tiers = Some(tiers);
        self
    }

    /// Set the web search configuration
    pub fn with_web_search_config(mut self, config: WebSearchConfig) -> Self {
        self.web_search_config = Some(config);
        self
    }

    /// Enable/disable filesystem tools
    pub fn with_filesystem(mut self, enabled: bool) -> Self {
        self.include_filesystem = enabled;
        self
    }

    /// Enable/disable shell tools
    pub fn with_shell(mut self, enabled: bool) -> Self {
        self.include_shell = enabled;
        self
    }

    /// Enable/disable web tools
    pub fn with_web(mut self, enabled: bool) -> Self {
        self.include_web = enabled;
        self
    }

    /// Enable/disable notebook tools
    pub fn with_notebook(mut self, enabled: bool) -> Self {
        self.include_notebook = enabled;
        self
    }

    /// Enable/disable LSP tools
    pub fn with_lsp(mut self, enabled: bool) -> Self {
        self.include_lsp = enabled;
        self
    }

    /// Enable/disable task/agent tools
    pub fn with_task(mut self, enabled: bool) -> Self {
        self.include_task = enabled;
        self
    }

    /// Enable/disable planning tools
    pub fn with_planning(mut self, enabled: bool) -> Self {
        self.include_planning = enabled;
        self
    }

    /// Enable/disable interaction tools (ask_user_question)
    pub fn with_interaction(mut self, enabled: bool) -> Self {
        self.include_interaction = enabled;
        self
    }

    /// Set the skill registry for the Skill tool
    pub fn with_skill_registry(mut self, registry: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(registry);
        self
    }

    /// Enable/disable MCP tools
    pub fn with_mcp(mut self, enabled: bool) -> Self {
        self.include_mcp = enabled;
        self
    }

    /// Set the MCP server manager for external tool integration
    pub fn with_mcp_manager(mut self, manager: Arc<McpServerManager>) -> Self {
        self.mcp_manager = Some(manager);
        self
    }

    /// Build the tool registry with the configured options
    pub fn build(self) -> ToolRegistry {
        if let Some(scope) = self.tool_scope.clone() {
            return self.build_scoped(scope);
        }

        let mut registry = ToolRegistry::new();

        // Filesystem tools
        if self.include_filesystem {
            registry.register(Arc::new(ReadFile::new(self.workspace.clone())));
            registry.register(Arc::new(WriteFile::new(self.workspace.clone())));
            registry.register(Arc::new(EditFile::new(self.workspace.clone())));
            registry.register(Arc::new(GlobFiles::new(self.workspace.clone())));
            registry.register(Arc::new(GrepFiles::new(self.workspace.clone())));
        }

        // Shell tools with shared process registry
        if self.include_shell {
            let shell_registry = Arc::new(ShellProcessRegistry::new());
            registry.register(Arc::new(
                ExecuteCommand::new(self.workspace.clone())
                    .with_registry(shell_registry.clone())
            ));
            registry.register(Arc::new(KillShell::new(shell_registry)));
        }

        // Web tools
        if self.include_web {
            registry.register(Arc::new(WebFetch::new()));

            // Check if provider has built-in web search
            let provider_has_native = self
                .provider_type
                .as_ref()
                .map(|p| supports_native_search(p.as_str()))
                .unwrap_or(false);

            // Only register WebSearch (SerpAPI) if:
            // 1. Provider does NOT have native search, AND
            // 2. SerpAPI is configured
            if !provider_has_native {
                let is_configured = self
                    .web_search_config
                    .as_ref()
                    .map(|c| {
                        let configured = c.is_configured();
                        tracing::debug!(
                            has_api_key = c.api_key.is_some(),
                            is_configured = configured,
                            "WebSearch SerpAPI config check"
                        );
                        configured
                    })
                    .unwrap_or(false);

                if is_configured {
                    tracing::info!("Registering WebSearch tool with SerpAPI");
                    let web_search = if let Some(config) = self.web_search_config.clone() {
                        WebSearch::with_config(config)
                    } else {
                        WebSearch::new()
                    };
                    registry.register(Arc::new(web_search));
                } else {
                    tracing::debug!("WebSearch not registered: SerpAPI not configured");
                }
            } else {
                tracing::debug!("WebSearch not registered: provider has native search");
            }
        }

        // Notebook tools
        if self.include_notebook {
            registry.register(Arc::new(NotebookEdit::new(self.workspace.clone())));
        }

        // Task management tools (TodoWrite is always available)
        registry.register(Arc::new(TodoWrite::new()));

        // Code intelligence tools
        if self.include_lsp {
            registry.register(Arc::new(LspTool::new(self.workspace.clone())));
        }

        // Interaction tools
        if self.include_interaction {
            registry.register(Arc::new(AskUserQuestion::new()));
        }

        // Planning tools with shared state
        if self.include_planning {
            let plan_mode_state = self.plan_mode_state.clone().unwrap_or_else(||
                Arc::new(tokio::sync::RwLock::new(PlanModeState::default()))
            );
            registry.register(Arc::new(EnterPlanMode::new(plan_mode_state.clone())));
            registry.register(Arc::new(ExitPlanMode::new(plan_mode_state)));
        }

        // Agent/Task tools - require provider_type for full functionality
        if self.include_task
            && let Some(provider_type) = self.provider_type {
                let agent_registry = Arc::new(AgentInstanceRegistry::new());
                let mut task_tool =
                    TaskTool::new(agent_registry.clone(), self.workspace.clone())
                        .with_provider(provider_type);

                if let Some(key) = self.api_key {
                    task_tool = task_tool.with_api_key(key);
                }
                if let Some(tiers) = self.model_tiers {
                    task_tool = task_tool.with_model_tiers(tiers);
                }
                if let (Some(tx), Some(sid)) = (self.progress_tx, self.progress_session_id) {
                    task_tool = task_tool.with_progress_channel(tx, sid);
                }
                if let Some(reg) = self.session_registry {
                    task_tool = task_tool.with_session_registry(reg);
                }

                registry.register(Arc::new(task_tool));
                registry.register(Arc::new(TaskOutputTool::new(agent_registry)));
            }

        // Skill tool - when a skill registry is provided
        if let Some(skill_registry) = self.skill_registry {
            registry.register(Arc::new(SkillTool::new(skill_registry, self.workspace.clone())));
        }

        // MCP tools - when an MCP manager is provided
        if self.include_mcp
            && let Some(ref mcp_manager) = self.mcp_manager {
                let mcp_tools = create_mcp_tools(mcp_manager.clone());
                for tool in mcp_tools {
                    registry.register(tool);
                }
                tracing::info!(
                    tool_count = registry.list().len(),
                    "Registered MCP tools from server manager"
                );
            }

        registry
    }

    /// Build a scoped tool registry for subagents
    ///
    /// Registers only the tools appropriate for the given scope,
    /// replacing `create_agent_tool_registry()` in executor.rs.
    fn build_scoped(self, scope: ToolScope) -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        let workspace = self.workspace;

        match scope {
            ToolScope::Bash => {
                let shell_registry = Arc::new(ShellProcessRegistry::new());
                registry.register(Arc::new(
                    ExecuteCommand::new(workspace).with_registry(shell_registry),
                ));
            }
            ToolScope::Explore => {
                // CC's Explore has all tools except Task, ExitPlanMode, Edit, Write, NotebookEdit
                registry.register(Arc::new(ReadFile::new(workspace.clone())));
                registry.register(Arc::new(GlobFiles::new(workspace.clone())));
                registry.register(Arc::new(GrepFiles::new(workspace.clone())));
                let shell_registry = Arc::new(ShellProcessRegistry::new());
                registry.register(Arc::new(
                    ExecuteCommand::new(workspace.clone()).with_registry(shell_registry),
                ));
                registry.register(Arc::new(WebFetch::new()));
                // Include WebSearch if SerpAPI is configured
                if let Some(config) = self.web_search_config.as_ref()
                    && config.is_configured() {
                        registry.register(Arc::new(WebSearch::with_config(config.clone())));
                    }
                registry.register(Arc::new(LspTool::new(workspace)));
                registry.register(Arc::new(TodoWrite::new()));
            }
            ToolScope::Plan => {
                // CC's Plan has all tools except Task, ExitPlanMode, Edit, Write, NotebookEdit
                registry.register(Arc::new(ReadFile::new(workspace.clone())));
                registry.register(Arc::new(GlobFiles::new(workspace.clone())));
                registry.register(Arc::new(GrepFiles::new(workspace.clone())));
                let shell_registry = Arc::new(ShellProcessRegistry::new());
                registry.register(Arc::new(
                    ExecuteCommand::new(workspace.clone()).with_registry(shell_registry),
                ));
                registry.register(Arc::new(WebFetch::new()));
                // Include WebSearch if SerpAPI is configured
                if let Some(config) = self.web_search_config.as_ref()
                    && config.is_configured() {
                        registry.register(Arc::new(WebSearch::with_config(config.clone())));
                    }
                registry.register(Arc::new(LspTool::new(workspace)));
                registry.register(Arc::new(TodoWrite::new()));
            }
            ToolScope::GeneralPurpose => {
                registry.register(Arc::new(ReadFile::new(workspace.clone())));
                registry.register(Arc::new(WriteFile::new(workspace.clone())));
                registry.register(Arc::new(EditFile::new(workspace.clone())));
                registry.register(Arc::new(GlobFiles::new(workspace.clone())));
                registry.register(Arc::new(GrepFiles::new(workspace.clone())));
                let shell_registry = Arc::new(ShellProcessRegistry::new());
                registry.register(Arc::new(
                    ExecuteCommand::new(workspace.clone()).with_registry(shell_registry),
                ));
                registry.register(Arc::new(WebFetch::new()));
                // Include WebSearch if SerpAPI is configured
                if let Some(config) = self.web_search_config.as_ref()
                    && config.is_configured() {
                        registry.register(Arc::new(WebSearch::with_config(config.clone())));
                    }
                registry.register(Arc::new(LspTool::new(workspace)));
                registry.register(Arc::new(TodoWrite::new()));
            }
        }

        registry
    }
}

/// Convenience function to create a standard tool registry with all tools enabled
///
/// This is equivalent to:
/// ```ignore
/// ToolRegistryBuilder::new(workspace.to_path_buf())
///     .with_provider(provider_type)
///     .with_api_key(api_key.unwrap_or_default())
///     .with_model_tiers(model_tiers.unwrap_or_default())
///     .with_web_search_config(web_search_config.unwrap_or_default())
///     .build()
/// ```
pub fn create_standard_tool_registry(
    workspace: &Path,
    provider_type: ProviderType,
    api_key: Option<&str>,
    model_tiers: Option<ModelTiers>,
    web_search_config: Option<WebSearchConfig>,
) -> ToolRegistry {
    let mut builder = ToolRegistryBuilder::new(workspace.to_path_buf())
        .with_provider(provider_type);

    if let Some(key) = api_key {
        builder = builder.with_api_key(key.to_string());
    }
    if let Some(tiers) = model_tiers {
        builder = builder.with_model_tiers(tiers);
    }
    if let Some(ws_config) = web_search_config {
        builder = builder.with_web_search_config(ws_config);
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_builder_creates_registry() {
        let temp_dir = tempdir().unwrap();
        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf()).build();

        // Should have filesystem tools (PascalCase names)
        assert!(registry.get("Read").is_some());
        assert!(registry.get("Write").is_some());
        assert!(registry.get("Edit").is_some());
        assert!(registry.get("Glob").is_some());
        assert!(registry.get("Grep").is_some());

        // Should have shell tools
        assert!(registry.get("Bash").is_some());
        assert!(registry.get("KillShell").is_some());

        // Should have web tools
        assert!(registry.get("WebFetch").is_some());
        // WebSearch requires fallback config when provider doesn't have native search
        // Without config or native provider, it won't be registered
        assert!(registry.get("WebSearch").is_none());

        // Should have planning tools
        assert!(registry.get("EnterPlanMode").is_some());
        assert!(registry.get("ExitPlanMode").is_some());

        // Should have other tools
        assert!(registry.get("TodoWrite").is_some());
        assert!(registry.get("AskUserQuestion").is_some());
        assert!(registry.get("NotebookEdit").is_some());
        assert!(registry.get("LSP").is_some());
    }

    #[test]
    fn test_websearch_with_configured_api_key() {
        let temp_dir = tempdir().unwrap();

        // Create a config with SerpAPI key set directly
        let mut ws_config = WebSearchConfig::default();
        ws_config.api_key = Some("test-api-key".to_string());

        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf())
            .with_web_search_config(ws_config)
            .build();

        // WebSearch should be registered when API key is configured
        assert!(registry.get("WebSearch").is_some());
    }

    #[test]
    fn test_websearch_with_native_provider() {
        let temp_dir = tempdir().unwrap();

        // Anthropic has native web search, so WebSearch fallback shouldn't be registered
        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf())
            .with_provider(ProviderType::Anthropic)
            .build();

        // WebSearch fallback should NOT be registered for native providers
        assert!(registry.get("WebSearch").is_none());
    }

    #[test]
    fn test_builder_can_disable_tools() {
        let temp_dir = tempdir().unwrap();
        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf())
            .with_filesystem(false)
            .with_shell(false)
            .build();

        // Filesystem tools should be missing
        assert!(registry.get("Read").is_none());
        assert!(registry.get("Write").is_none());

        // Shell tools should be missing
        assert!(registry.get("Bash").is_none());
        assert!(registry.get("KillShell").is_none());

        // But web tools should still be there
        assert!(registry.get("WebFetch").is_some());
    }

    #[test]
    fn test_create_standard_tool_registry() {
        let temp_dir = tempdir().unwrap();
        let registry = create_standard_tool_registry(
            temp_dir.path(),
            ProviderType::Anthropic,
            Some("test-key"),
            Some(ModelTiers::for_provider("anthropic")),
            None, // web_search_config
        );

        // Should have task tools since provider was specified (PascalCase names)
        assert!(registry.get("Task").is_some());
        assert!(registry.get("TaskOutput").is_some());
    }

    #[test]
    fn test_registry_without_provider_has_no_task_tools() {
        let temp_dir = tempdir().unwrap();
        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf()).build();

        // No task tools without provider
        assert!(registry.get("Task").is_none());
        assert!(registry.get("TaskOutput").is_none());
    }
}
