//! Configuration management for Cowork
//!
//! Handles loading, saving, and managing application configuration
//! including API keys and provider settings.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// LLM provider configuration
    #[serde(default)]
    pub provider: ProviderConfig,
    /// Approval settings
    #[serde(default)]
    pub approval: ApprovalConfig,
    /// Browser automation settings
    #[serde(default)]
    pub browser: BrowserConfig,
    /// General application settings
    #[serde(default)]
    pub general: GeneralConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            approval: ApprovalConfig::default(),
            browser: BrowserConfig::default(),
            general: GeneralConfig::default(),
        }
    }
}

/// LLM Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    /// Provider type: "anthropic", "openai", "ollama"
    pub provider_type: String,
    /// API key (can be loaded from env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Environment variable name for API key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Model to use
    pub model: String,
    /// Base URL for the API (optional, for self-hosted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Default max tokens
    pub default_max_tokens: u32,
    /// Default temperature
    pub default_temperature: f32,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: "anthropic".to_string(),
            api_key: None,
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            model: "claude-sonnet-4-20250514".to_string(),
            base_url: None,
            default_max_tokens: 4096,
            default_temperature: 0.7,
        }
    }
}

impl ProviderConfig {
    /// Get the API key, checking environment variable if not set directly
    pub fn get_api_key(&self) -> Option<String> {
        // First check direct API key
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                return Some(key.clone());
            }
        }

        // Then check environment variable
        if let Some(env_name) = &self.api_key_env {
            if let Ok(key) = std::env::var(env_name) {
                if !key.is_empty() {
                    return Some(key);
                }
            }
        }

        // Try default environment variables based on provider
        match self.provider_type.as_str() {
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
            "openai" => std::env::var("OPENAI_API_KEY").ok(),
            "gemini" | "google" => std::env::var("GEMINI_API_KEY")
                .or_else(|_| std::env::var("GOOGLE_API_KEY"))
                .ok(),
            "cohere" => std::env::var("COHERE_API_KEY").ok(),
            "perplexity" => std::env::var("PERPLEXITY_API_KEY").ok(),
            "groq" => std::env::var("GROQ_API_KEY").ok(),
            "xai" | "grok" => std::env::var("XAI_API_KEY").ok(),
            _ => None,
        }
    }
}

/// Approval policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalConfig {
    /// Auto-approve operations up to this level
    pub auto_approve_level: String,
    /// Show confirmation dialogs
    pub show_dialogs: bool,
    /// Timeout for approval requests (seconds)
    pub timeout_secs: u64,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            auto_approve_level: "low".to_string(),
            show_dialogs: true,
            timeout_secs: 300,
        }
    }
}

/// Browser automation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Run browser in headless mode
    pub headless: bool,
    /// Default page load timeout (seconds)
    pub timeout_secs: u64,
    /// Screenshot output directory
    pub screenshot_dir: Option<PathBuf>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            timeout_secs: 30,
            screenshot_dir: None,
        }
    }
}

/// General application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Working directory
    pub workspace_dir: Option<PathBuf>,
    /// Log level
    pub log_level: String,
    /// Enable telemetry
    pub telemetry: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            log_level: "info".to_string(),
            telemetry: false,
        }
    }
}

/// Configuration manager for loading and saving config
pub struct ConfigManager {
    config_path: PathBuf,
    config: Config,
}

impl ConfigManager {
    /// Create a new config manager with default path
    pub fn new() -> Result<Self> {
        let config_path = Self::default_config_path()?;
        Self::with_path(config_path)
    }

    /// Create a config manager with a specific path
    pub fn with_path(config_path: PathBuf) -> Result<Self> {
        let config = if config_path.exists() {
            Self::load_from_path(&config_path)?
        } else {
            Config::default()
        };

        Ok(Self { config_path, config })
    }

    /// Get the default config path
    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| Error::Config("Could not find config directory".to_string()))?;

        Ok(config_dir.join("cowork").join("config.toml"))
    }

    /// Load configuration from a file
    fn load_from_path(path: &Path) -> Result<Config> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("Failed to read config: {}", e)))?;

        toml::from_str(&content)
            .map_err(|e| Error::Config(format!("Failed to parse config: {}", e)))
    }

    /// Get the current configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get mutable access to configuration
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Save the current configuration to disk
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Config(format!("Failed to create config dir: {}", e)))?;
        }

        let content = toml::to_string_pretty(&self.config)
            .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(&self.config_path, content)
            .map_err(|e| Error::Config(format!("Failed to write config: {}", e)))?;

        Ok(())
    }

    /// Update provider settings
    pub fn set_provider(&mut self, provider: ProviderConfig) {
        self.config.provider = provider;
    }

    /// Set API key (directly, not via env var)
    pub fn set_api_key(&mut self, key: String) {
        self.config.provider.api_key = Some(key);
    }

    /// Get API key from config or environment
    pub fn get_api_key(&self) -> Option<String> {
        self.config.provider.get_api_key()
    }

    /// Check if API key is configured
    pub fn has_api_key(&self) -> bool {
        self.get_api_key().is_some()
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            config_path: PathBuf::from("config.toml"),
            config: Config::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider.provider_type, "anthropic");
        assert_eq!(config.provider.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("provider_type"));

        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.provider.provider_type, config.provider.provider_type);
    }

    #[test]
    fn test_api_key_from_env() {
        let mut config = ProviderConfig::default();
        config.api_key_env = Some("TEST_API_KEY_12345".to_string());

        // Set env var
        std::env::set_var("TEST_API_KEY_12345", "test-key");
        assert_eq!(config.get_api_key(), Some("test-key".to_string()));

        // Clean up
        std::env::remove_var("TEST_API_KEY_12345");
    }
}
