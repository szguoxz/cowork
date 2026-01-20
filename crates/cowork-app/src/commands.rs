//! Tauri commands - Settings and configuration only
//!
//! The main loop commands are in simple_commands.rs

use serde::{Deserialize, Serialize};
use tauri::State;

use cowork_core::provider::{
    create_provider_with_settings, LlmProvider, LlmRequest, ProviderType,
};
use cowork_core::ApprovalLevel;

use crate::state::{AppState, Settings};

/// Get current settings
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let cm = state.config_manager.read().await;
    Ok(Settings::from(cm.config()))
}

/// Update settings
#[tauri::command]
pub async fn update_settings(
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut cm = state.config_manager.write().await;
    let config = cm.config_mut();

    // Update provider settings
    let provider_name = &settings.provider.provider_type;
    if let Some(provider) = config.providers.get_mut(provider_name) {
        provider.model = settings.provider.model.clone();
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
                model: settings.provider.model.clone(),
                api_key: settings.provider.api_key.clone(),
                api_key_env: None,
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

    Ok(())
}

/// Save settings to disk
#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>) -> Result<(), String> {
    let cm = state.config_manager.read().await;
    cm.save().map_err(|e| e.to_string())
}

/// Check if API key is configured
#[tauri::command]
pub async fn check_api_key(state: State<'_, AppState>) -> Result<bool, String> {
    let cm = state.config_manager.read().await;
    Ok(cm.has_api_key())
}

/// Test the API connection with current settings
#[tauri::command]
pub async fn test_api_connection(
    provider_type: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    let ptype: ProviderType = provider_type
        .parse()
        .map_err(|e: String| e)?;

    let provider = create_provider_with_settings(ptype, &api_key, &model);

    // Try a simple completion
    let request = LlmRequest::new(vec![cowork_core::provider::LlmMessage {
        role: "user".to_string(),
        content: "Say 'hello' and nothing else.".to_string(),
    }])
    .with_max_tokens(10);

    let response = provider.complete(request).await.map_err(|e| e.to_string())?;

    Ok(response.content.unwrap_or_else(|| "No response".to_string()))
}

/// Check if initial setup is complete
#[tauri::command]
pub async fn is_setup_complete(state: State<'_, AppState>) -> Result<bool, String> {
    let cm = state.config_manager.read().await;

    // Setup is complete if we have an API key
    if !cm.has_api_key() {
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
    let ptype: ProviderType = provider_type.parse().map_err(|e: String| e)?;

    // Return hardcoded models for now - could fetch from API later
    let models = match ptype {
        ProviderType::Anthropic => vec![
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                name: "Claude Sonnet 4".to_string(),
                description: "Best balance of speed and capability".to_string(),
            },
            ModelInfo {
                id: "claude-opus-4-20250514".to_string(),
                name: "Claude Opus 4".to_string(),
                description: "Most capable model".to_string(),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                description: "Fast and efficient".to_string(),
            },
        ],
        ProviderType::OpenAI => vec![
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                description: "Latest GPT-4 model".to_string(),
            },
            ModelInfo {
                id: "gpt-4-turbo".to_string(),
                name: "GPT-4 Turbo".to_string(),
                description: "Fast GPT-4".to_string(),
            },
        ],
        _ => vec![],
    };

    Ok(models)
}
