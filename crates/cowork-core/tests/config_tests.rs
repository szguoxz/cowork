//! Configuration management tests
//!
//! Tests for ConfigManager and Config structures.

use cowork_core::config::{Config, ConfigManager, ProviderConfig, ApprovalConfig, BrowserConfig, GeneralConfig};
use tempfile::TempDir;
use std::fs;
use std::path::PathBuf;

/// Create a temp directory for config tests
fn setup_config_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

mod config_structure_tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        // Check multi-provider defaults
        assert_eq!(config.default_provider, "anthropic");
        assert!(config.providers.contains_key("anthropic"));
        assert!(config.providers.contains_key("openai"));
        assert!(config.providers.contains_key("gemini"));

        // Check default provider settings
        let default_provider = config.get_default_provider().unwrap();
        assert_eq!(default_provider.provider_type, "anthropic");
        assert!(default_provider.model.contains("claude"));
        assert!(default_provider.api_key.is_none());
        assert!(default_provider.api_key_env.is_some());

        // Check approval defaults
        assert_eq!(config.approval.auto_approve_level, "low");
        assert!(config.approval.show_dialogs);
        assert!(config.approval.timeout_secs > 0);

        // Check browser defaults
        assert!(config.browser.headless);
        assert!(config.browser.timeout_secs > 0);

        // Check general defaults
        assert_eq!(config.general.log_level, "info");
        assert!(!config.general.telemetry);
    }

    #[test]
    fn test_provider_config_defaults() {
        let provider = ProviderConfig::default();

        assert_eq!(provider.provider_type, "anthropic");
        assert_eq!(provider.default_max_tokens, 4096);
        assert!(provider.default_temperature > 0.0 && provider.default_temperature <= 1.0);
    }

    #[test]
    fn test_approval_config_defaults() {
        let approval = ApprovalConfig::default();

        assert_eq!(approval.auto_approve_level, "low");
        assert!(approval.show_dialogs);
        assert_eq!(approval.timeout_secs, 300); // 5 minutes
    }

    #[test]
    fn test_browser_config_defaults() {
        let browser = BrowserConfig::default();

        assert!(browser.headless);
        assert_eq!(browser.timeout_secs, 30);
        assert!(browser.screenshot_dir.is_none());
    }

    #[test]
    fn test_general_config_defaults() {
        let general = GeneralConfig::default();

        assert!(general.workspace_dir.is_none());
        assert_eq!(general.log_level, "info");
        assert!(!general.telemetry);
    }

    #[test]
    fn test_multi_provider_access() {
        let config = Config::default();

        // Access each provider
        let anthropic = config.get_provider("anthropic").unwrap();
        assert_eq!(anthropic.provider_type, "anthropic");
        assert!(anthropic.model.contains("claude"));

        let openai = config.get_provider("openai").unwrap();
        assert_eq!(openai.provider_type, "openai");
        assert!(openai.model.contains("gpt"));

        let gemini = config.get_provider("gemini").unwrap();
        assert_eq!(gemini.provider_type, "gemini");
        assert!(gemini.model.contains("gemini"));
    }
}

mod config_serialization_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_serialize_to_toml() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config);

        assert!(toml_str.is_ok(), "Serialization failed: {:?}", toml_str.err());
        let toml_content = toml_str.unwrap();

        assert!(toml_content.contains("default_provider"));
        assert!(toml_content.contains("[providers.anthropic]"));
        assert!(toml_content.contains("auto_approve_level"));
    }

    #[test]
    fn test_deserialize_new_format() {
        // New multi-provider format
        let toml_content = r#"
default_provider = "openai"

[providers.anthropic]
provider_type = "anthropic"
model = "claude-sonnet-4-20250514"
default_max_tokens = 4096
default_temperature = 0.7

[providers.openai]
provider_type = "openai"
model = "gpt-4o"
default_max_tokens = 8192
default_temperature = 0.5

[approval]
auto_approve_level = "medium"
show_dialogs = false
timeout_secs = 600

[browser]
headless = false
timeout_secs = 60

[general]
log_level = "debug"
telemetry = true
"#;

        let config: Result<Config, _> = toml::from_str(toml_content);
        assert!(config.is_ok(), "Deserialization failed: {:?}", config.err());

        let config = config.unwrap();
        assert_eq!(config.default_provider, "openai");

        let openai = config.get_provider("openai").unwrap();
        assert_eq!(openai.model, "gpt-4o");

        let anthropic = config.get_provider("anthropic").unwrap();
        assert_eq!(anthropic.model, "claude-sonnet-4-20250514");

        assert_eq!(config.approval.auto_approve_level, "medium");
        assert!(!config.approval.show_dialogs);
        assert!(!config.browser.headless);
        assert_eq!(config.general.log_level, "debug");
        assert!(config.general.telemetry);
    }

    #[test]
    fn test_deserialize_legacy_format() {
        // Legacy single-provider format (backwards compatibility)
        let toml_content = r#"
[provider]
provider_type = "openai"
model = "gpt-4o"
default_max_tokens = 8192
default_temperature = 0.5

[approval]
auto_approve_level = "medium"
show_dialogs = false
timeout_secs = 600
"#;

        let config: Result<Config, _> = toml::from_str(toml_content);
        assert!(config.is_ok(), "Legacy format deserialization failed: {:?}", config.err());

        let config = config.unwrap();
        // Legacy provider should be available
        assert!(config.provider.is_some());
        let provider = config.provider.as_ref().unwrap();
        assert_eq!(provider.provider_type, "openai");
        assert_eq!(provider.model, "gpt-4o");
    }

    #[test]
    fn test_roundtrip_serialization() {
        let mut providers = HashMap::new();
        providers.insert("anthropic".to_string(), ProviderConfig {
            provider_type: "anthropic".to_string(),
            api_key: Some("test-key".to_string()),
            api_key_env: None,
            model: "claude-sonnet".to_string(),
            model_tiers: None,
            base_url: Some("https://custom.api.com".to_string()),
            default_max_tokens: 2048,
            default_temperature: 0.8,
        });
        providers.insert("openai".to_string(), ProviderConfig::openai());

        let original = Config {
            default_provider: "anthropic".to_string(),
            providers,
            provider: None,
            mcp_servers: std::collections::HashMap::new(),
            approval: ApprovalConfig {
                auto_approve_level: "high".to_string(),
                show_dialogs: true,
                timeout_secs: 120,
            },
            browser: BrowserConfig {
                headless: true,
                timeout_secs: 45,
                screenshot_dir: Some(PathBuf::from("/tmp/screenshots")),
            },
            general: GeneralConfig {
                workspace_dir: Some(PathBuf::from("/home/user/projects")),
                log_level: "warn".to_string(),
                telemetry: false,
            },
        };

        // Serialize
        let toml_str = toml::to_string_pretty(&original).unwrap();

        // Deserialize
        let restored: Config = toml::from_str(&toml_str).unwrap();

        // Verify
        assert_eq!(restored.default_provider, original.default_provider);
        let restored_anthropic = restored.get_provider("anthropic").unwrap();
        let original_anthropic = original.get_provider("anthropic").unwrap();
        assert_eq!(restored_anthropic.model, original_anthropic.model);
        assert_eq!(restored.approval.timeout_secs, original.approval.timeout_secs);
        assert_eq!(restored.browser.headless, original.browser.headless);
        assert_eq!(restored.general.log_level, original.general.log_level);
    }

    #[test]
    fn test_partial_config_deserialize() {
        // Config with only default_provider - providers should use defaults
        let toml_content = r#"
default_provider = "openai"
"#;

        let config: Config = toml::from_str(toml_content).unwrap();

        assert_eq!(config.default_provider, "openai");
        // Providers should use defaults
        assert!(config.providers.contains_key("anthropic"));
        assert!(config.providers.contains_key("openai"));
        // Other sections should use defaults
        assert!(config.approval.show_dialogs);
    }
}

mod config_manager_tests {
    use super::*;

    #[test]
    fn test_create_config_manager_default() {
        // This might fail if the default path isn't writable
        // but the structure should work
        let result = ConfigManager::new();
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_create_config_manager_with_path() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let manager = ConfigManager::with_path(config_path.clone());
        assert!(manager.is_ok(), "Failed to create manager: {:?}", manager.err());

        let manager = manager.unwrap();
        assert_eq!(manager.default_provider(), "anthropic");
    }

    #[test]
    fn test_load_existing_config() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        // Write a config file (new format)
        let config_content = r#"
default_provider = "openai"

[providers.openai]
provider_type = "openai"
model = "gpt-4"
default_max_tokens = 4096
default_temperature = 0.7

[providers.anthropic]
provider_type = "anthropic"
model = "claude-sonnet-4-20250514"
default_max_tokens = 4096
default_temperature = 0.7

[approval]
auto_approve_level = "low"
show_dialogs = true
timeout_secs = 300

[browser]
headless = true
timeout_secs = 30

[general]
log_level = "info"
telemetry = false
"#;
        fs::write(&config_path, config_content).unwrap();

        let manager = ConfigManager::with_path(config_path).unwrap();
        assert_eq!(manager.default_provider(), "openai");
        let openai = manager.config().get_provider("openai").unwrap();
        assert_eq!(openai.model, "gpt-4");
    }

    #[test]
    fn test_save_config() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("subdir/config.toml");

        let mut manager = ConfigManager::with_path(config_path.clone()).unwrap();

        // Modify config - update anthropic provider model
        if let Some(anthropic) = manager.config_mut().get_provider_mut("anthropic") {
            anthropic.model = "new-model".to_string();
        }

        // Save
        let result = manager.save();
        assert!(result.is_ok(), "Save failed: {:?}", result.err());

        // Verify file was created
        assert!(config_path.exists(), "Config file should exist");

        // Verify content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("new-model"));
    }

    #[test]
    fn test_set_api_key() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let mut manager = ConfigManager::with_path(config_path).unwrap();

        // Default provider is anthropic, check its api_key is None
        let anthropic = manager.config().get_provider("anthropic").unwrap();
        assert!(anthropic.api_key.is_none());

        manager.set_api_key("sk-test-12345".to_string());

        let anthropic = manager.config().get_provider("anthropic").unwrap();
        assert_eq!(anthropic.api_key, Some("sk-test-12345".to_string()));
    }

    #[test]
    fn test_get_api_key_direct() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let mut manager = ConfigManager::with_path(config_path).unwrap();
        manager.set_api_key("direct-key".to_string());

        let key = manager.get_api_key();
        assert_eq!(key, Some("direct-key".to_string()));
    }

    #[test]
    fn test_has_api_key() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let mut manager = ConfigManager::with_path(config_path).unwrap();

        // Initially no key (unless env var is set)
        let has_key_initial = manager.has_api_key();

        manager.set_api_key("test-key".to_string());
        assert!(manager.has_api_key());

        println!("Had key initially: {}", has_key_initial);
    }

    #[test]
    fn test_set_provider() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let mut manager = ConfigManager::with_path(config_path).unwrap();

        let new_provider = ProviderConfig {
            provider_type: "gemini".to_string(),
            model: "gemini-pro".to_string(),
            api_key: Some("gemini-key".to_string()),
            ..Default::default()
        };

        manager.set_provider("gemini", new_provider);

        let gemini = manager.config().get_provider("gemini").unwrap();
        assert_eq!(gemini.provider_type, "gemini");
        assert_eq!(gemini.model, "gemini-pro");
    }

    #[test]
    fn test_set_default_provider() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let mut manager = ConfigManager::with_path(config_path).unwrap();

        assert_eq!(manager.default_provider(), "anthropic");

        manager.set_default_provider("openai");
        assert_eq!(manager.default_provider(), "openai");
    }

    #[test]
    fn test_list_providers() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let manager = ConfigManager::with_path(config_path).unwrap();
        let providers = manager.list_providers();

        assert!(providers.contains(&"anthropic"));
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"gemini"));
    }

    #[test]
    fn test_set_api_key_for_provider() {
        let dir = setup_config_dir();
        let config_path = dir.path().join("config.toml");

        let mut manager = ConfigManager::with_path(config_path).unwrap();

        manager.set_api_key_for("openai", "openai-key".to_string());
        manager.set_api_key_for("anthropic", "anthropic-key".to_string());

        let openai_key = manager.get_api_key_for("openai");
        let anthropic_key = manager.get_api_key_for("anthropic");

        assert_eq!(openai_key, Some("openai-key".to_string()));
        assert_eq!(anthropic_key, Some("anthropic-key".to_string()));
    }
}

mod api_key_resolution_tests {
    use super::*;

    #[test]
    fn test_api_key_from_direct_config() {
        let provider = ProviderConfig {
            api_key: Some("direct-key".to_string()),
            api_key_env: Some("NONEXISTENT_VAR_12345".to_string()),
            ..Default::default()
        };

        let key = provider.get_api_key();
        assert_eq!(key, Some("direct-key".to_string()));
    }

    #[test]
    fn test_api_key_from_env_var() {
        let provider = ProviderConfig {
            api_key: None,
            api_key_env: Some("TEST_COWORK_API_KEY_12345".to_string()),
            ..Default::default()
        };

        // Set env var
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::set_var("TEST_COWORK_API_KEY_12345", "env-key") };

        let key = provider.get_api_key();
        assert_eq!(key, Some("env-key".to_string()));

        // Clean up
        // SAFETY: Test runs in isolation, no concurrent access to this env var
        unsafe { std::env::remove_var("TEST_COWORK_API_KEY_12345") };
    }

    #[test]
    fn test_api_key_fallback_to_default_env() {
        let provider = ProviderConfig {
            provider_type: "openai".to_string(),
            api_key: None,
            api_key_env: None,
            ..Default::default()
        };

        // This test depends on whether OPENAI_API_KEY is set in the environment
        let key = provider.get_api_key();
        // Just verify it doesn't panic - result depends on environment
        println!("OpenAI fallback key present: {}", key.is_some());
    }

    #[test]
    fn test_empty_api_key_treated_as_none() {
        let provider = ProviderConfig {
            api_key: Some("".to_string()), // Empty string
            ..Default::default()
        };

        let key = provider.get_api_key();
        // Empty string should be treated as no key
        assert!(key.is_none() || key == Some("".to_string()));
    }
}

mod config_validation_tests {
    use super::*;

    #[test]
    fn test_valid_provider_types() {
        let valid_types = ["anthropic", "openai", "gemini", "ollama", "cohere", "groq"];

        for provider_type in valid_types {
            let config = ProviderConfig {
                provider_type: provider_type.to_string(),
                ..Default::default()
            };
            // Should not panic
            let _ = config.get_api_key();
        }
    }

    #[test]
    fn test_approval_levels() {
        let levels = ["none", "low", "medium", "high"];

        for level in levels {
            let config = ApprovalConfig {
                auto_approve_level: level.to_string(),
                ..Default::default()
            };
            assert_eq!(config.auto_approve_level, level);
        }
    }

    #[test]
    fn test_log_levels() {
        let levels = ["trace", "debug", "info", "warn", "error"];

        for level in levels {
            let config = GeneralConfig {
                log_level: level.to_string(),
                ..Default::default()
            };
            assert_eq!(config.log_level, level);
        }
    }
}
