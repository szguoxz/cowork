//! Session management skills
//!
//! These skills help manage the current session:
//! - /config - View/edit configuration
//! - /model - Show/switch model
//! - /provider - Show/switch provider

use std::path::PathBuf;

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};
use crate::config::ConfigManager;

// =============================================================================
// ConfigSkill - /config
// =============================================================================

/// Config skill - view/edit configuration
pub struct ConfigSkill {
    _workspace: PathBuf,
}

impl ConfigSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { _workspace: workspace }
    }
}

impl Skill for ConfigSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "config".to_string(),
            display_name: "Configuration".to_string(),
            description: "View current configuration".to_string(),
            usage: "/config [key] [value]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let config_manager = match ConfigManager::new() {
                Ok(cm) => cm,
                Err(e) => return SkillResult::error(format!("Failed to load config: {}", e)),
            };

            let args: Vec<&str> = ctx.args.split_whitespace().collect();

            if args.is_empty() {
                // Show current config
                let config = config_manager.config();
                let mut output = String::from("Current Configuration:\n\n");

                output.push_str(&format!("Default Provider: {}\n", config.default_provider));
                output.push_str("\nConfigured Providers:\n");

                for (name, provider) in &config.providers {
                    let has_key = provider.get_api_key().is_some();
                    let key_status = if has_key { "(key set)" } else { "(no key)" };
                    output.push_str(&format!(
                        "  {} - {} {} {}\n",
                        name, provider.provider_type, provider.model, key_status
                    ));
                }

                if !config.mcp_servers.is_empty() {
                    output.push_str("\nMCP Servers:\n");
                    for (name, server) in &config.mcp_servers {
                        let status = if server.enabled { "enabled" } else { "disabled" };
                        output.push_str(&format!("  {} - {} ({})\n", name, server.command, status));
                    }
                }

                output.push_str(&format!("\nApproval Level: {}\n", config.approval.auto_approve_level));
                output.push_str(&format!("Browser Headless: {}\n", config.browser.headless));
                output.push_str(&format!("Log Level: {}\n", config.general.log_level));

                if let Ok(path) = ConfigManager::default_config_path() {
                    output.push_str(&format!("\nConfig file: {}\n", path.display()));
                }

                SkillResult::success(output.trim())
            } else if args.len() == 1 {
                // Show specific config value
                let key = args[0];
                let config = config_manager.config();

                let value = match key {
                    "provider" | "default_provider" => config.default_provider.clone(),
                    "model" => {
                        config.get_default_provider()
                            .map(|p| p.model.clone())
                            .unwrap_or_else(|| "not set".to_string())
                    }
                    "approval" | "approval_level" => config.approval.auto_approve_level.clone(),
                    "headless" => config.browser.headless.to_string(),
                    "log_level" => config.general.log_level.clone(),
                    _ => return SkillResult::error(format!(
                        "Unknown config key: {}\n\nAvailable: provider, model, approval, headless, log_level",
                        key
                    )),
                };

                SkillResult::success(format!("{} = {}", key, value))
            } else {
                // Set config value (not implemented yet - would need mutable access)
                SkillResult::error(
                    "Setting config values via /config is not yet supported.\n\nEdit the config file directly."
                )
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

// =============================================================================
// ModelSkill - /model
// =============================================================================

/// Model skill - show/switch model
pub struct ModelSkill {
    _workspace: PathBuf,
}

impl ModelSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { _workspace: workspace }
    }
}

impl Skill for ModelSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "model".to_string(),
            display_name: "Model".to_string(),
            description: "Show or switch the active model".to_string(),
            usage: "/model [name]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let config_manager = match ConfigManager::new() {
                Ok(cm) => cm,
                Err(e) => return SkillResult::error(format!("Failed to load config: {}", e)),
            };

            let args = ctx.args.trim();

            if args.is_empty() {
                // Show current model
                let config = config_manager.config();
                let provider_name = &config.default_provider;

                if let Some(provider) = config.get_default_provider() {
                    let tiers = provider.get_model_tiers();

                    let mut output = format!(
                        "Current Model: {}\nProvider: {}\n\nModel Tiers:\n  Fast: {}\n  Balanced: {}\n  Powerful: {}\n",
                        provider.model,
                        provider_name,
                        tiers.fast,
                        tiers.balanced,
                        tiers.powerful
                    );

                    output.push_str("\nAvailable models by provider:\n");
                    for (name, p) in &config.providers {
                        output.push_str(&format!("  {}: {}\n", name, p.model));
                    }

                    SkillResult::success(output.trim())
                } else {
                    SkillResult::error("No default provider configured")
                }
            } else {
                // Request to switch model (not implemented - would need session state)
                SkillResult::error(format!(
                    "Model switching is not yet implemented.\n\nCurrent model: {}\n\nTo change the default model, edit your config file or restart with --model {}",
                    config_manager.config().get_default_provider().map(|p| p.model.as_str()).unwrap_or("unknown"),
                    args
                ))
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}

// =============================================================================
// ProviderSkill - /provider
// =============================================================================

/// Provider skill - show/switch provider
pub struct ProviderSkill {
    _workspace: PathBuf,
}

impl ProviderSkill {
    pub fn new(workspace: PathBuf) -> Self {
        Self { _workspace: workspace }
    }
}

impl Skill for ProviderSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "provider".to_string(),
            display_name: "Provider".to_string(),
            description: "Show or switch the active provider".to_string(),
            usage: "/provider [name]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let config_manager = match ConfigManager::new() {
                Ok(cm) => cm,
                Err(e) => return SkillResult::error(format!("Failed to load config: {}", e)),
            };

            let args = ctx.args.trim();

            if args.is_empty() {
                // Show current provider and list available
                let config = config_manager.config();

                let mut output = format!("Current Provider: {}\n\n", config.default_provider);
                output.push_str("Available Providers:\n");

                for (name, provider) in &config.providers {
                    let is_default = if name == &config.default_provider { " (default)" } else { "" };
                    let has_key = provider.get_api_key().is_some();
                    let key_status = if has_key { "ready" } else { "no key" };

                    output.push_str(&format!(
                        "  {} - {} [{}]{}\n",
                        name, provider.model, key_status, is_default
                    ));
                }

                output.push_str("\nTo switch providers, restart with --provider <name>");

                SkillResult::success(output.trim())
            } else {
                // Request to switch provider (not implemented - would need session state)
                let config = config_manager.config();

                if config.providers.contains_key(args) {
                    SkillResult::error(format!(
                        "Provider switching is not yet implemented.\n\nTo use {}, restart with --provider {}",
                        args, args
                    ))
                } else {
                    let available: Vec<&str> = config.providers.keys().map(|s| s.as_str()).collect();
                    SkillResult::error(format!(
                        "Unknown provider: {}\n\nAvailable: {}",
                        args,
                        available.join(", ")
                    ))
                }
            }
        })
    }

    fn prompt_template(&self) -> &str {
        ""
    }
}
