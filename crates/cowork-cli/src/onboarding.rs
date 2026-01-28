//! Onboarding wizard for first-run setup
//!
//! Guides new users through provider selection, API key configuration,
//! and initial setup.

use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Password, Select};

use cowork_core::config::{ConfigManager, ProviderConfig, WebSearchConfig};
use cowork_core::provider::{catalog, GenAIProvider};
use cowork_core::tools::web::supports_native_search;

/// Provider information for configuration and display
pub struct ProviderInfo {
    pub name: &'static str,
    pub display_name: &'static str,
    pub signup_url: &'static str,
    pub env_var: &'static str,
    /// Default model ID (balanced tier from catalog)
    pub default_model: &'static str,
    /// Default base URL (from catalog, can be overridden in config)
    pub base_url: &'static str,
}

/// Provider display metadata for onboarding UI
struct ProviderDisplay {
    id: &'static str,
    display_name: &'static str,
    description: &'static str,
    signup_url: &'static str,
}

/// Display metadata for providers shown in onboarding
const PROVIDER_DISPLAYS: &[ProviderDisplay] = &[
    ProviderDisplay {
        id: "anthropic",
        display_name: "Anthropic (Claude)",
        description: "Best for code, writing, and reasoning",
        signup_url: "https://console.anthropic.com/",
    },
    ProviderDisplay {
        id: "openai",
        display_name: "OpenAI (GPT-5)",
        description: "Versatile and widely supported",
        signup_url: "https://platform.openai.com/",
    },
    ProviderDisplay {
        id: "gemini",
        display_name: "Google Gemini",
        description: "Large context window (1M tokens)",
        signup_url: "https://aistudio.google.com/",
    },
    ProviderDisplay {
        id: "groq",
        display_name: "Groq",
        description: "Ultra-fast inference",
        signup_url: "https://console.groq.com/",
    },
    ProviderDisplay {
        id: "deepseek",
        display_name: "DeepSeek",
        description: "Cost-effective reasoning",
        signup_url: "https://platform.deepseek.com/",
    },
    ProviderDisplay {
        id: "xai",
        display_name: "xAI (Grok)",
        description: "Latest Grok models",
        signup_url: "https://x.ai/api",
    },
    ProviderDisplay {
        id: "together",
        display_name: "Together AI",
        description: "200+ open source models",
        signup_url: "https://api.together.xyz/",
    },
    ProviderDisplay {
        id: "fireworks",
        display_name: "Fireworks AI",
        description: "Fast open source model inference",
        signup_url: "https://fireworks.ai/",
    },
    ProviderDisplay {
        id: "zai",
        display_name: "Zai (Zhipu AI)",
        description: "GLM-4 models from China",
        signup_url: "https://z.ai/",
    },
    ProviderDisplay {
        id: "nebius",
        display_name: "Nebius AI Studio",
        description: "30+ open source models",
        signup_url: "https://studio.nebius.ai/",
    },
    ProviderDisplay {
        id: "mimo",
        display_name: "MIMO (Xiaomi)",
        description: "Xiaomi's MIMO models",
        signup_url: "https://xiaomimimo.com/",
    },
    ProviderDisplay {
        id: "bigmodel",
        display_name: "BigModel.cn",
        description: "Zhipu AI China platform",
        signup_url: "https://open.bigmodel.cn/",
    },
    ProviderDisplay {
        id: "ollama",
        display_name: "Ollama (Local)",
        description: "Run models locally, no API key needed",
        signup_url: "https://ollama.ai/",
    },
];

/// Get provider info for a provider ID
pub fn get_provider_info(provider_id: &str) -> ProviderInfo {
    // Find display metadata
    let display = PROVIDER_DISPLAYS
        .iter()
        .find(|p| p.id == provider_id)
        .unwrap_or(&ProviderDisplay {
            id: "unknown",
            display_name: "Unknown Provider",
            description: "Custom provider configuration",
            signup_url: "",
        });

    ProviderInfo {
        name: display.id,
        display_name: display.display_name,
        signup_url: display.signup_url,
        env_var: catalog::api_key_env(provider_id).unwrap_or("API_KEY"),
        default_model: catalog::default_model(provider_id).unwrap_or(""),
        base_url: catalog::base_url(provider_id).unwrap_or(""),
    }
}

/// All supported provider IDs for onboarding (order matches PROVIDER_DISPLAYS)
const ONBOARDING_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "gemini",
    "groq",
    "deepseek",
    "xai",
    "together",
    "fireworks",
    "zai",
    "nebius",
    "mimo",
    "bigmodel",
    "ollama",
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
        let provider_id = self.select_provider()?;
        let provider_info = get_provider_info(provider_id);
        let model = provider_info.default_model;

        // Loop for API key retry
        loop {
            // Step 2: API key input (skip for Ollama)
            let api_key = if provider_id == "ollama" {
                None
            } else {
                Some(self.input_api_key(&provider_info)?)
            };

            // Step 3: Connection test (skip for Ollama)
            if let Some(ref key) = api_key
                && !self.test_connection(provider_id, key, model).await? {
                    // User chose to try again - loop back to step 2
                    println!("{}", style("Let's try again...").dim());
                    println!();
                    continue;
                }

            // Save configuration
            self.save_config(&provider_info, api_key.as_deref())?;

            // Optional: SerpAPI key for providers without native web search
            if !supports_native_search(provider_info.name) {
                self.offer_serpapi_setup()?;
            }

            // Show completion
            self.show_completion(&provider_info);

            break;
        }

        Ok(())
    }

    /// Offer to set up SerpAPI for web search (for providers without native search)
    fn offer_serpapi_setup(&mut self) -> anyhow::Result<()> {
        println!();
        println!(
            "{} {}",
            style("Optional:").bold().yellow(),
            style("Web Search Setup").bold()
        );
        println!();
        println!(
            "  {}",
            style("Your chosen provider doesn't have built-in web search.").dim()
        );
        println!(
            "  {}",
            style("You can add SerpAPI for web search capabilities.").dim()
        );
        println!();

        let setup = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Would you like to set up SerpAPI for web search?")
            .default(false)
            .interact()?;

        if !setup {
            println!();
            println!(
                "  {}",
                style("Skipped. You can add it later in the config file.").dim()
            );
            return Ok(());
        }

        println!();
        println!(
            "  Get your API key at: {}",
            style("https://serpapi.com/").cyan().underlined()
        );
        println!();

        let api_key: String = Password::with_theme(&ColorfulTheme::default())
            .with_prompt("SERPAPI_API_KEY")
            .interact()?;

        if !api_key.is_empty() {
            // Update web search config with SerpAPI key
            self.config_manager.config_mut().web_search = WebSearchConfig {
                api_key: Some(api_key),
                ..Default::default()
            };
            self.config_manager.save()?;

            println!();
            println!(
                "  {} {}",
                style("✓").green().bold(),
                style("SerpAPI configured!").green()
            );
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

    fn select_provider(&self) -> anyhow::Result<&'static str> {
        println!(
            "{} {}",
            style("Step 1 of 3:").bold().cyan(),
            style("Choose your AI provider").bold()
        );
        println!();

        let items: Vec<String> = PROVIDER_DISPLAYS
            .iter()
            .map(|p| {
                format!(
                    "{:<25} {}",
                    p.display_name,
                    style(p.description).dim()
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
            style("Step 2 of 3:").bold().cyan(),
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

    async fn test_connection(
        &self,
        provider_id: &str,
        api_key: &str,
        model: &str,
    ) -> anyhow::Result<bool> {
        println!(
            "{} {}",
            style("Step 3 of 3:").bold().cyan(),
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
        let provider = match GenAIProvider::with_api_key(provider_id, api_key, Some(model)) {
            Ok(p) => p,
            Err(e) => {
                spinner.finish_and_clear();
                return Err(e.into());
            }
        };

        let test_messages = vec![cowork_core::provider::ChatMessage::user("Say 'hello' in one word.")];

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
        provider_info: &ProviderInfo,
        api_key: Option<&str>,
    ) -> anyhow::Result<()> {
        // Update or create provider config
        let provider_name = provider_info.name;

        let mut provider_config = self
            .config_manager
            .config()
            .providers
            .get(provider_name)
            .cloned()
            .unwrap_or_else(|| ProviderConfig::for_provider(provider_name));

        provider_config.model = provider_info.default_model.to_string();
        if let Some(key) = api_key {
            provider_config.api_key = Some(key.to_string());
        }
        // base_url defaults to None (uses the provider's default endpoint).
        // Pro users can set a custom base_url in the config file.

        // Set provider and make it default
        self.config_manager
            .set_provider(provider_name, provider_config);
        self.config_manager.set_default_provider(provider_name);

        // Save to disk (ConfigManager adds sample config comments for new files)
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
            style(provider_info.default_model).green()
        );
        println!(
            "  Base URL: {}",
            style(provider_info.base_url).dim()
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

        println!("{}", style("Advanced Configuration:").bold());
        println!(
            "  Edit the config file to add MCP servers, skills, and more."
        );
        println!(
            "  Config file: {}",
            style(format!("{}", self.config_manager.config_path().display())).cyan()
        );
        println!(
            "  {}",
            style("The config file contains sample configuration with comments.").dim()
        );
        println!();

        println!(
            "{}",
            style("Type 'help' for commands or just start chatting!").dim()
        );
        println!();

        // Wait for user to read the info before starting TUI
        println!(
            "{}",
            style("Press Enter to start...").bold().cyan()
        );
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_info() {
        let info = get_provider_info("anthropic");
        assert_eq!(info.name, "anthropic");
        assert_eq!(info.env_var, "ANTHROPIC_API_KEY");
    }
}
