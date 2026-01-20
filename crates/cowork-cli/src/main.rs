//! Cowork CLI - Multi-agent assistant command line tool

mod onboarding;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use console::style;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect, Select};
use indicatif::{ProgressBar, ProgressStyle};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Config, Editor, Helper};

use onboarding::OnboardingWizard;

use cowork_core::config::ConfigManager;
use cowork_core::mcp_manager::McpServerManager;
use cowork_core::provider::{
    create_provider_from_config, get_api_key, get_model_tiers, has_api_key_configured,
    CompletionResult, GenAIProvider, LlmMessage, ProviderType,
};
use cowork_core::orchestration::{create_standard_tool_registry, format_size, format_tool_result, SystemPrompt};
use cowork_core::ToolApprovalConfig;
use cowork_core::skills::SkillRegistry;
// Only import tools that are used directly (for quick commands like /ls, /read, /search)
use cowork_core::tools::filesystem::{ListDirectory, ReadFile, SearchFiles};
use cowork_core::tools::shell::ExecuteCommand;
use cowork_core::tools::{Tool, ToolDefinition, ToolRegistry};

/// Slash command completer for readline
#[derive(Default)]
struct SlashCompleter {
    commands: Vec<(&'static str, &'static str)>,
}

impl SlashCompleter {
    fn new() -> Self {
        Self {
            commands: vec![
                ("/help", "Show help"),
                ("/exit", "Exit the program"),
                ("/quit", "Exit the program"),
                ("/clear", "Clear conversation history"),
                ("/tools", "Show available tools"),
                ("/ls", "List directory contents"),
                ("/read", "Read file contents"),
                ("/run", "Run a shell command"),
                ("/search", "Search for files"),
                ("/commit", "Create a git commit"),
                ("/push", "Push to remote"),
                ("/pr", "Create a pull request"),
                ("/review", "Review staged changes"),
                ("/clean-gone", "Clean up deleted branches"),
            ],
        }
    }
}

impl Completer for SlashCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Only complete if line starts with /
        if !line.starts_with('/') {
            return Ok((0, vec![]));
        }

        let input = &line[..pos];
        let matches: Vec<Pair> = self
            .commands
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(input))
            .map(|(cmd, desc)| Pair {
                display: format!("{} - {}", cmd, desc),
                replacement: cmd.to_string(),
            })
            .collect();

        Ok((0, matches))
    }
}

impl Hinter for SlashCompleter {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        if !line.starts_with('/') || pos < line.len() {
            return None;
        }

        // If just "/" typed, show hint to press Tab
        if line == "/" {
            return Some(" <Tab> for commands".to_string());
        }

        // Find first matching command and show hint for typeahead
        self.commands
            .iter()
            .find(|(cmd, _)| cmd.starts_with(line) && *cmd != line)
            .map(|(cmd, _)| cmd[line.len()..].to_string())
    }
}

impl Highlighter for SlashCompleter {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("\x1b[90m{}\x1b[0m", hint))
    }
}

impl Validator for SlashCompleter {}
impl Helper for SlashCompleter {}

#[derive(Parser)]
#[command(name = "cowork")]
#[command(author = "Cowork Team")]
#[command(version = "0.1.0")]
#[command(about = "Multi-agent AI assistant for desktop automation", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Workspace directory
    #[arg(short, long, default_value = ".")]
    workspace: PathBuf,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// LLM Provider (anthropic, openai, deepseek, etc.) - defaults to config setting
    #[arg(short, long)]
    provider: Option<String>,

    /// Model to use (defaults to provider's default)
    #[arg(short, long)]
    model: Option<String>,

    /// Auto-approve all tool calls (use with caution!)
    #[arg(long)]
    auto_approve: bool,

    /// Execute a single prompt and exit (non-interactive mode)
    #[arg(long)]
    one_shot: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive chat mode
    Chat,

    /// Execute a shell command
    Run {
        /// Command to execute
        command: String,
    },

    /// List files in workspace
    List {
        /// Path to list
        #[arg(default_value = ".")]
        path: String,
    },

    /// Read a file
    Read {
        /// File path
        path: String,
    },

    /// Search for files
    Search {
        /// Search pattern
        pattern: String,

        /// Search in file contents
        #[arg(short, long)]
        content: bool,
    },

    /// Show available tools
    Tools,

    /// Show configuration
    Config,
}

/// Parse provider name string to ProviderType
fn parse_provider_type(provider_str: &str) -> ProviderType {
    provider_str.parse::<ProviderType>().unwrap_or_else(|_| {
        eprintln!("Warning: Unknown provider '{}', defaulting to Anthropic", provider_str);
        ProviderType::Anthropic
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Setup logging
    tracing_subscriber::fmt()
        .with_env_filter(if cli.verbose {
            "debug"
        } else {
            "info"
        })
        .init();

    // Use dunce::canonicalize to avoid UNC path prefix on Windows (\\?\)
    // If canonicalize fails, ensure we at least have an absolute path
    let workspace = dunce::canonicalize(&cli.workspace).unwrap_or_else(|_| {
        if cli.workspace.is_absolute() {
            cli.workspace.clone()
        } else {
            // Make relative path absolute using current directory
            std::env::current_dir()
                .map(|cwd| cwd.join(&cli.workspace))
                .unwrap_or(cli.workspace.clone())
        }
    });

    // Load config to get default provider
    let config_manager = ConfigManager::new().ok();

    // Determine provider: CLI arg > config default > fallback to Anthropic
    let provider_str = cli.provider.clone().unwrap_or_else(|| {
        config_manager
            .as_ref()
            .map(|cm| cm.default_provider().to_string())
            .unwrap_or_else(|| "anthropic".to_string())
    });

    // Parse provider type
    let provider_type = parse_provider_type(&provider_str);

    // Handle one-shot mode
    if let Some(prompt) = cli.one_shot {
        return run_one_shot(&workspace, provider_type, cli.model.as_deref(), &prompt, cli.auto_approve).await;
    }

    match cli.command {
        Some(Commands::Chat) => run_chat(&workspace, provider_type, cli.model.as_deref(), cli.auto_approve).await?,
        Some(Commands::Run { command }) => run_command(&workspace, &command).await?,
        Some(Commands::List { path }) => list_files(&workspace, &path).await?,
        Some(Commands::Read { path }) => read_file(&workspace, &path).await?,
        Some(Commands::Search { pattern, content }) => {
            search_files(&workspace, &pattern, content).await?
        }
        Some(Commands::Tools) => show_tools(),
        Some(Commands::Config) => show_config(&workspace),
        None => run_chat(&workspace, provider_type, cli.model.as_deref(), cli.auto_approve).await?,
    }

    Ok(())
}

/// Run a single prompt non-interactively (for scripting/testing)
async fn run_one_shot(
    workspace: &Path,
    provider_type: ProviderType,
    model: Option<&str>,
    prompt: &str,
    auto_approve: bool,
) -> anyhow::Result<()> {
    // Load config
    let config_manager = ConfigManager::new()?;

    // Create system prompt
    let system_prompt = SystemPrompt::new()
        .with_workspace_context(workspace)
        .build();

    // Create provider from config or environment
    let provider = create_provider_from_config(&config_manager, provider_type, model)?
        .with_system_prompt(&system_prompt);

    // Get API key and model tiers for subagents
    let api_key = get_api_key(&config_manager, provider_type);
    let model_tiers = get_model_tiers(&config_manager, provider_type);

    // Create tool registry with API key and model tiers for subagent execution
    let tool_registry = create_tool_registry(workspace, provider_type, api_key.as_deref(), Some(model_tiers));
    let tool_definitions = tool_registry.list();

    // Chat history
    let mut messages: Vec<LlmMessage> = Vec::new();

    // Tool approval configuration
    let mut approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Process the single message
    process_ai_message(
        prompt,
        &provider,
        &tool_registry,
        &tool_definitions,
        &mut messages,
        &mut approval_config,
    )
    .await?;

    Ok(())
}

async fn run_chat(
    workspace: &Path,
    cli_provider_type: ProviderType,
    model: Option<&str>,
    auto_approve: bool,
) -> anyhow::Result<()> {
    // Load config
    let mut config_manager = ConfigManager::new()?;

    // Check if onboarding wizard should run (first-run detection)
    let mut wizard = OnboardingWizard::new(config_manager);
    let ran_wizard = wizard.should_run();
    if ran_wizard {
        wizard.run().await?;
    }
    config_manager = wizard.into_config_manager();

    // After wizard, re-read provider from config (wizard may have changed it)
    let provider_type = if ran_wizard {
        // Use the provider that was just configured
        parse_provider_type(config_manager.default_provider())
    } else {
        // Use CLI argument or config default
        cli_provider_type
    };

    println!("{}", style("Cowork - AI Coding Assistant").bold().cyan());
    println!(
        "{}",
        style(format!("Provider: {:?}", provider_type)).dim()
    );
    if auto_approve {
        println!(
            "{}",
            style("Warning: Auto-approve mode is ON - all tool calls will be approved automatically!").yellow().bold()
        );
    }
    println!();

    // Check if API key is configured - show setup instructions if not
    if !has_api_key_configured(&config_manager, provider_type) {
        show_setup_instructions(provider_type);
        return Ok(());
    }

    println!(
        "{}",
        style("Type 'help' for commands, 'exit' to quit, or just chat with the AI").dim()
    );
    println!();

    // Initialize MCP server manager (servers start lazily when tools are called)
    let mcp_manager = std::sync::Arc::new(
        McpServerManager::with_configs(config_manager.config().mcp_servers.clone())
    );

    // Create system prompt with workspace context
    let system_prompt = SystemPrompt::new()
        .with_workspace_context(workspace)
        .build();

    // Create provider from config or environment
    let provider = match create_provider_from_config(&config_manager, provider_type, model) {
        Ok(p) => p.with_system_prompt(&system_prompt),
        Err(e) => {
            println!(
                "{}",
                style(format!("Warning: {}. The AI may not work.", e)).yellow()
            );
            println!();
            GenAIProvider::new(provider_type, model).with_system_prompt(&system_prompt)
        }
    };

    // Get API key and model tiers for subagents
    let api_key = get_api_key(&config_manager, provider_type);
    let model_tiers = get_model_tiers(&config_manager, provider_type);

    // Create tool registry with API key and model tiers for subagent execution
    let tool_registry = create_tool_registry(workspace, provider_type, api_key.as_deref(), Some(model_tiers));
    let tool_definitions = tool_registry.list();

    // Create skill registry for slash commands with MCP manager
    let skill_registry = SkillRegistry::with_builtins_and_mcp(workspace.to_path_buf(), Some(mcp_manager));

    // Chat history
    let mut messages: Vec<LlmMessage> = Vec::new();

    // Tool approval configuration (shared with UI)
    let mut approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Set up readline with history and slash command completion
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(rustyline::CompletionType::List)
        .edit_mode(rustyline::EditMode::Emacs)
        .auto_add_history(false) // We add manually
        .build();
    let mut rl = Editor::with_config(config)?;
    rl.set_helper(Some(SlashCompleter::new()));

    // Load history from file
    let history_path = directories::ProjectDirs::from("", "", "cowork")
        .map(|p| p.config_dir().join("history.txt"))
        .unwrap_or_else(|| PathBuf::from(".cowork_history"));
    let _ = rl.load_history(&history_path);

    // Use simple prompt without ANSI to avoid cursor position issues
    let prompt = "You> ";

    loop {
        let readline = rl.readline(prompt);
        let input = match readline {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                println!("{}", style("Use /exit to quit").dim());
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("{}", style("Goodbye!").green());
                break;
            }
            Err(err) => {
                println!("{}", style(format!("Error: {}", err)).red());
                continue;
            }
        };

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Add to history
        let _ = rl.add_history_entry(input);

        match input {
            "/exit" | "/quit" | "/q" => {
                println!("{}", style("Goodbye!").green());
                break;
            }
            "/help" | "/?" => {
                print_help();
            }
            "/tools" => {
                show_tools();
            }
            "/clear" => {
                messages.clear();
                println!("{}", style("Conversation cleared.").green());
            }
            cmd if cmd.starts_with("/run ") => {
                let command = &cmd[5..];
                run_command(workspace, command).await?;
            }
            cmd if cmd.starts_with("/ls ") || cmd.starts_with("/list ") => {
                let path = cmd.split_whitespace().nth(1).unwrap_or(".");
                list_files(workspace, path).await?;
            }
            "/ls" | "/list" => {
                list_files(workspace, ".").await?;
            }
            cmd if cmd.starts_with("/cat ") || cmd.starts_with("/read ") => {
                let path = cmd.split_whitespace().nth(1).unwrap_or("");
                if path.is_empty() {
                    println!("{}", style("Usage: /read <file>").yellow());
                } else {
                    read_file(workspace, path).await?;
                }
            }
            cmd if cmd.starts_with("/search ") || cmd.starts_with("/find ") => {
                let pattern = &cmd[cmd.find(' ').unwrap_or(0) + 1..];
                search_files(workspace, pattern, false).await?;
            }
            cmd if cmd.starts_with('/') => {
                // Handle slash commands via skill registry
                handle_slash_command(cmd, workspace, &skill_registry).await;
            }
            _ => {
                // Process with AI
                process_ai_message(
                    input,
                    &provider,
                    &tool_registry,
                    &tool_definitions,
                    &mut messages,
                    &mut approval_config,
                )
                .await?;
            }
        }

        println!();
    }

    // Save history on exit
    if let Some(parent) = history_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = rl.save_history(&history_path);

    Ok(())
}

/// Process a message through the AI
async fn process_ai_message(
    input: &str,
    provider: &GenAIProvider,
    tool_registry: &ToolRegistry,
    tool_definitions: &[ToolDefinition],
    messages: &mut Vec<LlmMessage>,
    approval_config: &mut ToolApprovalConfig,
) -> anyhow::Result<()> {
    // Add user message
    messages.push(LlmMessage {
        role: "user".to_string(),
        content: input.to_string(),
    });

    // Agentic loop - keep going until we get a text response (no more tool calls)
    loop {
        // Show spinner while waiting for AI
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        spinner.set_message("Thinking...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        // Get response from AI
        let result = provider
            .chat(messages.clone(), Some(tool_definitions.to_vec()))
            .await;

        spinner.finish_and_clear();

        match result {
            Ok(CompletionResult::Message(text)) => {
                // Got a text response - display it and we're done
                println!("{}: {}", style("Assistant").bold().green(), text);
                messages.push(LlmMessage {
                    role: "assistant".to_string(),
                    content: text,
                });
                break;
            }
            Ok(CompletionResult::ToolCalls(calls)) => {
                // AI wants to use tools
                println!(
                    "{}",
                    style(format!("AI wants to use {} tool(s)", calls.len())).cyan()
                );

                let mut tool_results = Vec::new();

                for call in &calls {
                    // Display tool call in a formatted box
                    println!();
                    println!("{}", style("┌─ Tool Call ─────────────────────────────────────").dim());
                    println!("│ {} {}", style("Tool:").bold(), style(&call.name).yellow().bold());

                    // Format arguments nicely
                    let args_str = serde_json::to_string_pretty(&call.arguments)
                        .unwrap_or_else(|_| call.arguments.to_string());
                    for (i, line) in args_str.lines().enumerate() {
                        if i == 0 {
                            println!("│ {} {}", style("Args:").bold(), line);
                        } else {
                            println!("│       {}", line);
                        }
                    }
                    println!("{}", style("└─────────────────────────────────────────────────").dim());

                    // Determine approval status using shared approval config
                    let approved = if approval_config.should_auto_approve(&call.name) {
                        // Auto-approved (read-only or session approved)
                        println!("  {} {}", style("✓").green(), style("Auto-approved").dim());
                        true
                    } else if !approval_config.needs_approval(&call.name) {
                        // Configured as auto-approve
                        println!("  {} {}", style("✓").green(), style("Auto-approved (configured)").dim());
                        true
                    } else {
                        // Need user approval - show options
                        let options: Vec<String> = vec![
                            "Yes - approve this call".to_string(),
                            "No - reject this call".to_string(),
                            format!("Always - auto-approve '{}' for session", call.name),
                            "Approve all - auto-approve everything for session".to_string(),
                        ];

                        let selection = Select::with_theme(&ColorfulTheme::default())
                            .with_prompt("Approve?")
                            .items(&options)
                            .default(0)
                            .interact()?;

                        match selection {
                            0 => true,  // Yes
                            1 => false, // No
                            2 => {
                                // Always - add to session approved
                                approval_config.approve_for_session(call.name.clone());
                                println!("  {} '{}' will be auto-approved for this session",
                                    style("✓").green(), call.name);
                                true
                            }
                            3 => {
                                // Approve all
                                approval_config.approve_all_for_session();
                                println!("  {} All tools will be auto-approved for this session",
                                    style("✓").green());
                                true
                            }
                            _ => false,
                        }
                    };

                    if approved {
                        // Special handling for ask_user_question - intercept and handle interactively
                        if call.name == "ask_user_question" {
                            match handle_ask_user_question(&call.arguments) {
                                Ok(result_str) => {
                                    println!("  {} {}", style("✓").green(), style("User answered questions").dim());
                                    tool_results.push((call.name.clone(), result_str, true));
                                }
                                Err(e) => {
                                    let error_msg = format!("Error: {}", e);
                                    println!("  {}", style(&error_msg).red());
                                    tool_results.push((call.name.clone(), error_msg, false));
                                }
                            }
                            continue;
                        }

                        // Execute tool
                        let exec_spinner = ProgressBar::new_spinner();
                        exec_spinner.set_style(
                            ProgressStyle::default_spinner()
                                .template("{spinner:.blue} Executing...")
                                .unwrap(),
                        );
                        exec_spinner.enable_steady_tick(std::time::Duration::from_millis(100));

                        if let Some(tool) = tool_registry.get(&call.name) {
                            match tool.execute(call.arguments.clone()).await {
                                Ok(output) => {
                                    exec_spinner.finish_and_clear();
                                    let result_str = output.content.to_string();
                                    let formatted = format_tool_result(&call.name, &result_str);
                                    println!("  {}", style("Result:").bold().green());
                                    for line in formatted.lines() {
                                        println!("    {}", line);
                                    }

                                    tool_results.push((call.name.clone(), result_str, true));
                                }
                                Err(e) => {
                                    exec_spinner.finish_and_clear();
                                    let error_msg = format!("Error: {}", e);
                                    println!("  {}", style(&error_msg).red());
                                    tool_results.push((call.name.clone(), error_msg, false));
                                }
                            }
                        } else {
                            exec_spinner.finish_and_clear();
                            let error_msg = format!("Unknown tool: {}", call.name);
                            println!("  {}", style(&error_msg).red());
                            tool_results.push((call.name.clone(), error_msg, false));
                        }
                    } else {
                        println!("  {}", style("✗ Rejected by user").yellow());
                        tool_results.push((
                            call.name.clone(),
                            "User rejected this tool call".to_string(),
                            false,
                        ));
                    }
                }

                // Add tool results to messages for context
                // Format as a user message with the tool execution results
                // This simulates the system reporting back what happened
                let results_summary: Vec<String> = tool_results
                    .iter()
                    .map(|(name, result, success)| {
                        if *success {
                            format!("[Tool '{}' executed successfully]\nResult: {}", name, result)
                        } else {
                            format!("[Tool '{}' failed]\nError: {}", name, result)
                        }
                    })
                    .collect();

                // Add as user message so the AI knows to continue with next steps
                messages.push(LlmMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Tool execution results:\n\n{}\n\nPlease continue with the next step of the task.",
                        results_summary.join("\n\n")
                    ),
                });

                // Continue the loop to let AI process tool results
            }
            Err(e) => {
                println!("{}", style(format!("Error: {}", e)).red());
                // Remove the last user message since the request failed
                messages.pop();
                break;
            }
        }
    }

    Ok(())
}


/// Handle ask_user_question tool call interactively in CLI
fn handle_ask_user_question(args: &serde_json::Value) -> Result<String, String> {
    let questions = args
        .get("questions")
        .and_then(|q| q.as_array())
        .ok_or_else(|| "Missing questions array".to_string())?;

    let mut answers: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();

    for (i, question) in questions.iter().enumerate() {
        let question_text = question
            .get("question")
            .and_then(|q| q.as_str())
            .unwrap_or("Question");
        let header = question
            .get("header")
            .and_then(|h| h.as_str())
            .unwrap_or("");
        let multi_select = question
            .get("multiSelect")
            .and_then(|m| m.as_bool())
            .unwrap_or(false);
        let options = question
            .get("options")
            .and_then(|o| o.as_array())
            .ok_or_else(|| format!("Question {} missing options", i + 1))?;

        // Build display items with label and description
        let mut items: Vec<String> = options
            .iter()
            .map(|opt| {
                let label = opt.get("label").and_then(|l| l.as_str()).unwrap_or("");
                let desc = opt.get("description").and_then(|d| d.as_str()).unwrap_or("");
                if desc.is_empty() {
                    label.to_string()
                } else {
                    format!("{} - {}", label, style(desc).dim())
                }
            })
            .collect();

        // Add "Other" option
        items.push(format!("{}", style("Other (type custom answer)").italic()));

        // Display the question
        println!();
        println!("{}", style(format!("┌─ {} ─────────────────────────────────────", header)).cyan());
        println!("│ {}", style(question_text).bold());
        println!("{}", style("└─────────────────────────────────────────────────").cyan());

        if multi_select {
            // Multi-select mode
            let selections = MultiSelect::with_theme(&ColorfulTheme::default())
                .items(&items)
                .interact()
                .map_err(|e| format!("Selection failed: {}", e))?;

            let mut selected_labels: Vec<String> = Vec::new();
            for idx in selections {
                if idx == items.len() - 1 {
                    // "Other" selected - prompt for custom input
                    let custom: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Enter your custom answer")
                        .interact_text()
                        .map_err(|e| format!("Input failed: {}", e))?;
                    selected_labels.push(custom);
                } else {
                    // Get the original label (not the formatted display string)
                    let label = options[idx]
                        .get("label")
                        .and_then(|l| l.as_str())
                        .unwrap_or("")
                        .to_string();
                    selected_labels.push(label);
                }
            }
            answers.insert(i.to_string(), serde_json::json!(selected_labels.join(", ")));
        } else {
            // Single select mode
            let selection = Select::with_theme(&ColorfulTheme::default())
                .items(&items)
                .default(0)
                .interact()
                .map_err(|e| format!("Selection failed: {}", e))?;

            let answer = if selection == items.len() - 1 {
                // "Other" selected - prompt for custom input
                let custom: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter your custom answer")
                    .interact_text()
                    .map_err(|e| format!("Input failed: {}", e))?;
                custom
            } else {
                // Get the original label
                options[selection]
                    .get("label")
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            answers.insert(i.to_string(), serde_json::json!(answer));
        }
    }

    // Return the answers as JSON
    let result = serde_json::json!({
        "answered": true,
        "answers": answers
    });

    serde_json::to_string(&result).map_err(|e| format!("JSON serialization failed: {}", e))
}

/// Handle slash commands
async fn handle_slash_command(cmd: &str, workspace: &Path, registry: &SkillRegistry) {
    let result = registry.execute_command(cmd, workspace.to_path_buf()).await;
    if result.success {
        println!("{}", result.response);
    } else {
        println!(
            "{}",
            style(format!("Error: {}", result.error.unwrap_or_default())).red()
        );
    }
}

/// Create tool registry with all available tools
/// Uses the shared tool registry factory from cowork-core
fn create_tool_registry(
    workspace: &Path,
    provider_type: ProviderType,
    api_key: Option<&str>,
    model_tiers: Option<cowork_core::config::ModelTiers>,
) -> ToolRegistry {
    create_standard_tool_registry(workspace, provider_type, api_key, model_tiers)
}

fn print_help() {
    println!("{}", style("Built-in Commands:").bold());
    println!("  {}       - Show this help", style("/help").green());
    println!("  {}       - Exit the program", style("/exit").green());
    println!("  {}      - Clear conversation history", style("/clear").green());
    println!("  {}      - Show available tools", style("/tools").green());
    println!();
    println!("{}", style("Quick Commands:").bold());
    println!("  {}     - Run a shell command", style("/run <cmd>").green());
    println!(
        "  {}   - List directory contents",
        style("/ls [path]").green()
    );
    println!("  {} - Read file contents", style("/read <file>").green());
    println!(
        "  {} - Search for files",
        style("/search <pattern>").green()
    );
    println!();
    println!("{}", style("Slash Commands (Skills):").bold());
    println!("  {}      - Create a git commit", style("/commit").green());
    println!("  {}        - Push to remote", style("/push").green());
    println!(
        "  {}   - Create a pull request",
        style("/pr [title]").green()
    );
    println!(
        "  {}      - Review staged changes",
        style("/review").green()
    );
    println!(
        "  {}  - Clean up deleted branches",
        style("/clean-gone").green()
    );
    println!();
    println!(
        "{}",
        style("Or just type what you want to do - the AI will help!").dim()
    );
}

async fn run_command(workspace: &Path, command: &str) -> anyhow::Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.blue} {msg}")
            .unwrap(),
    );
    spinner.set_message(format!("Running: {}", command));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let tool = ExecuteCommand::new(workspace.to_path_buf());
    let params = serde_json::json!({
        "command": command,
        "timeout": 30
    });

    let result = tool.execute(params).await;
    spinner.finish_and_clear();

    match result {
        Ok(output) => {
            if output.success {
                if let Some(stdout) = output.content.get("stdout") {
                    if let Some(s) = stdout.as_str() {
                        if !s.is_empty() {
                            println!("{}", s);
                        }
                    }
                }
                if let Some(stderr) = output.content.get("stderr") {
                    if let Some(s) = stderr.as_str() {
                        if !s.is_empty() {
                            eprintln!("{}", style(s).yellow());
                        }
                    }
                }
            } else {
                println!("{}", style("Command failed").red());
                if let Some(err) = output.error {
                    println!("{}", style(err).red());
                }
            }
        }
        Err(e) => {
            println!("{}", style(format!("Error: {}", e)).red());
        }
    }

    Ok(())
}

async fn list_files(workspace: &Path, path: &str) -> anyhow::Result<()> {
    let tool = ListDirectory::new(workspace.to_path_buf());
    let params = serde_json::json!({
        "path": path,
        "include_hidden": false
    });

    let result = tool.execute(params).await;

    match result {
        Ok(output) => {
            if let Some(entries) = output.content.get("entries") {
                if let Some(arr) = entries.as_array() {
                    for entry in arr {
                        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let is_dir = entry.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
                        let size = entry.get("size").and_then(|v| v.as_u64());

                        if is_dir {
                            println!("{}/", style(name).blue().bold());
                        } else {
                            let size_str = size
                                .map(format_size)
                                .unwrap_or_else(|| "-".to_string());
                            println!("{:<40} {}", name, style(size_str).dim());
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("{}", style(format!("Error: {}", e)).red());
        }
    }

    Ok(())
}

async fn read_file(workspace: &Path, path: &str) -> anyhow::Result<()> {
    let tool = ReadFile::new(workspace.to_path_buf());
    let params = serde_json::json!({
        "path": path
    });

    let result = tool.execute(params).await;

    match result {
        Ok(output) => {
            if let Some(content) = output.content.get("content") {
                if let Some(s) = content.as_str() {
                    println!("{}", s);
                }
            }
        }
        Err(e) => {
            println!("{}", style(format!("Error: {}", e)).red());
        }
    }

    Ok(())
}

async fn search_files(workspace: &Path, pattern: &str, in_content: bool) -> anyhow::Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.blue} Searching...")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let tool = SearchFiles::new(workspace.to_path_buf());
    let params = if in_content {
        serde_json::json!({
            "content": pattern,
            "max_results": 50
        })
    } else {
        serde_json::json!({
            "pattern": pattern,
            "max_results": 50
        })
    };

    let result = tool.execute(params).await;
    spinner.finish_and_clear();

    match result {
        Ok(output) => {
            if let Some(results) = output.content.get("results") {
                if let Some(arr) = results.as_array() {
                    if arr.is_empty() {
                        println!("{}", style("No matches found").yellow());
                    } else {
                        for entry in arr {
                            let path = entry.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            println!("{}", style(path).green());
                        }
                        println!();
                        println!("{}", style(format!("Found {} matches", arr.len())).dim());
                    }
                }
            }
        }
        Err(e) => {
            println!("{}", style(format!("Error: {}", e)).red());
        }
    }

    Ok(())
}

fn show_tools() {
    println!("{}", style("Available Tools:").bold());
    println!();

    let tools = [
        // Filesystem
        ("read_file", "Read file contents", "None"),
        ("write_file", "Create or overwrite a file", "High"),
        ("edit", "Surgical string replacement", "High"),
        ("glob", "Find files by pattern", "None"),
        ("grep", "Search file contents", "None"),
        ("list_directory", "List directory contents", "None"),
        ("search_files", "Search for files", "None"),
        ("delete_file", "Delete a file", "High"),
        ("move_file", "Move or rename files", "Low"),
        // Shell
        ("execute_command", "Run shell commands", "Medium"),
        // Web
        ("web_fetch", "Fetch URL content", "Low"),
        ("web_search", "Search the web", "Low"),
        // Notebook
        ("notebook_edit", "Edit Jupyter notebooks", "High"),
        // Task management
        ("todo_write", "Manage task list", "None"),
        // Code intelligence
        ("lsp", "Language Server Protocol", "None"),
        // Sub-agents
        ("task", "Launch subagent for complex tasks", "Low"),
        ("task_output", "Get output from agents", "None"),
        // Browser automation
        ("browser_navigate", "Navigate to a URL", "Low"),
        ("browser_screenshot", "Take a screenshot", "Low"),
        ("browser_click", "Click an element", "Medium"),
        ("browser_type", "Type text in an element", "Medium"),
        ("browser_get_page_content", "Get page HTML content", "None"),
        // Document parsing
        ("read_pdf", "Extract text from PDF files", "None"),
        ("read_office_doc", "Extract text from Office docs", "None"),
        // Planning
        ("enter_plan_mode", "Enter planning mode for complex tasks", "Low"),
        ("exit_plan_mode", "Exit planning mode and request approval", "None"),
    ];

    for (name, desc, approval) in tools {
        let approval_style = match approval {
            "None" => style(approval).green(),
            "Low" => style(approval).yellow(),
            "Medium" => style(approval).yellow().bold(),
            "High" => style(approval).red().bold(),
            _ => style(approval).dim(),
        };

        println!(
            "  {:<20} {:<40} [{}]",
            style(name).cyan(),
            desc,
            approval_style
        );
    }
}

fn show_config(workspace: &Path) {
    println!("{}", style("Configuration:").bold());
    println!();
    println!("  Workspace: {}", style(workspace.display()).green());
    println!(
        "  Config dir: {}",
        style(
            directories::ProjectDirs::from("com", "cowork", "cowork")
                .map(|d| d.config_dir().display().to_string())
                .unwrap_or_else(|| "N/A".to_string())
        )
        .dim()
    );
}

/// Show setup instructions when no API key is configured
fn show_setup_instructions(provider_type: ProviderType) {
    println!("{}", style("Welcome to Cowork!").bold().cyan());
    println!();
    println!("{}", style("Setup Required").bold().yellow());
    println!("No API key configured. Please set one up before using Cowork.");
    println!();

    let (env_var, _signup_url) = match provider_type {
        ProviderType::Anthropic => ("ANTHROPIC_API_KEY", "https://console.anthropic.com/"),
        ProviderType::OpenAI => ("OPENAI_API_KEY", "https://platform.openai.com/"),
        ProviderType::Gemini => ("GOOGLE_API_KEY", "https://aistudio.google.com/"),
        _ => ("API_KEY", "your provider's website"),
    };

    println!("{}", style("Option 1: Environment Variable (Quick)").bold());
    println!("  export {}=\"your-api-key-here\"", style(env_var).cyan());
    println!();

    println!("{}", style("Option 2: Config File (Persistent)").bold());
    let config_path = ConfigManager::default_config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "~/.config/cowork/config.toml".to_string());
    println!("  Edit: {}", style(&config_path).cyan());
    println!();
    println!("  Example config:");
    println!("  {}", style("─".repeat(50)).dim());
    println!(r#"  default_provider = "anthropic"

  [providers.anthropic]
  provider_type = "anthropic"
  model = "claude-sonnet-4-20250514"
  api_key = "your-api-key-here"

  [approval]
  auto_approve_level = "medium""#);
    println!("  {}", style("─".repeat(50)).dim());
    println!();

    println!("{}", style("Get your API key:").bold());
    println!("  Anthropic (Claude): {}", style("https://console.anthropic.com/").cyan());
    println!("  OpenAI (GPT-4):     {}", style("https://platform.openai.com/").cyan());
    println!();

    println!("{}", style("After configuring, run 'cowork' again to start.").dim());
    println!();
    println!("For more help: {}", style("cowork --help").cyan());
}

