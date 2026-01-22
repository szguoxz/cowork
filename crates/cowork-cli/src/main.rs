//! Cowork CLI - Multi-agent assistant command line tool
//!
//! This CLI uses the unified session architecture from cowork-core,
//! sharing the same agent loop logic with the UI application.

mod onboarding;

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
use tokio::sync::mpsc;

use onboarding::OnboardingWizard;

use cowork_core::config::ConfigManager;
use cowork_core::mcp_manager::McpServerManager;
use cowork_core::provider::{
    has_api_key_configured, ProviderType,
};
use cowork_core::prompt::ComponentRegistry;
use cowork_core::session::{SessionConfig, SessionInput, SessionManager, SessionOutput};
use cowork_core::ToolApprovalConfig;
use cowork_core::skills::SkillRegistry;
// Import for ! prefix bash mode
use cowork_core::tools::shell::ExecuteCommand;
use cowork_core::tools::Tool;

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

    /// Show available tools
    Tools,

    /// Show configuration
    Config,

    /// Manage plugins
    #[command(subcommand)]
    Plugin(PluginCommands),

    /// List prompt system components (agents, commands, skills)
    #[command(subcommand)]
    Components(ComponentCommands),
}

#[derive(Subcommand)]
enum PluginCommands {
    /// List installed plugins
    List,

    /// Show plugin details
    Info {
        /// Plugin name
        name: String,
    },

    /// Enable a plugin
    Enable {
        /// Plugin name
        name: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin name
        name: String,
    },
}

#[derive(Subcommand)]
enum ComponentCommands {
    /// List all available agents
    Agents,

    /// List all available commands
    Commands,

    /// List all available skills
    Skills,

    /// Show all components summary
    All,
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
        Some(Commands::Tools) => show_tools(),
        Some(Commands::Config) => show_config(&workspace),
        Some(Commands::Plugin(cmd)) => handle_plugin_command(&workspace, cmd)?,
        Some(Commands::Components(cmd)) => handle_component_command(&workspace, cmd)?,
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
    let api_key = cowork_core::provider::get_api_key(&config_manager, provider_type);

    // Create session config
    let workspace = workspace.to_path_buf();
    let model = model.map(|s| s.to_string());
    let approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Create session config
    let mut session_config = SessionConfig::new(workspace.clone())
        .with_provider(provider_type)
        .with_approval_config(approval_config.clone());
    if let Some(ref m) = model {
        session_config = session_config.with_model(m.clone());
    }
    if let Some(ref key) = api_key {
        session_config = session_config.with_api_key(key.clone());
    }

    // Create session manager
    let (session_manager, mut output_rx) = SessionManager::new(session_config);

    let session_id = "cli-oneshot";

    // Send the prompt
    session_manager
        .push_message(session_id, SessionInput::user_message(prompt))
        .await?;

    // Process outputs until idle
    while let Some((_, output)) = output_rx.recv().await {
        match output {
            SessionOutput::AssistantMessage { content, .. } => {
                println!("{}: {}", style("Assistant").bold().green(), content);
            }
            SessionOutput::ToolStart { name, .. } => {
                println!("  {} {}", style("[Executing:").dim(), style(&name).yellow());
            }
            SessionOutput::ToolDone { name, success, .. } => {
                // Only show status, not the full result
                if success {
                    println!("  {} {}", style("✓").green(), style(format!("{} completed", name)).dim());
                } else {
                    println!("  {} {}", style("✗").red(), style(format!("{} failed", name)).dim());
                }
            }
            SessionOutput::ToolPending { id, name, arguments, .. } => {
                // In one-shot mode with auto_approve=false, we need to handle approval
                if auto_approve {
                    session_manager
                        .push_message(session_id, SessionInput::approve_tool(&id))
                        .await?;
                } else {
                    // Show tool and auto-reject in non-interactive one-shot mode
                    println!("{}: {} (auto-rejected in one-shot mode)", style("Tool pending").yellow(), name);
                    println!("  Args: {}", serde_json::to_string_pretty(&arguments).unwrap_or_default());
                    session_manager
                        .push_message(session_id, SessionInput::reject_tool(&id, Some("Non-interactive mode".to_string())))
                        .await?;
                }
            }
            SessionOutput::Error { message } => {
                println!("{}", style(format!("Error: {}", message)).red());
            }
            SessionOutput::Idle => {
                // Done processing
                break;
            }
            _ => {}
        }
    }

    // Stop the session
    session_manager.stop_session(session_id).await?;

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
    let mcp_manager = Arc::new(
        McpServerManager::with_configs(config_manager.config().mcp_servers.clone())
    );

    // Get API key for session config
    let api_key = cowork_core::provider::get_api_key(&config_manager, provider_type);

    // Create skill registry for slash commands with MCP manager
    let skill_registry = SkillRegistry::with_builtins_and_mcp(workspace.to_path_buf(), Some(mcp_manager));

    // Create session config factory
    let workspace_path = workspace.to_path_buf();
    let model = model.map(|s| s.to_string());
    let mut approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Create session config
    let mut session_config = SessionConfig::new(workspace_path.clone())
        .with_provider(provider_type)
        .with_approval_config(approval_config.clone());
    if let Some(ref m) = model {
        session_config = session_config.with_model(m.clone());
    }
    if let Some(ref key) = api_key {
        session_config = session_config.with_api_key(key.clone());
    }

    // Create session manager
    let (session_manager, mut output_rx) = SessionManager::new(session_config);
    let session_manager = Arc::new(session_manager);

    // Channel for sending approval decisions back to the output handler
    let (approval_tx, mut approval_rx) = mpsc::channel::<ApprovalDecision>(16);

    // Spawn output handler task
    let session_manager_clone = session_manager.clone();
    let output_handle = tokio::spawn(async move {
        // Approval config owned by this task - no mutex needed
        let mut spinner: Option<ProgressBar> = None;

        loop {
            tokio::select! {
                // Handle session outputs
                output = output_rx.recv() => {
                    match output {
                        Some((session_id, output)) => {
                            match output {
                                SessionOutput::Ready => {
                                    // Session ready, nothing to display
                                }
                                SessionOutput::Idle => {
                                    // Clear spinner if any
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    // Print prompt hint since readline's prompt may have been overwritten
                                    print!("\n{} ", style("You>").bold());
                                    use std::io::Write;
                                    let _ = std::io::stdout().flush();
                                }
                                SessionOutput::UserMessage { .. } => {
                                    // User message echo - we already printed it
                                }
                                SessionOutput::Thinking { content } => {
                                    // Clear previous spinner if any
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }

                                    // Display thinking content if available
                                    if !content.is_empty() {
                                        println!();
                                        println!("{}", style("┌─ Thinking ──────────────────────────────────────").dim().blue());
                                        // Truncate thinking content for display
                                        let max_lines = 20;
                                        let lines: Vec<&str> = content.lines().collect();
                                        let display_lines = if lines.len() > max_lines {
                                            &lines[..max_lines]
                                        } else {
                                            &lines[..]
                                        };
                                        for line in display_lines {
                                            println!("│ {}", style(line).dim());
                                        }
                                        if lines.len() > max_lines {
                                            println!("│ {}", style(format!("... ({} more lines)", lines.len() - max_lines)).dim().italic());
                                        }
                                        println!("{}", style("└─────────────────────────────────────────────────").dim().blue());
                                    }

                                    // Show spinner for ongoing thinking
                                    let s = ProgressBar::new_spinner();
                                    s.set_style(
                                        ProgressStyle::default_spinner()
                                            .template("{spinner:.blue} {msg}")
                                            .unwrap(),
                                    );
                                    s.set_message("Thinking...");
                                    s.enable_steady_tick(std::time::Duration::from_millis(100));
                                    spinner = Some(s);
                                }
                                SessionOutput::AssistantMessage { content, .. } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    if !content.is_empty() {
                                        println!("{}: {}", style("Assistant").bold().green(), content);
                                    }
                                }
                                SessionOutput::ToolStart { name, arguments, .. } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    println!();
                                    println!("{}", style("┌─ Tool Executing ────────────────────────────────").dim());
                                    println!("│ {} {}", style("Tool:").bold(), style(&name).yellow().bold());
                                    let args_str = serde_json::to_string_pretty(&arguments)
                                        .unwrap_or_else(|_| arguments.to_string());
                                    for (i, line) in args_str.lines().enumerate() {
                                        if i == 0 {
                                            println!("│ {} {}", style("Args:").bold(), line);
                                        } else {
                                            println!("│       {}", line);
                                        }
                                    }
                                    println!("{}", style("└─────────────────────────────────────────────────").dim());

                                    // Show executing spinner
                                    let s = ProgressBar::new_spinner();
                                    s.set_style(
                                        ProgressStyle::default_spinner()
                                            .template("{spinner:.blue} Executing...")
                                            .unwrap(),
                                    );
                                    s.enable_steady_tick(std::time::Duration::from_millis(100));
                                    spinner = Some(s);
                                }
                                SessionOutput::ToolPending { id, name, arguments, .. } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }

                                    // Display tool call in a formatted box
                                    println!();
                                    println!("{}", style("┌─ Tool Call (Needs Approval) ────────────────────").dim());
                                    println!("│ {} {}", style("Tool:").bold(), style(&name).yellow().bold());
                                    let args_str = serde_json::to_string_pretty(&arguments)
                                        .unwrap_or_else(|_| arguments.to_string());
                                    for (i, line) in args_str.lines().enumerate() {
                                        if i == 0 {
                                            println!("│ {} {}", style("Args:").bold(), line);
                                        } else {
                                            println!("│       {}", line);
                                        }
                                    }
                                    println!("{}", style("└─────────────────────────────────────────────────").dim());

                                    // Check if should auto-approve based on session state
                                    if approval_config.should_auto_approve(&name) {
                                        println!("  {} {}", style("✓").green(), style("Auto-approved").dim());
                                        let _ = session_manager_clone
                                            .push_message(&session_id, SessionInput::approve_tool(&id))
                                            .await;
                                    } else {
                                        // Need user approval - show options
                                        let options: Vec<String> = vec![
                                            "Yes - approve this call".to_string(),
                                            "No - reject this call".to_string(),
                                            format!("Always - auto-approve '{}' for session", name),
                                            "Approve all - auto-approve everything for session".to_string(),
                                        ];

                                        let selection = Select::with_theme(&ColorfulTheme::default())
                                            .with_prompt("Approve?")
                                            .items(&options)
                                            .default(0)
                                            .interact()
                                            .unwrap_or(1); // Default to reject on error

                                        match selection {
                                            0 => {
                                                // Yes - approve this call
                                                let _ = session_manager_clone
                                                    .push_message(&session_id, SessionInput::approve_tool(&id))
                                                    .await;
                                            }
                                            1 => {
                                                // No - reject
                                                let _ = session_manager_clone
                                                    .push_message(&session_id, SessionInput::reject_tool(&id, None))
                                                    .await;
                                                println!("  {}", style("✗ Rejected by user").yellow());
                                            }
                                            2 => {
                                                // Always - add to session approved
                                                approval_config.approve_for_session(name.clone());
                                                println!("  {} '{}' will be auto-approved for this session",
                                                    style("✓").green(), name);
                                                let _ = session_manager_clone
                                                    .push_message(&session_id, SessionInput::approve_tool(&id))
                                                    .await;
                                            }
                                            3 => {
                                                // Approve all
                                                approval_config.approve_all_for_session();
                                                println!("  {} All tools will be auto-approved for this session",
                                                    style("✓").green());
                                                let _ = session_manager_clone
                                                    .push_message(&session_id, SessionInput::approve_tool(&id))
                                                    .await;
                                            }
                                            _ => {
                                                let _ = session_manager_clone
                                                    .push_message(&session_id, SessionInput::reject_tool(&id, None))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                SessionOutput::ToolDone { name, success, .. } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    // Only show status indicator, not the full result
                                    if success {
                                        println!("  {} {}", style("✓").green(), style(format!("{} completed", name)).dim());
                                    } else {
                                        println!("  {} {}", style("✗").red(), style(format!("{} failed", name)).dim());
                                    }
                                }
                                SessionOutput::Question { request_id, questions } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    // Visual separator for question
                                    println!();
                                    println!("{}", style("┌─ Question ───────────────────────────────────────").bold().cyan());
                                    // Handle ask_user_question interactively
                                    match handle_questions_interactive(&questions) {
                                        Ok(answers) => {
                                            println!("{}", style("└─────────────────────────────────────────────────").dim().cyan());
                                            let _ = session_manager_clone
                                                .push_message(&session_id, SessionInput::answer_question(request_id, answers))
                                                .await;
                                        }
                                        Err(e) => {
                                            println!("{}", style("└─────────────────────────────────────────────────").dim().cyan());
                                            println!("{}", style(format!("Error handling question: {}", e)).red());
                                        }
                                    }
                                }
                                SessionOutput::Error { message } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    println!("{}", style(format!("Error: {}", message)).red());
                                }
                            }
                        }
                        None => {
                            // Channel closed - session ended
                            break;
                        }
                    }
                }
                // Handle approval decisions from main thread
                decision = approval_rx.recv() => {
                    if decision.is_none() {
                        break;
                    }
                }
            }
        }
    });

    // Set up readline with history and slash command completion
    let rl_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(rustyline::CompletionType::List)
        .edit_mode(rustyline::EditMode::Emacs)
        .auto_add_history(false) // We add manually
        .build();
    let mut rl = Editor::with_config(rl_config)?;
    rl.set_helper(Some(SlashCompleter::new()));

    // Load history from file
    let history_path = directories::ProjectDirs::from("", "", "cowork")
        .map(|p| p.config_dir().join("history.txt"))
        .unwrap_or_else(|| PathBuf::from(".cowork_history"));
    let _ = rl.load_history(&history_path);

    // Use simple prompt without ANSI to avoid cursor position issues
    let prompt = "You> ";

    let session_id = "cli-session";

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
                // Stop current session and create a new one
                let _ = session_manager.stop_session(session_id).await;
                println!("{}", style("Conversation cleared.").green());
            }
            cmd if cmd.starts_with('!') => {
                // Bash mode: run command directly without AI
                let command = cmd[1..].trim();
                if command.is_empty() {
                    println!("{}", style("Usage: ! <command>").yellow());
                } else {
                    run_bash_command(&workspace_path, command).await?;
                }
            }
            cmd if cmd.starts_with('/') => {
                // Handle slash commands via skill registry
                handle_slash_command(cmd, &workspace_path, &skill_registry).await;
            }
            _ => {
                // Send to session manager
                session_manager
                    .push_message(session_id, SessionInput::user_message(input))
                    .await?;
            }
        }

        println!();
    }

    // Stop session and clean up
    let _ = session_manager.stop_all().await;
    drop(approval_tx);
    let _ = output_handle.await;

    // Save history on exit
    if let Some(parent) = history_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = rl.save_history(&history_path);

    Ok(())
}

/// Approval decision from user
#[derive(Debug)]
enum ApprovalDecision {
    #[allow(dead_code)]
    Approve(String),
    #[allow(dead_code)]
    Reject(String, Option<String>),
}

/// Handle questions from ask_user_question tool interactively
fn handle_questions_interactive(
    questions: &[cowork_core::QuestionInfo],
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut answers = std::collections::HashMap::new();

    for (i, question) in questions.iter().enumerate() {
        // Build display items with label and description
        let mut items: Vec<String> = question
            .options
            .iter()
            .map(|opt| {
                if let Some(ref desc) = opt.description {
                    format!("{} - {}", opt.label, style(desc).dim())
                } else {
                    opt.label.clone()
                }
            })
            .collect();

        // Add "Other" option
        items.push(format!("{}", style("Other (type custom answer)").italic()));

        // Display the question
        println!();
        let header = question.header.as_deref().unwrap_or("Question");
        println!("{}", style(format!("┌─ {} ─────────────────────────────────────", header)).cyan());
        println!("│ {}", style(&question.question).bold());
        println!("{}", style("└─────────────────────────────────────────────────").cyan());

        if question.multi_select {
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
                    selected_labels.push(question.options[idx].label.clone());
                }
            }
            answers.insert(i.to_string(), selected_labels.join(", "));
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
                question.options[selection].label.clone()
            };
            answers.insert(i.to_string(), answer);
        }
    }

    Ok(answers)
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

fn print_help() {
    println!("{}", style("Built-in Commands:").bold());
    println!("  {}       - Show this help", style("/help").green());
    println!("  {}       - Exit the program", style("/exit").green());
    println!("  {}      - Clear conversation history", style("/clear").green());
    println!("  {}      - Show available tools", style("/tools").green());
    println!();
    println!("{}", style("Bash Mode:").bold());
    println!(
        "  {}  - Run shell command directly (bypasses AI)",
        style("! <command>").green()
    );
    println!(
        "  {}",
        style("  Examples: ! ls -la, ! git status, ! cat file.txt").dim()
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

/// Run a bash command directly (! prefix mode)
async fn run_bash_command(workspace: &Path, command: &str) -> anyhow::Result<()> {
    let tool = ExecuteCommand::new(workspace.to_path_buf());
    let params = serde_json::json!({
        "command": command,
        "timeout": 120
    });

    let result = tool.execute(params).await;

    match result {
        Ok(output) => {
            if output.success {
                if let Some(stdout) = output.content.get("stdout")
                    && let Some(s) = stdout.as_str()
                        && !s.is_empty() {
                            println!("{}", s);
                        }
                if let Some(stderr) = output.content.get("stderr")
                    && let Some(s) = stderr.as_str()
                        && !s.is_empty() {
                            eprintln!("{}", style(s).yellow());
                        }
            } else {
                if let Some(stderr) = output.content.get("stderr")
                    && let Some(s) = stderr.as_str()
                        && !s.is_empty() {
                            eprintln!("{}", style(s).red());
                        }
                if let Some(err) = output.error {
                    eprintln!("{}", style(err).red());
                }
            }
        }
        Err(e) => {
            eprintln!("{}", style(format!("Error: {}", e)).red());
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

/// Handle plugin management commands
fn handle_plugin_command(workspace: &Path, cmd: PluginCommands) -> anyhow::Result<()> {
    // Use core's convenience constructor
    let mut registry = ComponentRegistry::for_workspace(workspace)
        .unwrap_or_else(|e| {
            eprintln!("{}", style(format!("Warning: {}", e)).yellow());
            ComponentRegistry::with_builtins()
        });

    match cmd {
        PluginCommands::List => {
            println!("{}", style("Installed Plugins:").bold());
            println!();

            let plugins: Vec<_> = registry.list_plugins().collect();
            if plugins.is_empty() {
                println!("  {}", style("No plugins installed").dim());
                println!();
                println!("  Plugins can be installed to:");
                println!("    • {}", style(".claude/plugins/").cyan());
                println!("    • {}", style("~/.claude/plugins/").cyan());
            } else {
                for plugin in plugins {
                    let status = if plugin.is_enabled() {
                        style("enabled").green()
                    } else {
                        style("disabled").red()
                    };
                    println!(
                        "  {} {} - {} [{}]",
                        style("•").cyan(),
                        style(plugin.name()).bold(),
                        plugin.version(),
                        status
                    );
                    let desc = plugin.description();
                    if !desc.is_empty() {
                        println!("    {}", style(desc).dim());
                    }
                    println!(
                        "    Components: {} agents, {} commands, {} skills",
                        plugin.agents.len(),
                        plugin.commands.len(),
                        plugin.skills.len()
                    );
                }
            }
        }

        PluginCommands::Info { name } => {
            match registry.get_plugin(&name) {
                Some(plugin) => {
                    println!("{}", style(format!("Plugin: {}", plugin.name())).bold());
                    println!();
                    println!("  Version: {}", plugin.version());
                    let desc = plugin.description();
                    if !desc.is_empty() {
                        println!("  Description: {}", desc);
                    }
                    let author = &plugin.manifest.author;
                    if !author.is_empty() {
                        println!("  Author: {}", author);
                    }
                    println!("  Status: {}", if plugin.is_enabled() { "enabled" } else { "disabled" });
                    println!("  Path: {}", plugin.base_path.display());
                    println!();

                    if !plugin.agents.is_empty() {
                        println!("  {}:", style("Agents").bold());
                        for agent in &plugin.agents {
                            println!("    • {} - {}", agent.name(), agent.description());
                        }
                    }
                    if !plugin.commands.is_empty() {
                        println!("  {}:", style("Commands").bold());
                        for cmd in &plugin.commands {
                            println!("    • /{} - {}", cmd.name(), cmd.description());
                        }
                    }
                    if !plugin.skills.is_empty() {
                        println!("  {}:", style("Skills").bold());
                        for skill in &plugin.skills {
                            println!("    • {} - {}", skill.frontmatter.name, skill.frontmatter.description);
                        }
                    }
                }
                None => {
                    println!("{}", style(format!("Plugin '{}' not found", name)).red());
                }
            }
        }

        PluginCommands::Enable { name } => {
            match registry.plugins_mut().enable(&name) {
                Ok(_) => println!("{}", style(format!("Plugin '{}' enabled", name)).green()),
                Err(e) => println!("{}", style(format!("Failed to enable plugin: {}", e)).red()),
            }
        }

        PluginCommands::Disable { name } => {
            match registry.plugins_mut().disable(&name, "Disabled by user") {
                Ok(_) => println!("{}", style(format!("Plugin '{}' disabled", name)).green()),
                Err(e) => println!("{}", style(format!("Failed to disable plugin: {}", e)).red()),
            }
        }
    }

    Ok(())
}

/// Handle component listing commands
fn handle_component_command(workspace: &Path, cmd: ComponentCommands) -> anyhow::Result<()> {
    // Use core's convenience constructor
    let registry = ComponentRegistry::for_workspace(workspace)
        .unwrap_or_else(|e| {
            eprintln!("{}", style(format!("Warning: {}", e)).yellow());
            ComponentRegistry::with_builtins()
        });

    // Get summary for serializable info
    let summary = registry.summary();

    match cmd {
        ComponentCommands::Agents => {
            println!("{}", style("Available Agents:").bold());
            println!();

            let mut agents: Vec<_> = registry.list_agents().collect();
            agents.sort_by_key(|a| a.name());

            for agent in agents {
                println!(
                    "  {} {} - {}",
                    style("•").cyan(),
                    style(agent.name()).bold(),
                    agent.description()
                );
                let model = agent.model();
                if !matches!(model, cowork_core::ModelPreference::Inherit) {
                    println!("    Model: {}", style(format!("{:?}", model)).dim());
                }
                let restrictions = agent.tool_restrictions();
                if !restrictions.allowed.is_empty() {
                    let tool_names: Vec<_> = restrictions.allowed.iter().map(|t| t.to_string()).collect();
                    println!("    Tools: {}", style(tool_names.join(", ")).dim());
                }
            }
        }

        ComponentCommands::Commands => {
            println!("{}", style("Available Commands:").bold());
            println!();

            let mut commands: Vec<_> = registry.list_commands().collect();
            commands.sort_by_key(|c| c.name());

            for cmd in commands {
                println!(
                    "  {} /{} - {}",
                    style("•").cyan(),
                    style(cmd.name()).bold(),
                    cmd.description()
                );
            }
        }

        ComponentCommands::Skills => {
            println!("{}", style("Available Skills:").bold());
            println!();

            let mut skills: Vec<_> = registry.list_skills().collect();
            skills.sort_by_key(|s| s.frontmatter.name.as_str());

            if skills.is_empty() {
                println!("  {}", style("No custom skills installed").dim());
                println!();
                println!("  Skills can be added to:");
                println!("    • {}", style(".claude/skills/*/SKILL.md").cyan());
                println!("    • {}", style("~/.claude/skills/*/SKILL.md").cyan());
            } else {
                for skill in skills {
                    let invocable = if skill.frontmatter.user_invocable {
                        style("[user-invocable]").green()
                    } else {
                        style("[auto-only]").dim()
                    };
                    println!(
                        "  {} {} - {} {}",
                        style("•").cyan(),
                        style(&skill.frontmatter.name).bold(),
                        skill.frontmatter.description,
                        invocable
                    );
                }
            }
        }

        ComponentCommands::All => {
            // Show summary of all components using the serializable summary
            println!("{}", style("Component Registry Summary:").bold());
            println!();

            println!("  {} {} agents", style("•").cyan(), summary.counts.agents);
            println!("  {} {} commands", style("•").cyan(), summary.counts.commands);
            println!("  {} {} skills", style("•").cyan(), summary.counts.skills);
            println!("  {} {} plugins", style("•").cyan(), summary.counts.plugins);
            println!();

            // Show source breakdown
            println!("{}", style("Agents:").bold());
            for agent in &summary.agents {
                println!("  {} [{}]", style(&agent.name).green(), agent.scope);
            }
            println!();

            println!("{}", style("Commands:").bold());
            for cmd in &summary.commands {
                println!("  /{} [{}]", style(&cmd.name).green(), cmd.scope);
            }

            if !summary.skills.is_empty() {
                println!();
                println!("{}", style("Skills:").bold());
                for skill in &summary.skills {
                    println!("  {} [{}]", style(&skill.name).green(), skill.source);
                }
            }
        }
    }

    Ok(())
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

