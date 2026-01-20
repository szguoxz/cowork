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
use cowork_core::orchestration::format_size;
use cowork_core::session::{SessionConfig, SessionInput, SessionManager, SessionOutput};
use cowork_core::ToolApprovalConfig;
use cowork_core::skills::SkillRegistry;
// Only import tools that are used directly (for quick commands like /ls, /read, /search)
use cowork_core::tools::filesystem::{ListDirectory, ReadFile, SearchFiles};
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
    let api_key = cowork_core::provider::get_api_key(&config_manager, provider_type);

    // Create session config
    let workspace = workspace.to_path_buf();
    let model = model.map(|s| s.to_string());
    let approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Create session manager with a config factory
    let (session_manager, mut output_rx) = SessionManager::new(move || {
        let mut config = SessionConfig::new(workspace.clone())
            .with_provider(provider_type)
            .with_approval_config(approval_config.clone());
        if let Some(ref m) = model {
            config = config.with_model(m.clone());
        }
        if let Some(ref key) = api_key {
            config = config.with_api_key(key.clone());
        }
        config
    });

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
            SessionOutput::ToolDone { name, success, output, .. } => {
                if success {
                    let truncated = truncate_output(&output, 500);
                    println!("  {} {} {}", style("[Done:").dim(), style(&name).green(), style(truncated).dim());
                } else {
                    println!("  {} {} {}", style("[Failed:").dim(), style(&name).red(), style(&output).red());
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

/// Truncate output for display
fn truncate_output(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
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
    let base_approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Session-level approval state (can be modified during session)
    let session_approval = Arc::new(tokio::sync::Mutex::new(base_approval_config.clone()));

    // Create session manager
    let (session_manager, mut output_rx) = SessionManager::new({
        let workspace_path = workspace_path.clone();
        let model = model.clone();
        let api_key = api_key.clone();
        let approval_config = base_approval_config.clone();
        move || {
            let mut config = SessionConfig::new(workspace_path.clone())
                .with_provider(provider_type)
                .with_approval_config(approval_config.clone());
            if let Some(ref m) = model {
                config = config.with_model(m.clone());
            }
            if let Some(ref key) = api_key {
                config = config.with_api_key(key.clone());
            }
            config
        }
    });
    let session_manager = Arc::new(session_manager);

    // Channel for sending approval decisions back to the output handler
    let (approval_tx, mut approval_rx) = mpsc::channel::<ApprovalDecision>(16);

    // Spawn output handler task
    let session_manager_clone = Arc::clone(&session_manager);
    let session_approval_clone = Arc::clone(&session_approval);
    let output_handle = tokio::spawn(async move {
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
                                }
                                SessionOutput::UserMessage { .. } => {
                                    // User message echo - we already printed it
                                }
                                SessionOutput::Thinking { .. } => {
                                    // Show spinner
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
                                    let approval_config = session_approval_clone.lock().await;
                                    if approval_config.should_auto_approve(&name) {
                                        println!("  {} {}", style("✓").green(), style("Auto-approved").dim());
                                        drop(approval_config);
                                        let _ = session_manager_clone
                                            .push_message(&session_id, SessionInput::approve_tool(&id))
                                            .await;
                                    } else {
                                        drop(approval_config);
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
                                                {
                                                    let mut approval_config = session_approval_clone.lock().await;
                                                    approval_config.approve_for_session(name.clone());
                                                }
                                                println!("  {} '{}' will be auto-approved for this session",
                                                    style("✓").green(), name);
                                                let _ = session_manager_clone
                                                    .push_message(&session_id, SessionInput::approve_tool(&id))
                                                    .await;
                                            }
                                            3 => {
                                                // Approve all
                                                {
                                                    let mut approval_config = session_approval_clone.lock().await;
                                                    approval_config.approve_all_for_session();
                                                }
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
                                SessionOutput::ToolDone { name, success, output, .. } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    if success {
                                        let formatted = format_tool_result_cli(&name, &output);
                                        println!("  {}", style("Result:").bold().green());
                                        for line in formatted.lines() {
                                            println!("    {}", line);
                                        }
                                    } else {
                                        println!("  {}", style(format!("Error: {}", output)).red());
                                    }
                                }
                                SessionOutput::Question { request_id, questions } => {
                                    if let Some(s) = spinner.take() {
                                        s.finish_and_clear();
                                    }
                                    // Handle ask_user_question interactively
                                    match handle_questions_interactive(&questions) {
                                        Ok(answers) => {
                                            let _ = session_manager_clone
                                                .push_message(&session_id, SessionInput::answer_question(request_id, answers))
                                                .await;
                                        }
                                        Err(e) => {
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
            cmd if cmd.starts_with("/run ") => {
                let command = &cmd[5..];
                run_command(&workspace_path, command).await?;
            }
            cmd if cmd.starts_with("/ls ") || cmd.starts_with("/list ") => {
                let path = cmd.split_whitespace().nth(1).unwrap_or(".");
                list_files(&workspace_path, path).await?;
            }
            "/ls" | "/list" => {
                list_files(&workspace_path, ".").await?;
            }
            cmd if cmd.starts_with("/cat ") || cmd.starts_with("/read ") => {
                let path = cmd.split_whitespace().nth(1).unwrap_or("");
                if path.is_empty() {
                    println!("{}", style("Usage: /read <file>").yellow());
                } else {
                    read_file(&workspace_path, path).await?;
                }
            }
            cmd if cmd.starts_with("/search ") || cmd.starts_with("/find ") => {
                let pattern = &cmd[cmd.find(' ').unwrap_or(0) + 1..];
                search_files(&workspace_path, pattern, false).await?;
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

/// Format tool result for CLI display
#[allow(dead_code)]
fn format_tool_result_cli(_tool_name: &str, result: &str) -> String {
    // Truncate long results
    let max_len = 2000;
    if result.len() > max_len {
        format!("{}... (truncated, {} total chars)", &result[..max_len], result.len())
    } else {
        result.to_string()
    }
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

