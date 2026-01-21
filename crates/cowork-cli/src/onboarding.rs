//! Onboarding wizard for first-run setup
//!
//! Guides new users through provider selection, API key configuration,
//! and initial setup.

use console::style;
use dialoguer::{theme::ColorfulTheme, Input, Password, Select};

use cowork_core::config::{ConfigManager, ProviderConfig};
use cowork_core::provider::{fetch_models, GenAIProvider, ModelInfo, ProviderType};

/// Provider information for display
pub struct ProviderInfo {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub signup_url: &'static str,
    pub env_var: &'static str,
    #[allow(dead_code)] // Kept for reference, models now fetched from API
    pub default_model: &'static str,
}

/// Get provider info for a provider type
pub fn get_provider_info(provider_type: ProviderType) -> ProviderInfo {
    match provider_type {
        ProviderType::Anthropic => ProviderInfo {
            name: "anthropic",
            display_name: "Anthropic (Claude)",
            description: "Best for code, writing, and reasoning",
            signup_url: "https://console.anthropic.com/",
            env_var: "ANTHROPIC_API_KEY",
            default_model: "claude-sonnet-4-20250514",
        },
        ProviderType::OpenAI => ProviderInfo {
            name: "openai",
            display_name: "OpenAI (GPT-4)",
            description: "Versatile and widely supported",
            signup_url: "https://platform.openai.com/",
            env_var: "OPENAI_API_KEY",
            default_model: "gpt-4o",
        },
        ProviderType::Gemini => ProviderInfo {
            name: "gemini",
            display_name: "Google Gemini",
            description: "Large context window (1M tokens)",
            signup_url: "https://aistudio.google.com/",
            env_var: "GEMINI_API_KEY",
            default_model: "gemini-2.0-flash",
        },
        ProviderType::Groq => ProviderInfo {
            name: "groq",
            display_name: "Groq",
            description: "Ultra-fast inference",
            signup_url: "https://console.groq.com/",
            env_var: "GROQ_API_KEY",
            default_model: "llama-3.3-70b-versatile",
        },
        ProviderType::DeepSeek => ProviderInfo {
            name: "deepseek",
            display_name: "DeepSeek",
            description: "Cost-effective reasoning",
            signup_url: "https://platform.deepseek.com/",
            env_var: "DEEPSEEK_API_KEY",
            default_model: "deepseek-chat",
        },
        ProviderType::XAI => ProviderInfo {
            name: "xai",
            display_name: "xAI (Grok)",
            description: "Latest Grok models",
            signup_url: "https://x.ai/api",
            env_var: "XAI_API_KEY",
            default_model: "grok-2",
        },
        ProviderType::Together => ProviderInfo {
            name: "together",
            display_name: "Together AI",
            description: "200+ open source models",
            signup_url: "https://api.together.xyz/",
            env_var: "TOGETHER_API_KEY",
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
        },
        ProviderType::Fireworks => ProviderInfo {
            name: "fireworks",
            display_name: "Fireworks AI",
            description: "Fast open source model inference",
            signup_url: "https://fireworks.ai/",
            env_var: "FIREWORKS_API_KEY",
            default_model: "accounts/fireworks/models/llama-v3p1-70b-instruct",
        },
        ProviderType::Zai => ProviderInfo {
            name: "zai",
            display_name: "Zai (Zhipu AI)",
            description: "GLM-4 models from China",
            signup_url: "https://z.ai/",
            env_var: "ZAI_API_KEY",
            default_model: "glm-4-plus",
        },
        ProviderType::Nebius => ProviderInfo {
            name: "nebius",
            display_name: "Nebius AI Studio",
            description: "30+ open source models",
            signup_url: "https://studio.nebius.ai/",
            env_var: "NEBIUS_API_KEY",
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct",
        },
        ProviderType::MIMO => ProviderInfo {
            name: "mimo",
            display_name: "MIMO (Xiaomi)",
            description: "Xiaomi's MIMO models",
            signup_url: "https://xiaomimimo.com/",
            env_var: "MIMO_API_KEY",
            default_model: "mimo-v2-flash",
        },
        ProviderType::BigModel => ProviderInfo {
            name: "bigmodel",
            display_name: "BigModel.cn",
            description: "Zhipu AI China platform",
            signup_url: "https://open.bigmodel.cn/",
            env_var: "BIGMODEL_API_KEY",
            default_model: "glm-4-plus",
        },
        ProviderType::Ollama => ProviderInfo {
            name: "ollama",
            display_name: "Ollama (Local)",
            description: "Run models locally, no API key needed",
            signup_url: "https://ollama.ai/",
            env_var: "",
            default_model: "llama3.2",
        },
        _ => ProviderInfo {
            name: "unknown",
            display_name: "Unknown Provider",
            description: "Custom provider configuration",
            signup_url: "",
            env_var: "API_KEY",
            default_model: "",
        },
    }
}

/// All supported providers for onboarding
const ONBOARDING_PROVIDERS: &[ProviderType] = &[
    ProviderType::Anthropic,
    ProviderType::OpenAI,
    ProviderType::Gemini,
    ProviderType::Groq,
    ProviderType::DeepSeek,
    ProviderType::XAI,
    ProviderType::Together,
    ProviderType::Fireworks,
    ProviderType::Zai,
    ProviderType::Nebius,
    ProviderType::MIMO,
    ProviderType::BigModel,
    ProviderType::Ollama,
];

/// Onboarding wizard for first-run setup
pub struct OnboardingWizard {
    config_manager: ConfigManager,
}

impl OnboardingWizard {
    /// Create a new onboarding wizard
    pub fn new(config_manager: ConfigManager) -> Self {
        Self { config_manager }
    }

    /// Check if onboarding should run (first-run detection)
    pub fn should_run(&self) -> bool {
        // Check if config file exists
        if !self.config_manager.config_path().exists() {
            return true;
        }

        // Check if any provider has an API key configured
        !self
            .config_manager
            .config()
            .providers
            .values()
            .any(|p| p.get_api_key().is_some())
    }

    /// Run the onboarding wizard
    pub async fn run(&mut self) -> anyhow::Result<()> {
        self.show_welcome();

        // Step 1: Provider selection
        let provider_type = self.select_provider()?;
        let provider_info = get_provider_info(provider_type);

        // Loop for API key retry
        loop {
            // Step 2: API key input (skip for Ollama)
            let api_key = if provider_type == ProviderType::Ollama {
                None
            } else {
                Some(self.input_api_key(&provider_info)?)
            };

            // Step 3: Model selection (fetches from API)
            let model = self
                .select_model(provider_type, api_key.as_deref())
                .await?;

            // Step 4: Connection test (skip for Ollama)
            if let Some(ref key) = api_key
                && !self.test_connection(provider_type, key, &model).await? {
                    // User chose to try again - loop back to step 2
                    println!("{}", style("Let's try again...").dim());
                    println!();
                    continue;
                }

            // Step 5: Save configuration
            self.save_config(provider_type, &provider_info, api_key.as_deref(), &model)?;

            // Step 6: Show completion
            self.show_completion(&provider_info);

            break;
        }

        Ok(())
    }

    /// Consume the wizard and return the config manager
    pub fn into_config_manager(self) -> ConfigManager {
        self.config_manager
    }

    fn show_welcome(&self) {
        println!();
        println!(
            "{}",
            style("┌─────────────────────────────────────────────────────┐").cyan()
        );
        println!(
            "{}",
            style("│                                                     │").cyan()
        );
        println!(
            "{}  {}  {}",
            style("│").cyan(),
            style("Welcome to Cowork!").bold().white(),
            style("                            │").cyan()
        );
        println!(
            "{}  {}  {}",
            style("│").cyan(),
            style("AI-Powered Coding Assistant").dim(),
            style("                 │").cyan()
        );
        println!(
            "{}",
            style("│                                                     │").cyan()
        );
        println!(
            "{}",
            style("└─────────────────────────────────────────────────────┘").cyan()
        );
        println!();
        println!(
            "{}",
            style("Let's get you set up in just a few steps.").dim()
        );
        println!();
    }

    fn select_provider(&self) -> anyhow::Result<ProviderType> {
        println!(
            "{} {}",
            style("Step 1 of 4:").bold().cyan(),
            style("Choose your AI provider").bold()
        );
        println!();

        let items: Vec<String> = ONBOARDING_PROVIDERS
            .iter()
            .map(|pt| {
                let info = get_provider_info(*pt);
                format!(
                    "{:<25} {}",
                    info.display_name,
                    style(info.description).dim()
                )
            })
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a provider")
            .items(&items)
            .default(0)
            .interact()?;

        println!();
        Ok(ONBOARDING_PROVIDERS[selection])
    }

    fn input_api_key(&self, provider_info: &ProviderInfo) -> anyhow::Result<String> {
        println!(
            "{} {}",
            style("Step 2 of 4:").bold().cyan(),
            style("Enter your API key").bold()
        );
        println!();
        println!(
            "  Get your API key at: {}",
            style(provider_info.signup_url).cyan().underlined()
        );
        println!();
        println!(
            "  {}",
            style(format!(
                "Tip: You can also set the {} environment variable.",
                provider_info.env_var
            ))
            .dim()
        );
        println!();

        let api_key: String = Password::with_theme(&ColorfulTheme::default())
            .with_prompt(provider_info.env_var.to_string())
            .interact()?;

        println!();
        Ok(api_key)
    }

    async fn select_model(
        &self,
        provider_type: ProviderType,
        api_key: Option<&str>,
    ) -> anyhow::Result<String> {
        println!(
            "{} {}",
            style("Step 3 of 4:").bold().cyan(),
            style("Choose model").bold()
        );
        println!();

        // Fetch available models from the provider API
        let models = self.fetch_model_list(provider_type, api_key).await;

        if models.is_empty() {
            // Fallback to manual input if no models fetched
            println!(
                "  {}",
                style("Could not fetch models. Please enter model name manually.").yellow()
            );
            println!();

            let default_model = provider_type.default_model();
            let model: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Model name")
                .default(default_model.to_string())
                .interact_text()?;

            println!();
            return Ok(model);
        }

        // Build selection list
        let items: Vec<String> = models
            .iter()
            .map(|m| {
                let name = m.name.as_deref().unwrap_or(&m.id);
                if m.recommended {
                    format!("{} {}", name, style("(recommended)").green())
                } else {
                    name.to_string()
                }
            })
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a model")
            .items(&items)
            .default(0)
            .interact()?;

        let model = models[selection].id.clone();
        println!();
        Ok(model)
    }

    async fn fetch_model_list(
        &self,
        provider_type: ProviderType,
        api_key: Option<&str>,
    ) -> Vec<ModelInfo> {
        let Some(key) = api_key else {
            // For Ollama, try to fetch without key
            if provider_type == ProviderType::Ollama {
                match fetch_models(provider_type, "").await {
                    Ok(models) => return models,
                    Err(_) => return Vec::new(),
                }
            }
            return Vec::new();
        };

        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        spinner.set_message("Fetching available models...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let result = fetch_models(provider_type, key).await;
        spinner.finish_and_clear();

        match result {
            Ok(models) => {
                println!(
                    "  {} Found {} models",
                    style("✓").green(),
                    models.len()
                );
                println!();
                models
            }
            Err(e) => {
                println!(
                    "  {} Could not fetch models: {}",
                    style("⚠").yellow(),
                    style(e.to_string()).dim()
                );
                println!();
                Vec::new()
            }
        }
    }

    async fn test_connection(
        &self,
        provider_type: ProviderType,
        api_key: &str,
        model: &str,
    ) -> anyhow::Result<bool> {
        println!(
            "{} {}",
            style("Step 4 of 4:").bold().cyan(),
            style("Testing connection").bold()
        );
        println!();

        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        spinner.set_message("Connecting to API...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        // Create provider and make test call
        let provider = GenAIProvider::with_api_key(provider_type, api_key, Some(model));

        let test_messages = vec![cowork_core::provider::LlmMessage::user("Say 'hello' in one word.")];

        let result = provider.chat(test_messages, None).await;

        spinner.finish_and_clear();

        match result {
            Ok(_) => {
                println!(
                    "  {} {}",
                    style("✓").green().bold(),
                    style("Connection successful!").green()
                );
                println!();
                Ok(true)
            }
            Err(e) => {
                println!(
                    "  {} {}",
                    style("✗").red().bold(),
                    style("Connection failed").red()
                );
                println!("  {}", style(format!("Error: {}", e)).dim());
                println!();

                let options = vec![
                    "Try again with different API key",
                    "Continue anyway (save current settings)",
                    "Exit setup",
                ];

                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("What would you like to do?")
                    .items(&options)
                    .default(0)
                    .interact()?;

                match selection {
                    0 => {
                        // Recursively try again - user will re-enter key
                        println!();
                        Ok(false) // Signal to restart
                    }
                    1 => {
                        println!();
                        Ok(true) // Continue anyway
                    }
                    _ => {
                        println!();
                        println!("{}", style("Setup cancelled.").yellow());
                        std::process::exit(0);
                    }
                }
            }
        }
    }

    fn save_config(
        &mut self,
        provider_type: ProviderType,
        provider_info: &ProviderInfo,
        api_key: Option<&str>,
        model: &str,
    ) -> anyhow::Result<()> {
        // Update or create provider config
        let provider_name = provider_info.name;

        let mut provider_config = self
            .config_manager
            .config()
            .providers
            .get(provider_name)
            .cloned()
            .unwrap_or_else(|| match provider_type {
                ProviderType::Anthropic => ProviderConfig::anthropic(),
                ProviderType::OpenAI => ProviderConfig::openai(),
                ProviderType::Gemini => ProviderConfig::gemini(),
                ProviderType::Groq => ProviderConfig::groq(),
                ProviderType::DeepSeek => ProviderConfig::deepseek(),
                _ => ProviderConfig::anthropic(),
            });

        provider_config.model = model.to_string();
        if let Some(key) = api_key {
            provider_config.api_key = Some(key.to_string());
        }

        // Set provider and make it default
        self.config_manager
            .set_provider(provider_name, provider_config);
        self.config_manager.set_default_provider(provider_name);

        // Save to disk
        self.config_manager.save()?;

        Ok(())
    }

    fn show_completion(&self, provider_info: &ProviderInfo) {
        println!();
        println!(
            "{}",
            style("┌─────────────────────────────────────────────────────┐").green()
        );
        println!(
            "{}",
            style("│                                                     │").green()
        );
        println!(
            "{}  {}  {}",
            style("│").green(),
            style("Setup Complete!").bold().white(),
            style("                              │").green()
        );
        println!(
            "{}",
            style("│                                                     │").green()
        );
        println!(
            "{}",
            style("└─────────────────────────────────────────────────────┘").green()
        );
        println!();

        println!("{}", style("Configuration saved to:").bold());
        println!(
            "  {}",
            style(format!(
                "{}",
                self.config_manager.config_path().display()
            ))
            .cyan()
        );
        println!();

        println!("{}", style("Your setup:").bold());
        println!(
            "  Provider: {}",
            style(provider_info.display_name).green()
        );
        println!(
            "  Model:    {}",
            style(&self.config_manager.config().providers.get(provider_info.name)
                .map(|p| p.model.as_str())
                .unwrap_or("default")).green()
        );
        println!();

        println!("{}", style("Quick Start Tips:").bold());
        println!("  {} - Start chatting with AI", style("cowork").cyan());
        println!(
            "  {} - Create a git commit",
            style("/commit").cyan()
        );
        println!("  {} - Show available commands", style("/help").cyan());
        println!();

        println!(
            "{}",
            style("Type 'help' for commands or just start chatting!").dim()
        );
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_info() {
        let info = get_provider_info(ProviderType::Anthropic);
        assert_eq!(info.name, "anthropic");
        assert_eq!(info.env_var, "ANTHROPIC_API_KEY");
    }
}
