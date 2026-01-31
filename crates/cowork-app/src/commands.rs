//! Tauri commands - Settings, configuration, and component management
//!
//! The main loop commands are in simple_commands.rs

use serde::{Deserialize, Serialize};
use tauri::State;

use cowork_core::prompt::{
    AgentInfo, CommandInfo, ComponentRegistry, PluginInfo, RegistrySummary, SkillInfo,
};
use cowork_core::provider::{catalog, create_provider_with_settings, ChatMessage};
use cowork_core::ApprovalLevel;

use crate::state::{AppState, Settings};

/// Get current settings
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let cm = state.config_manager.read();
    Ok(Settings::from(cm.config()))
}

/// Update settings
#[tauri::command]
pub async fn update_settings(
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cm = state.config_manager.write();
    let config = cm.config_mut();

    // Update provider settings
    let provider_name = &settings.provider.provider_type;

    // Use provided model or fall back to catalog default
    let model = settings.provider.model
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            catalog::default_model(provider_name)
                .unwrap_or("gpt-4o")
                .to_string()
        });

    if let Some(provider) = config.providers.get_mut(provider_name) {
        provider.model = model.clone();
        if let Some(key) = &settings.provider.api_key {
            provider.api_key = Some(key.clone());
        }
        provider.base_url = settings.provider.base_url.clone();
    } else {
        // Create new provider entry
        config.providers.insert(
            provider_name.clone(),
            cowork_core::config::ProviderConfig {
                provider_type: provider_name.clone(),
                model,
                api_key: settings.provider.api_key.clone(),
                base_url: settings.provider.base_url.clone(),
                default_max_tokens: 4096,
                default_temperature: 0.7,
                model_tiers: None,
            },
        );
    }
    config.default_provider = provider_name.clone();

    // Update approval settings
    config.approval.auto_approve_level = settings.approval.auto_approve_level.clone();
    config.approval.show_dialogs = settings.approval.show_confirmation_dialogs;

    // Update web search settings if provided
    // Handle both setting and clearing the API key
    if let Some(web_search) = &settings.web_search {
        // If api_key is Some, use it (even if empty string - we'll filter that)
        // If api_key is None, clear the config value
        config.web_search.api_key = web_search
            .api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .cloned();
    }

    Ok(())
}

/// Save settings to disk
#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>) -> Result<(), String> {
    let cm = state.config_manager.read();
    cm.save().map_err(|e| e.to_string())
}

/// Check if API key is configured
#[tauri::command]
pub async fn check_api_key(state: State<'_, AppState>) -> Result<bool, String> {
    let cm = state.config_manager.read();
    Ok(cm.has_api_key())
}

/// API test result matching frontend expectations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTestResult {
    pub success: bool,
    pub message: String,
}

/// Test the API connection with current settings
#[tauri::command]
pub async fn test_api_connection(
    provider_type: String,
    api_key: String,
    model: Option<String>,
) -> Result<ApiTestResult, String> {
    // Validate provider exists in catalog
    if catalog::get(&provider_type).is_none() {
        return Ok(ApiTestResult {
            success: false,
            message: format!("Unknown provider: {}", provider_type),
        });
    }

    // Use provided model or fall back to provider's default
    let model_id = model
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            catalog::default_model(&provider_type)
                .unwrap_or("gpt-4o")
                .to_string()
        });

    let provider = match create_provider_with_settings(&provider_type, &api_key, &model_id) {
        Ok(p) => p,
        Err(e) => {
            return Ok(ApiTestResult {
                success: false,
                message: format!("Failed to create provider: {}", e),
            });
        }
    };

    // Try a simple chat
    let messages = vec![ChatMessage::user("Say 'hello' and nothing else.")];

    match provider.chat(messages, None).await {
        Ok(response) => Ok(ApiTestResult {
            success: true,
            message: response
                .content
                .unwrap_or_else(|| "Connected successfully".to_string()),
        }),
        Err(e) => Ok(ApiTestResult {
            success: false,
            message: e.to_string(),
        }),
    }
}

/// Check if initial setup is complete
///
/// This checks if the config FILE exists with an API key saved.
/// Environment variables don't count - we want users to go through
/// the wizard to save their preferences in the config file.
#[tauri::command]
pub async fn is_setup_complete(state: State<'_, AppState>) -> Result<bool, String> {
    let cm = state.config_manager.read();

    // Use config-only check: requires config file to exist with API key saved
    // This ensures wizard shows even if ANTHROPIC_API_KEY env var is set
    if !cm.is_setup_complete_config_only() {
        return Ok(false);
    }

    // And if approval level is set
    let level: Result<ApprovalLevel, _> = cm.config().approval.auto_approve_level.parse();
    Ok(level.is_ok())
}

/// Available model info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Fetch available models for a provider
#[tauri::command]
pub async fn fetch_provider_models(provider_type: String) -> Result<Vec<ModelInfo>, String> {
    // Get provider from catalog directly using the string
    let Some(provider) = catalog::get(&provider_type) else {
        return Ok(vec![]);
    };

    // Build model list from the three tiers
    let mut models = Vec::new();

    if let Some(balanced) = provider.model(catalog::ModelTier::Balanced) {
        models.push(ModelInfo {
            id: balanced.id.clone(),
            name: balanced.name.clone(),
            description: "Best balance of speed and capability".to_string(),
        });
    }

    if let Some(powerful) = provider.model(catalog::ModelTier::Powerful) {
        // Only add if different from balanced
        if models.iter().all(|m| m.id != powerful.id) {
            models.push(ModelInfo {
                id: powerful.id.clone(),
                name: powerful.name.clone(),
                description: "Most capable model".to_string(),
            });
        }
    }

    if let Some(fast) = provider.model(catalog::ModelTier::Fast) {
        // Only add if different from balanced
        if models.iter().all(|m| m.id != fast.id) {
            models.push(ModelInfo {
                id: fast.id.clone(),
                name: fast.name.clone(),
                description: "Fast and efficient".to_string(),
            });
        }
    }

    Ok(models)
}

// ================== Component Registry Commands ==================

/// Get a summary of all registered components
#[tauri::command]
pub async fn get_component_summary(state: State<'_, AppState>) -> Result<RegistrySummary, String> {
    let registry = ComponentRegistry::for_workspace(&state.workspace_path)
        .map_err(|e| e.to_string())?;
    Ok(registry.summary())
}

/// List all registered agents
#[tauri::command]
pub async fn list_agents(state: State<'_, AppState>) -> Result<Vec<AgentInfo>, String> {
    let registry = ComponentRegistry::for_workspace(&state.workspace_path)
        .map_err(|e| e.to_string())?;
    Ok(registry.summary().agents)
}

/// List all registered commands
#[tauri::command]
pub async fn list_commands(state: State<'_, AppState>) -> Result<Vec<CommandInfo>, String> {
    let registry = ComponentRegistry::for_workspace(&state.workspace_path)
        .map_err(|e| e.to_string())?;
    Ok(registry.summary().commands)
}

/// List all registered skills
#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let registry = ComponentRegistry::for_workspace(&state.workspace_path)
        .map_err(|e| e.to_string())?;
    Ok(registry.summary().skills)
}

/// List all registered plugins
#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginInfo>, String> {
    let registry = ComponentRegistry::for_workspace(&state.workspace_path)
        .map_err(|e| e.to_string())?;
    Ok(registry.summary().plugins)
}
