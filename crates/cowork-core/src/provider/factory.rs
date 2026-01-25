//! Provider Factory Module
//!
//! Shared provider creation and configuration utilities for both CLI and UI.
//! Centralizes API key retrieval, model tier configuration, and provider instantiation.

use crate::config::{ConfigManager, ModelTiers};
use crate::error::{Error, Result};
use super::genai_provider::{GenAIProvider, ProviderType};

/// Get API key for a provider, checking config then environment variables
///
/// # Arguments
/// * `config_manager` - The configuration manager
/// * `provider_type` - The provider type to get the API key for
///
/// # Returns
/// The API key if found, None otherwise
pub fn get_api_key(config_manager: &ConfigManager, provider_type: ProviderType) -> Option<String> {
    let provider_name = provider_type.to_string();

    // Try config first
    if let Some(provider_config) = config_manager.config().providers.get(&provider_name)
        && let Some(key) = provider_config.get_api_key() {
            return Some(key);
        }

    // Fall back to environment variable
    if let Some(env_var) = provider_type.api_key_env()
        && let Ok(key) = std::env::var(env_var) {
            return Some(key);
        }

    None
}

/// Get model tiers from config or use provider defaults
///
/// # Arguments
/// * `config_manager` - The configuration manager
/// * `provider_type` - The provider type to get model tiers for
///
/// # Returns
/// The configured model tiers, or provider defaults if not configured
pub fn get_model_tiers(config_manager: &ConfigManager, provider_type: ProviderType) -> ModelTiers {
    let provider_name = provider_type.to_string();

    // Check config for custom model_tiers
    if let Some(provider_config) = config_manager.config().providers.get(&provider_name) {
        return provider_config.get_model_tiers();
    }

    // Fall back to provider defaults
    ModelTiers::for_provider(&provider_name)
}

/// Check if an API key is configured for the given provider
///
/// # Arguments
/// * `config_manager` - The configuration manager
/// * `provider_type` - The provider type to check
///
/// # Returns
/// true if an API key is available (from config or environment), false otherwise
pub fn has_api_key_configured(config_manager: &ConfigManager, provider_type: ProviderType) -> bool {
    get_api_key(config_manager, provider_type).is_some()
}

/// Create a provider from config, falling back to environment variables
///
/// # Arguments
/// * `config_manager` - The configuration manager
/// * `provider_type` - The provider type to create
/// * `model_override` - Optional model name to override the configured model
///
/// # Returns
/// A configured GenAIProvider instance
///
/// # Errors
/// Returns an error if no API key is configured
pub fn create_provider_from_config(
    config_manager: &ConfigManager,
    provider_type: ProviderType,
    model_override: Option<&str>,
) -> Result<GenAIProvider> {
    let provider_name = provider_type.to_string();

    // Try to get provider config from config file
    if let Some(provider_config) = config_manager.config().providers.get(&provider_name) {
        // Get API key from config or environment
        let api_key = provider_config.get_api_key().ok_or_else(|| {
            Error::Config(format!(
                "No API key configured for {}. Set it in config or via {}",
                provider_name,
                provider_type.api_key_env().unwrap_or("environment variable")
            ))
        })?;

        // Use model from argument, or from config
        let model = model_override.unwrap_or(&provider_config.model);

        // Create provider with config (supports custom base_url)
        return Ok(GenAIProvider::with_config(
            provider_type,
            &api_key,
            Some(model),
            provider_config.base_url.as_deref(),
        ));
    }

    // No config for this provider, try environment variable
    if let Some(env_var) = provider_type.api_key_env()
        && let Ok(api_key) = std::env::var(env_var) {
            return Ok(GenAIProvider::with_api_key(
                provider_type,
                &api_key,
                model_override,
            ));
        }

    Err(Error::Config(format!(
        "No configuration found for provider '{}'. Add it to config file or set {}",
        provider_name,
        provider_type.api_key_env().unwrap_or("API key")
    )))
}

/// Create a provider with direct settings (for UI use)
///
/// # Arguments
/// * `provider_type` - The provider type to create
/// * `api_key` - The API key
/// * `model` - The model name
///
/// # Returns
/// A configured GenAIProvider instance
pub fn create_provider_with_settings(
    provider_type: ProviderType,
    api_key: &str,
    model: &str,
) -> GenAIProvider {
    GenAIProvider::with_api_key(provider_type, api_key, Some(model))
}

/// Create a provider directly from a ProviderConfig
///
/// This is useful when you have a ProviderConfig but not a full ConfigManager.
///
/// # Arguments
/// * `config` - The provider configuration
///
/// # Returns
/// A configured GenAIProvider instance
///
/// # Errors
/// Returns an error if no API key is configured
pub fn create_provider_from_provider_config(
    config: &crate::config::ProviderConfig,
) -> Result<GenAIProvider> {
    let provider_type: ProviderType = config
        .provider_type
        .parse()
        .map_err(|e: String| Error::Config(e))?;

    let api_key = config.get_api_key().ok_or_else(|| {
        Error::Config(format!(
            "No API key configured for {}. Set it in config or via {}",
            config.provider_type,
            provider_type.api_key_env().unwrap_or("environment variable")
        ))
    })?;

    Ok(GenAIProvider::with_config(
        provider_type,
        &api_key,
        Some(&config.model),
        config.base_url.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::catalog;
    use crate::provider::LlmProvider;
    use tempfile::tempdir;

    fn write_test_config(config_path: &std::path::Path) {
        std::fs::write(config_path, format!(r#"
            default_provider = "anthropic"
            [providers.anthropic]
            provider_type = "anthropic"
            model = "{}"
        "#, catalog::default_model("anthropic").unwrap())).unwrap();
    }

    #[test]
    fn test_get_api_key_from_env() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        write_test_config(&config_path);

        let config_manager = ConfigManager::with_path(config_path).unwrap();

        // Set environment variable
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "test-key-from-env") };

        let api_key = get_api_key(&config_manager, ProviderType::Anthropic);
        assert_eq!(api_key, Some("test-key-from-env".to_string()));

        // Clean up
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn test_has_api_key_configured_false() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        write_test_config(&config_path);

        let config_manager = ConfigManager::with_path(config_path).unwrap();

        // Make sure no env var is set
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

        assert!(!has_api_key_configured(&config_manager, ProviderType::Anthropic));
    }

    #[test]
    fn test_get_model_tiers_default() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        write_test_config(&config_path);

        let config_manager = ConfigManager::with_path(config_path).unwrap();

        let tiers = get_model_tiers(&config_manager, ProviderType::Anthropic);
        assert_eq!(tiers.fast, catalog::model_id("anthropic", catalog::ModelTier::Fast).unwrap());
        assert_eq!(tiers.balanced, catalog::default_model("anthropic").unwrap());
        assert_eq!(tiers.powerful, catalog::model_id("anthropic", catalog::ModelTier::Powerful).unwrap());
    }

    #[test]
    fn test_create_provider_with_settings() {
        let provider = create_provider_with_settings(
            ProviderType::Anthropic,
            "test-key",
            catalog::default_model("anthropic").unwrap(),
        );
        assert_eq!(provider.name(), "anthropic");
    }
}
