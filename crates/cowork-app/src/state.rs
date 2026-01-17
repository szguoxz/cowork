//! Application state management

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use cowork_core::{AgentRegistry, Context};

/// Global application state
pub struct AppState {
    /// Current execution context
    pub context: Arc<RwLock<Context>>,
    /// Agent registry
    pub registry: Arc<RwLock<AgentRegistry>>,
    /// Workspace root path
    pub workspace_path: PathBuf,
}

/// Task state for tracking running tasks
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskState {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub progress: f32,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

/// Application settings
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// LLM provider configuration
    pub provider: ProviderSettings,
    /// Approval policy settings
    pub approval: ApprovalSettings,
    /// UI preferences
    pub ui: UiSettings,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSettings {
    pub provider_type: String,
    pub api_key: Option<String>,
    pub model: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApprovalSettings {
    pub auto_approve_level: String,
    pub show_confirmation_dialogs: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiSettings {
    pub theme: String,
    pub font_size: u32,
    pub show_tool_calls: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: ProviderSettings {
                provider_type: "anthropic".to_string(),
                api_key: None,
                model: "claude-3-sonnet-20240229".to_string(),
                base_url: None,
            },
            approval: ApprovalSettings {
                auto_approve_level: "low".to_string(),
                show_confirmation_dialogs: true,
            },
            ui: UiSettings {
                theme: "system".to_string(),
                font_size: 14,
                show_tool_calls: true,
            },
        }
    }
}
