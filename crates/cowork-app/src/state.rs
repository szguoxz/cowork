//! Application state management

use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;

use cowork_core::provider::catalog;
use cowork_core::session::SessionManager;
use cowork_core::{Config, ConfigManager, Context};

/// Global application state
pub struct AppState {
    /// Current execution context
    pub context: Arc<RwLock<Context>>,
    /// Workspace root path
    pub workspace_path: PathBuf,
    /// Configuration manager
    pub config_manager: Arc<RwLock<ConfigManager>>,
    /// Session manager for the unified agent loop
    pub session_manager: Arc<SessionManager>,
}

impl AppState {
    /// Get the current configuration
    pub fn config(&self) -> Config {
        let cm = self.config_manager.read();
        cm.config().clone()
    }

    /// Check if API key is configured
    pub fn has_api_key(&self) -> bool {
        let cm = self.config_manager.read();
        cm.has_api_key()
    }

    /// Get API key
    pub fn get_api_key(&self) -> Option<String> {
        let cm = self.config_manager.read();
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
    /// Web search configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search: Option<WebSearchSettings>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSettings {
    pub provider_type: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebSearchSettings {
    pub api_key: Option<String>,
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
                // Fallback to defaults (must match config.rs defaults)
                (
                    "anthropic".to_string(),
                    None,
                    catalog::default_model("anthropic").unwrap_or("").to_string(),
                    None,
                )
            };

        // Convert web_search config if API key is configured
        let web_search = if config.web_search.api_key.is_some() {
            Some(WebSearchSettings {
                api_key: config.web_search.api_key.clone(),
            })
        } else {
            None
        };

        Self {
            provider: ProviderSettings {
                provider_type,
                api_key,
                model: Some(model),
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
            web_search,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: ProviderSettings {
                provider_type: "anthropic".to_string(),
                api_key: None,
                model: Some(catalog::default_model("anthropic").unwrap_or("").to_string()),
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
            web_search: None,
        }
    }
}
