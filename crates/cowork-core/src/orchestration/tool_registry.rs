//! Tool Registry Factory Module
//!
//! Shared tool registry creation for both CLI and UI.
//! Centralizes tool registration and provides builder pattern for customization.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::ModelTiers;
use crate::provider::ProviderType;
use crate::tools::browser::BrowserController;
use crate::tools::document::{ReadOfficeDoc, ReadPdf};
use crate::tools::filesystem::{
    DeleteFile, EditFile, GlobFiles, GrepFiles, ListDirectory, MoveFile, ReadFile, SearchFiles,
    WriteFile,
};
use crate::tools::interaction::AskUserQuestion;
use crate::tools::lsp::LspTool;
use crate::tools::notebook::NotebookEdit;
use crate::tools::planning::{EnterPlanMode, ExitPlanMode, PlanModeState};
use crate::tools::shell::ExecuteCommand;
use crate::tools::task::{AgentInstanceRegistry, TaskOutputTool, TaskTool, TodoWrite};
use crate::tools::web::{WebFetch, WebSearch};
use crate::tools::ToolRegistry;

/// Builder for creating a tool registry with customizable options
pub struct ToolRegistryBuilder {
    workspace: PathBuf,
    provider_type: Option<ProviderType>,
    api_key: Option<String>,
    model_tiers: Option<ModelTiers>,
    include_filesystem: bool,
    include_shell: bool,
    include_web: bool,
    include_browser: bool,
    include_notebook: bool,
    include_lsp: bool,
    include_document: bool,
    include_task: bool,
    include_planning: bool,
    include_interaction: bool,
}

impl ToolRegistryBuilder {
    /// Create a new builder with the given workspace path
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            provider_type: None,
            api_key: None,
            model_tiers: None,
            include_filesystem: true,
            include_shell: true,
            include_web: true,
            include_browser: true,
            include_notebook: true,
            include_lsp: true,
            include_document: true,
            include_task: true,
            include_planning: true,
            include_interaction: true,
        }
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

    /// Enable/disable browser tools
    pub fn with_browser(mut self, enabled: bool) -> Self {
        self.include_browser = enabled;
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

    /// Enable/disable document tools
    pub fn with_document(mut self, enabled: bool) -> Self {
        self.include_document = enabled;
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

    /// Build the tool registry with the configured options
    pub fn build(self) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Filesystem tools
        if self.include_filesystem {
            registry.register(Arc::new(ReadFile::new(self.workspace.clone())));
            registry.register(Arc::new(WriteFile::new(self.workspace.clone())));
            registry.register(Arc::new(EditFile::new(self.workspace.clone())));
            registry.register(Arc::new(GlobFiles::new(self.workspace.clone())));
            registry.register(Arc::new(GrepFiles::new(self.workspace.clone())));
            registry.register(Arc::new(ListDirectory::new(self.workspace.clone())));
            registry.register(Arc::new(SearchFiles::new(self.workspace.clone())));
            registry.register(Arc::new(DeleteFile::new(self.workspace.clone())));
            registry.register(Arc::new(MoveFile::new(self.workspace.clone())));
        }

        // Shell tools
        if self.include_shell {
            registry.register(Arc::new(ExecuteCommand::new(self.workspace.clone())));
        }

        // Web tools
        if self.include_web {
            registry.register(Arc::new(WebFetch::new()));
            registry.register(Arc::new(WebSearch::new()));
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

        // Document tools
        if self.include_document {
            registry.register(Arc::new(ReadPdf::new(self.workspace.clone())));
            registry.register(Arc::new(ReadOfficeDoc::new(self.workspace.clone())));
        }

        // Browser tools (headless by default)
        if self.include_browser {
            let browser_controller = BrowserController::default();
            for tool in browser_controller.create_tools() {
                registry.register(tool);
            }
        }

        // Planning tools with shared state
        if self.include_planning {
            let plan_mode_state =
                Arc::new(tokio::sync::RwLock::new(PlanModeState::default()));
            registry.register(Arc::new(EnterPlanMode::new(plan_mode_state.clone())));
            registry.register(Arc::new(ExitPlanMode::new(plan_mode_state)));
        }

        // Agent/Task tools - require provider_type for full functionality
        if self.include_task {
            if let Some(provider_type) = self.provider_type {
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

                registry.register(Arc::new(task_tool));
                registry.register(Arc::new(TaskOutputTool::new(agent_registry)));
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
///     .build()
/// ```
pub fn create_standard_tool_registry(
    workspace: &Path,
    provider_type: ProviderType,
    api_key: Option<&str>,
    model_tiers: Option<ModelTiers>,
) -> ToolRegistry {
    let mut builder = ToolRegistryBuilder::new(workspace.to_path_buf())
        .with_provider(provider_type);

    if let Some(key) = api_key {
        builder = builder.with_api_key(key.to_string());
    }
    if let Some(tiers) = model_tiers {
        builder = builder.with_model_tiers(tiers);
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

        // Should have filesystem tools
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("edit").is_some()); // EditFile has name "edit"
        assert!(registry.get("glob").is_some());
        assert!(registry.get("grep").is_some());

        // Should have shell tools
        assert!(registry.get("execute_command").is_some());

        // Should have web tools
        assert!(registry.get("web_fetch").is_some());
        assert!(registry.get("web_search").is_some());

        // Should have planning tools
        assert!(registry.get("enter_plan_mode").is_some());
        assert!(registry.get("exit_plan_mode").is_some());
    }

    #[test]
    fn test_builder_can_disable_tools() {
        let temp_dir = tempdir().unwrap();
        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf())
            .with_filesystem(false)
            .with_shell(false)
            .build();

        // Filesystem tools should be missing
        assert!(registry.get("read_file").is_none());
        assert!(registry.get("write_file").is_none());

        // Shell tools should be missing
        assert!(registry.get("execute_command").is_none());

        // But web tools should still be there
        assert!(registry.get("web_fetch").is_some());
    }

    #[test]
    fn test_create_standard_tool_registry() {
        let temp_dir = tempdir().unwrap();
        let registry = create_standard_tool_registry(
            temp_dir.path(),
            ProviderType::Anthropic,
            Some("test-key"),
            Some(ModelTiers::anthropic()),
        );

        // Should have task tools since provider was specified
        assert!(registry.get("task").is_some());
        assert!(registry.get("task_output").is_some());
    }

    #[test]
    fn test_registry_without_provider_has_no_task_tools() {
        let temp_dir = tempdir().unwrap();
        let registry = ToolRegistryBuilder::new(temp_dir.path().to_path_buf()).build();

        // No task tools without provider
        assert!(registry.get("task").is_none());
        assert!(registry.get("task_output").is_none());
    }
}
