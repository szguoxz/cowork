//! Configuration management for Cowork
//!
//! Handles loading, saving, and managing application configuration
//! including API keys and provider settings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use crate::prompt::ComponentPaths;
use crate::provider::catalog;

use crate::error::{Error, Result};

/// Default constants used throughout the application
pub mod defaults {
    /// Default command execution timeout in seconds
    pub const COMMAND_TIMEOUT_SECS: u64 = 30;

    /// Maximum number of iterations for the agentic loop
    pub const MAX_AGENTIC_ITERATIONS: usize = 100;

    /// Default approval level for tool execution
    pub const DEFAULT_APPROVAL_LEVEL: &str = "low";

    /// Default history file name
    pub const HISTORY_FILE_NAME: &str = "history.txt";

    /// Default max tokens for LLM requests
    pub const DEFAULT_MAX_TOKENS: u32 = 4096;

    /// Default temperature for LLM requests
    pub const DEFAULT_TEMPERATURE: f32 = 0.7;

    /// Default provider name
    pub const DEFAULT_PROVIDER: &str = "anthropic";

    /// Session directory name (relative to workspace)
    pub const SESSION_DIR_NAME: &str = ".cowork";

    /// Maximum context size in characters before truncation
    pub const MAX_CONTEXT_SIZE: usize = 100_000;

    /// Default number of search results to return
    pub const DEFAULT_SEARCH_RESULTS: usize = 50;
}

/// MCP transport type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// Standard I/O transport (local process)
    #[default]
    Stdio,
    /// HTTP/SSE transport (remote server)
    Http,
}

/// MCP (Model Context Protocol) server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Transport type (stdio or http)
    #[serde(default)]
    pub transport: McpTransport,
    /// Command to run the MCP server (for stdio transport)
    #[serde(default)]
    pub command: String,
    /// Arguments to pass to the command (for stdio transport)
    #[serde(default)]
    pub args: Vec<String>,
    /// URL of the MCP server (for http transport)
    #[serde(default)]
    pub url: Option<String>,
    /// Environment variables for the server
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// HTTP headers for remote servers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Whether this server is enabled (auto-starts on CLI startup)
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl McpServerConfig {
    /// Create a new MCP server config for stdio transport
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            transport: McpTransport::Stdio,
            command: command.into(),
            args: Vec::new(),
            url: None,
            env: HashMap::new(),
            headers: HashMap::new(),
            enabled: true,
        }
    }

    /// Create a new MCP server config for HTTP transport
    pub fn new_http(url: impl Into<String>) -> Self {
        Self {
            transport: McpTransport::Http,
            command: String::new(),
            args: Vec::new(),
            url: Some(url.into()),
            env: HashMap::new(),
            headers: HashMap::new(),
            enabled: true,
        }
    }

    /// Check if this is an HTTP transport
    pub fn is_http(&self) -> bool {
        self.transport == McpTransport::Http || self.url.is_some()
    }

    /// Add arguments to the config
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    /// Add an environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Add an HTTP header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set enabled status
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default provider to use
    #[serde(default = "default_provider_name")]
    pub default_provider: String,
    /// Multiple provider configurations
    #[serde(default = "default_providers")]
    pub providers: HashMap<String, ProviderConfig>,
    /// Legacy single provider config (for backwards compatibility)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConfig>,
    /// MCP server configurations
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    /// Approval settings
    #[serde(default)]
    pub approval: ApprovalConfig,
    /// General application settings
    #[serde(default)]
    pub general: GeneralConfig,
    /// Web search settings
    #[serde(default)]
    pub web_search: WebSearchConfig,
    /// Prompt system settings
    #[serde(default)]
    pub prompt: PromptSystemConfig,
}

fn default_provider_name() -> String {
    "anthropic".to_string()
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut providers = HashMap::new();
    providers.insert("anthropic".to_string(), ProviderConfig::anthropic());
    providers.insert("openai".to_string(), ProviderConfig::openai());
    providers.insert("gemini".to_string(), ProviderConfig::gemini());
    providers
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_provider: default_provider_name(),
            providers: default_providers(),
            provider: None,
            mcp_servers: HashMap::new(),
            approval: ApprovalConfig::default(),
            general: GeneralConfig::default(),
            web_search: WebSearchConfig::default(),
            prompt: PromptSystemConfig::default(),
        }
    }
}

impl Config {
    /// Get the provider config for the default provider
    pub fn get_default_provider(&self) -> Option<&ProviderConfig> {
        // First check new multi-provider config
        if let Some(provider) = self.providers.get(&self.default_provider) {
            return Some(provider);
        }
        // Fall back to legacy single provider config
        self.provider.as_ref()
    }

    /// Get a specific provider config by name
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// Get mutable provider config
    pub fn get_provider_mut(&mut self, name: &str) -> Option<&mut ProviderConfig> {
        self.providers.get_mut(name)
    }

    /// List all configured provider names
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

/// Model tiers for subagent execution
/// Maps capability tiers to specific model names for each provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTiers {
    /// Fast model for quick, simple tasks (e.g., Haiku, GPT-5 Mini)
    pub fast: String,
    /// Balanced model for general tasks (e.g., Sonnet, GPT-5.2)
    pub balanced: String,
    /// Powerful model for complex reasoning (e.g., Opus, o3)
    pub powerful: String,
}

impl ModelTiers {
    /// Create model tiers from catalog for a provider
    fn from_catalog(provider_id: &str) -> Option<Self> {
        catalog::model_tiers(provider_id).map(|(fast, balanced, powerful)| Self {
            fast: fast.to_string(),
            balanced: balanced.to_string(),
            powerful: powerful.to_string(),
        })
    }

    /// Default tiers for Anthropic
    pub fn anthropic() -> Self {
        Self::from_catalog("anthropic").unwrap()
    }

    /// Default tiers for OpenAI
    pub fn openai() -> Self {
        Self::from_catalog("openai").unwrap()
    }

    /// Default tiers for Gemini
    pub fn gemini() -> Self {
        Self::from_catalog("gemini").unwrap()
    }

    /// Default tiers for DeepSeek
    pub fn deepseek() -> Self {
        Self::from_catalog("deepseek").unwrap()
    }

    /// Default tiers for Groq
    pub fn groq() -> Self {
        Self::from_catalog("groq").unwrap()
    }

    /// Default tiers for xAI
    pub fn xai() -> Self {
        Self::from_catalog("xai").unwrap()
    }

    /// Default tiers for Cohere
    pub fn cohere() -> Self {
        Self::from_catalog("cohere").unwrap()
    }

    /// Default tiers for Perplexity
    pub fn perplexity() -> Self {
        Self::from_catalog("perplexity").unwrap()
    }

    /// Default tiers for Ollama
    pub fn ollama() -> Self {
        Self::from_catalog("ollama").unwrap()
    }

    /// Default tiers for Together AI
    pub fn together() -> Self {
        Self::from_catalog("together").unwrap()
    }

    /// Default tiers for Fireworks AI
    pub fn fireworks() -> Self {
        Self::from_catalog("fireworks").unwrap()
    }

    /// Default tiers for Zai (Zhipu AI)
    pub fn zai() -> Self {
        Self::from_catalog("zai").unwrap()
    }

    /// Default tiers for Nebius
    pub fn nebius() -> Self {
        Self::from_catalog("nebius").unwrap()
    }

    /// Default tiers for MIMO
    pub fn mimo() -> Self {
        Self::from_catalog("mimo").unwrap()
    }

    /// Default tiers for BigModel.cn
    pub fn bigmodel() -> Self {
        Self::from_catalog("bigmodel").unwrap()
    }

    /// Get default tiers for a provider type
    pub fn for_provider(provider_type: &str) -> Self {
        let lower = provider_type.to_lowercase();
        let provider_id = match lower.as_str() {
            "gemini" | "google" => "gemini",
            "xai" | "grok" => "xai",
            "zai" | "zhipu" => "zai",
            other => other,
        };
        Self::from_catalog(provider_id).unwrap_or_else(Self::anthropic)
    }

    /// Get the model name for a given tier
    pub fn get_model(&self, tier: &str) -> &str {
        match tier.to_lowercase().as_str() {
            "fast" | "haiku" => &self.fast,
            "powerful" | "opus" => &self.powerful,
            _ => &self.balanced, // Default to balanced
        }
    }
}

/// LLM Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    /// Provider type: "anthropic", "openai", "gemini", etc.
    pub provider_type: String,
    /// API key (can be loaded from env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Environment variable name for API key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Model to use (default/primary model)
    pub model: String,
    /// Model tiers for subagent execution (fast/balanced/powerful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_tiers: Option<ModelTiers>,
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
        Self::anthropic()
    }
}

impl ProviderConfig {
    /// Create provider config from catalog
    fn from_catalog(provider_id: &str) -> Self {
        let provider = catalog::get(provider_id);
        Self {
            provider_type: provider_id.to_string(),
            api_key: None,
            api_key_env: provider.and_then(|p| p.api_key_env.clone()),
            model: catalog::default_model(provider_id).unwrap_or("").to_string(),
            model_tiers: None,
            base_url: None,
            default_max_tokens: 4096,
            default_temperature: 0.7,
        }
    }

    /// Create Anthropic provider config
    pub fn anthropic() -> Self { Self::from_catalog("anthropic") }

    /// Create OpenAI provider config
    pub fn openai() -> Self { Self::from_catalog("openai") }

    /// Create Gemini provider config
    pub fn gemini() -> Self { Self::from_catalog("gemini") }

    /// Create Groq provider config
    pub fn groq() -> Self { Self::from_catalog("groq") }

    /// Create DeepSeek provider config
    pub fn deepseek() -> Self { Self::from_catalog("deepseek") }

    /// Create Cohere provider config
    pub fn cohere() -> Self { Self::from_catalog("cohere") }

    /// Create Together AI provider config
    pub fn together() -> Self { Self::from_catalog("together") }

    /// Create Fireworks AI provider config
    pub fn fireworks() -> Self { Self::from_catalog("fireworks") }

    /// Create Zai (Zhipu AI) provider config
    pub fn zai() -> Self { Self::from_catalog("zai") }

    /// Create Nebius provider config
    pub fn nebius() -> Self { Self::from_catalog("nebius") }

    /// Create MIMO provider config
    pub fn mimo() -> Self { Self::from_catalog("mimo") }

    /// Create BigModel.cn provider config
    pub fn bigmodel() -> Self { Self::from_catalog("bigmodel") }

    /// Get model tiers, falling back to provider defaults
    pub fn get_model_tiers(&self) -> ModelTiers {
        self.model_tiers
            .clone()
            .unwrap_or_else(|| ModelTiers::for_provider(&self.provider_type))
    }
}

impl ProviderConfig {
    /// Get the API key, checking environment variable if not set directly
    pub fn get_api_key(&self) -> Option<String> {
        // First check direct API key
        if let Some(key) = &self.api_key
            && !key.is_empty() {
                return Some(key.clone());
            }

        // Then check environment variable
        if let Some(env_name) = &self.api_key_env
            && let Ok(key) = std::env::var(env_name)
                && !key.is_empty() {
                    return Some(key);
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
            "deepseek" => std::env::var("DEEPSEEK_API_KEY").ok(),
            "together" => std::env::var("TOGETHER_API_KEY").ok(),
            "fireworks" => std::env::var("FIREWORKS_API_KEY").ok(),
            "zai" | "zhipu" => std::env::var("ZAI_API_KEY").ok(),
            "nebius" => std::env::var("NEBIUS_API_KEY").ok(),
            "mimo" => std::env::var("MIMO_API_KEY").ok(),
            "bigmodel" => std::env::var("BIGMODEL_API_KEY").ok(),
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

/// Prompt system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSystemConfig {
    /// Enable hook execution
    #[serde(default = "default_true")]
    pub enable_hooks: bool,
    /// Enable plugin system
    #[serde(default = "default_true")]
    pub enable_plugins: bool,
    /// Enterprise config directory (highest priority)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enterprise_config: Option<PathBuf>,
    /// Custom project config directory (overrides .claude/)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_config: Option<PathBuf>,
    /// Custom user config directory (overrides ~/.claude/)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_config: Option<PathBuf>,
    /// Hook execution timeout in milliseconds
    #[serde(default = "default_hook_timeout_ms")]
    pub hook_timeout_ms: u64,
    /// Enable auto-invocation of skills
    #[serde(default = "default_true")]
    pub enable_skill_auto_invoke: bool,
    /// Base system prompt (if not using default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_system_prompt: Option<String>,
}

fn default_hook_timeout_ms() -> u64 {
    30_000 // 30 seconds
}

impl Default for PromptSystemConfig {
    fn default() -> Self {
        Self {
            enable_hooks: true,
            enable_plugins: true,
            enterprise_config: None,
            project_config: None,
            user_config: None,
            hook_timeout_ms: default_hook_timeout_ms(),
            enable_skill_auto_invoke: true,
            base_system_prompt: None,
        }
    }
}

impl PromptSystemConfig {
    /// Build ComponentPaths from this configuration
    pub fn to_component_paths(&self, workspace_dir: &Path) -> ComponentPaths {
        let project_path = Some(self.project_config
            .clone()
            .unwrap_or_else(|| workspace_dir.join(".claude")));

        let user_path = Some(self.user_config
            .clone()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".claude"))
                    .unwrap_or_else(|| PathBuf::from(".claude"))
            }));

        ComponentPaths {
            enterprise_path: self.enterprise_config.clone(),
            project_path,
            user_path,
            plugin_paths: Vec::new(), // Plugins discovered separately
        }
    }
}

/// Web search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    /// Fallback search provider when native search is not available
    /// Options: "brave", "serpapi", "serper", "tavily", "searxng"
    #[serde(default = "default_fallback_provider")]
    pub fallback_provider: String,
    /// API endpoint for fallback search (e.g., SearXNG instance URL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_endpoint: Option<String>,
    /// API key for fallback search provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_api_key: Option<String>,
    /// Environment variable name for fallback API key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_api_key_env: Option<String>,
    /// Maximum results to return
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_fallback_provider() -> String {
    "brave".to_string()
}

fn default_max_results() -> usize {
    10
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            fallback_provider: default_fallback_provider(),
            fallback_endpoint: None,
            fallback_api_key: None,
            fallback_api_key_env: Some("BRAVE_API_KEY".to_string()),
            max_results: default_max_results(),
        }
    }
}

impl WebSearchConfig {
    /// Get the fallback API key, checking environment variable if not set directly
    pub fn get_fallback_api_key(&self) -> Option<String> {
        // First check direct API key
        if let Some(key) = &self.fallback_api_key
            && !key.is_empty()
        {
            return Some(key.clone());
        }

        // Then check environment variable
        if let Some(env_name) = &self.fallback_api_key_env
            && let Ok(key) = std::env::var(env_name)
            && !key.is_empty()
        {
            return Some(key);
        }

        // Try default environment variables based on provider
        match self.fallback_provider.as_str() {
            "brave" => std::env::var("BRAVE_API_KEY").ok().filter(|k| !k.is_empty()),
            "serper" => std::env::var("SERPER_API_KEY").ok().filter(|k| !k.is_empty()),
            "serpapi" => std::env::var("SERPAPI_API_KEY").ok().filter(|k| !k.is_empty()),
            "tavily" => std::env::var("TAVILY_API_KEY").ok().filter(|k| !k.is_empty()),
            "searxng" => None, // SearXNG typically doesn't need an API key
            _ => None,
        }
    }

    /// Get the default endpoint for the fallback provider
    pub fn get_fallback_endpoint(&self) -> Option<String> {
        if self.fallback_endpoint.is_some() {
            return self.fallback_endpoint.clone();
        }

        // Default endpoints for known providers
        match self.fallback_provider.as_str() {
            "brave" => Some("https://api.search.brave.com/res/v1/web/search".to_string()),
            "serper" => Some("https://google.serper.dev/search".to_string()),
            "serpapi" => Some("https://serpapi.com/search".to_string()),
            "tavily" => Some("https://api.tavily.com/search".to_string()),
            _ => None,
        }
    }

    /// Check if fallback search is configured
    pub fn is_fallback_configured(&self) -> bool {
        self.get_fallback_endpoint().is_some() &&
        (self.fallback_provider == "searxng" || self.get_fallback_api_key().is_some())
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

    /// Update provider settings for a specific provider
    pub fn set_provider(&mut self, name: &str, provider: ProviderConfig) {
        self.config.providers.insert(name.to_string(), provider);
    }

    /// Set the default provider
    pub fn set_default_provider(&mut self, name: &str) {
        self.config.default_provider = name.to_string();
    }

    /// Set API key for a specific provider (or default if not specified)
    pub fn set_api_key(&mut self, key: String) {
        self.set_api_key_for(&self.config.default_provider.clone(), key);
    }

    /// Set API key for a specific provider
    pub fn set_api_key_for(&mut self, provider_name: &str, key: String) {
        if let Some(provider) = self.config.providers.get_mut(provider_name) {
            provider.api_key = Some(key);
        }
    }

    /// Get API key for default provider
    pub fn get_api_key(&self) -> Option<String> {
        self.config
            .get_default_provider()
            .and_then(|p| p.get_api_key())
    }

    /// Get API key for a specific provider
    pub fn get_api_key_for(&self, provider_name: &str) -> Option<String> {
        self.config
            .get_provider(provider_name)
            .and_then(|p| p.get_api_key())
    }

    /// Check if API key is configured for default provider
    pub fn has_api_key(&self) -> bool {
        self.get_api_key().is_some()
    }

    /// Check if API key is configured for a specific provider
    pub fn has_api_key_for(&self, provider_name: &str) -> bool {
        self.get_api_key_for(provider_name).is_some()
    }

    /// Get the default provider name
    pub fn default_provider(&self) -> &str {
        &self.config.default_provider
    }

    /// Get the config file path
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Check if setup is complete (at least one provider has an API key)
    /// Note: This checks both config file and environment variables
    pub fn is_setup_complete(&self) -> bool {
        self.config
            .providers
            .values()
            .any(|p| p.get_api_key().is_some())
    }

    /// Check if setup is complete based on config file only (not env vars)
    /// This is used for onboarding - we want to show the wizard if no config
    /// file exists or no API key is saved, even if env vars are set.
    pub fn is_setup_complete_config_only(&self) -> bool {
        // Check if config file exists on disk
        if !self.config_path.exists() {
            return false;
        }

        // Check if any provider has an explicit API key in config (not from env)
        self.config
            .providers
            .values()
            .any(|p| p.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false))
    }

    /// List all configured providers
    pub fn list_providers(&self) -> Vec<&str> {
        self.config.list_providers()
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
        assert_eq!(config.default_provider, "anthropic");

        // Check that multiple providers are configured
        assert!(config.providers.contains_key("anthropic"));
        assert!(config.providers.contains_key("openai"));
        assert!(config.providers.contains_key("gemini"));

        // Check default provider settings
        let anthropic = config.get_default_provider().unwrap();
        assert_eq!(anthropic.provider_type, "anthropic");
        assert_eq!(anthropic.model, catalog::default_model("anthropic").unwrap());
    }

    #[test]
    fn test_multi_provider_config() {
        let config = Config::default();

        // Check each provider
        let anthropic = config.get_provider("anthropic").unwrap();
        assert_eq!(anthropic.model, catalog::default_model("anthropic").unwrap());

        let openai = config.get_provider("openai").unwrap();
        assert_eq!(openai.model, catalog::default_model("openai").unwrap());

        let gemini = config.get_provider("gemini").unwrap();
        assert_eq!(gemini.model, catalog::default_model("gemini").unwrap());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("default_provider"));
        assert!(toml_str.contains("[providers.anthropic]"));
        assert!(toml_str.contains("[providers.openai]"));

        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.default_provider, config.default_provider);
        assert_eq!(parsed.providers.len(), config.providers.len());
    }

    #[test]
    fn test_api_key_from_env() {
        let config = ProviderConfig {
            api_key_env: Some("TEST_API_KEY_12345".to_string()),
            ..Default::default()
        };

        // Set env var
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::set_var("TEST_API_KEY_12345", "test-key") };
        assert_eq!(config.get_api_key(), Some("test-key".to_string()));

        // Clean up
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::remove_var("TEST_API_KEY_12345") };
    }

    #[test]
    fn test_provider_factories() {
        let anthropic = ProviderConfig::anthropic();
        assert_eq!(anthropic.provider_type, "anthropic");
        assert_eq!(anthropic.api_key_env, Some("ANTHROPIC_API_KEY".to_string()));

        let openai = ProviderConfig::openai();
        assert_eq!(openai.provider_type, "openai");
        assert_eq!(openai.api_key_env, Some("OPENAI_API_KEY".to_string()));

        let gemini = ProviderConfig::gemini();
        assert_eq!(gemini.provider_type, "gemini");
        assert_eq!(gemini.api_key_env, Some("GEMINI_API_KEY".to_string()));
    }
}
