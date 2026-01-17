//! Application state management

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use cowork_core::{AgentRegistry, Config, ConfigManager, Context};

use crate::agentic_loop::LoopHandle;
use crate::chat::ChatSession;

/// Global application state
pub struct AppState {
    /// Current execution context
    pub context: Arc<RwLock<Context>>,
    /// Agent registry
    pub registry: Arc<RwLock<AgentRegistry>>,
    /// Workspace root path
    pub workspace_path: PathBuf,
    /// Active chat sessions
    pub sessions: Arc<RwLock<HashMap<String, ChatSession>>>,
    /// Configuration manager
    pub config_manager: Arc<RwLock<ConfigManager>>,
    /// Active agentic loop handles
    pub loop_handles: Arc<RwLock<HashMap<String, LoopHandle>>>,
}

impl AppState {
    /// Get the current configuration
    pub async fn config(&self) -> Config {
        let cm = self.config_manager.read().await;
        cm.config().clone()
    }

    /// Check if API key is configured
    pub async fn has_api_key(&self) -> bool {
        let cm = self.config_manager.read().await;
        cm.has_api_key()
    }

    /// Get API key
    pub async fn get_api_key(&self) -> Option<String> {
        let cm = self.config_manager.read().await;
        cm.get_api_key()
    }
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

/// Application settings (serializable form for frontend)
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

impl From<&Config> for Settings {
    fn from(config: &Config) -> Self {
        // Get the default provider settings
        let (provider_type, api_key, model, base_url) =
            if let Some(provider) = config.get_default_provider() {
                (
                    provider.provider_type.clone(),
                    provider.get_api_key(),
                    provider.model.clone(),
                    provider.base_url.clone(),
                )
            } else {
                // Fallback to defaults
                (
                    "anthropic".to_string(),
                    None,
                    "claude-sonnet-4-20250514".to_string(),
                    None,
                )
            };

        Self {
            provider: ProviderSettings {
                provider_type,
                api_key,
                model,
                base_url,
            },
            approval: ApprovalSettings {
                auto_approve_level: config.approval.auto_approve_level.clone(),
                show_confirmation_dialogs: config.approval.show_dialogs,
            },
            ui: UiSettings {
                theme: "system".to_string(),
                font_size: 14,
                show_tool_calls: true,
            },
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: ProviderSettings {
                provider_type: "anthropic".to_string(),
                api_key: None,
                model: "claude-sonnet-4-20250514".to_string(),
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
