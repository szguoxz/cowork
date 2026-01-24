//! Cowork CLI - Multi-agent assistant command line tool
//!
//! This CLI uses the unified session architecture from cowork-core,
//! sharing the same agent loop logic with the UI application.

mod onboarding;
mod tui;
mod update;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use console::style;
use onboarding::OnboardingWizard;

use cowork_core::config::ConfigManager;
use cowork_core::provider::{has_api_key_configured, ProviderType};
use cowork_core::orchestration::SystemPrompt;
use cowork_core::prompt::{ComponentRegistry, TemplateVars, substitute_commands};
use cowork_core::session::{SessionConfig, SessionInput, SessionManager, SessionOutput};
use cowork_core::skills::SkillRegistry;
use cowork_core::ToolApprovalConfig;
// Import for ! prefix bash mode
use cowork_core::tools::shell::ExecuteCommand;
use cowork_core::tools::Tool;

// TUI imports
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tui::{
    App, AppState, Event, EventHandler, Interaction, KeyAction, Message,
    handle_key_approval, handle_key_normal, handle_key_question,
};

#[derive(Parser)]
#[command(name = "cowork")]
#[command(author = "Cowork Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
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

    /// Check for updates and self-update the CLI binary
    Update {
        /// Only check for updates, don't install
        #[arg(long)]
        check: bool,
    },

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

    // Setup logging - use warn level by default to avoid interfering with CLI prompt
    // Use --verbose for info/debug level logs
    tracing_subscriber::fmt()
        .with_env_filter(if cli.verbose {
            "info,cowork_core=debug"
        } else {
            "warn"
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

    // Apply staged update if available (skip if user is running `update`)
    if !matches!(cli.command, Some(Commands::Update { .. })) {
        // apply_staged_update prints its own status messages
        let _ = update::apply_staged_update();
    }

    // Background version check: downloads eligible updates to staging
    let _version_check = if !matches!(cli.command, Some(Commands::Update { .. })) {
        Some(update::spawn_startup_check())
    } else {
        None
    };

    match cli.command {
        Some(Commands::Chat) => run_chat(&workspace, provider_type, cli.model.as_deref(), cli.auto_approve).await?,
        Some(Commands::Tools) => show_tools(),
        Some(Commands::Config) => show_config(&workspace),
        Some(Commands::Update { check }) => update::run_update(check).await?,
        Some(Commands::Plugin(cmd)) => handle_plugin_command(&workspace, cmd)?,
        Some(Commands::Components(cmd)) => handle_component_command(&workspace, cmd)?,
        None => run_chat(&workspace, provider_type, cli.model.as_deref(), cli.auto_approve).await?,
    }

    Ok(())
}

/// Build the system prompt with all template variables properly substituted
fn build_system_prompt(workspace: &Path, model_info: Option<&str>) -> String {
    let mut vars = TemplateVars::default();
    vars.working_directory = workspace.display().to_string();
    vars.is_git_repo = workspace.join(".git").exists();

    // Get git status if in a repo
    if vars.is_git_repo {
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(workspace)
            .output()
        {
            vars.git_status = String::from_utf8_lossy(&output.stdout).to_string();
        }

        // Get current branch
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(workspace)
            .output()
        {
            vars.current_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }

        // Get recent commits for style reference
        if let Ok(output) = std::process::Command::new("git")
            .args(["log", "--oneline", "-5"])
            .current_dir(workspace)
            .output()
        {
            vars.recent_commits = String::from_utf8_lossy(&output.stdout).to_string();
        }
    }

    if let Some(info) = model_info {
        vars.model_info = info.to_string();
    }

    // Populate available skills for the Skill tool
    let skill_registry = SkillRegistry::with_builtins(workspace.to_path_buf());
    let skills: Vec<String> = skill_registry
        .list_user_invocable()
        .iter()
        .map(|s| format!("- {}: {}", s.name, s.description))
        .collect();
    if !skills.is_empty() {
        vars.skills_xml = skills.join("\n");
    }

    SystemPrompt::new()
        .with_template_vars(vars)
        .build()
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

    // Build system prompt with template variables
    let system_prompt = build_system_prompt(&workspace, model.as_deref());

    // Create session config
    let mut session_config = SessionConfig::new(workspace.clone())
        .with_provider(provider_type)
        .with_approval_config(approval_config.clone())
        .with_system_prompt(system_prompt);
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
    session_manager.stop_session(session_id)?;

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

    // Check if API key is configured - show setup instructions if not
    if !has_api_key_configured(&config_manager, provider_type) {
        show_setup_instructions(provider_type);
        return Ok(());
    }

    // Get API key for session config
    let api_key = cowork_core::provider::get_api_key(&config_manager, provider_type);

    // Create session config
    let workspace_path = workspace.to_path_buf();
    let model = model.map(|s| s.to_string());
    let approval_config = if auto_approve {
        ToolApprovalConfig::trust_all()
    } else {
        ToolApprovalConfig::default()
    };

    // Build system prompt with template variables
    let system_prompt = build_system_prompt(&workspace_path, model.as_deref());

    // Create session config
    let mut session_config = SessionConfig::new(workspace_path.clone())
        .with_provider(provider_type)
        .with_approval_config(approval_config.clone())
        .with_system_prompt(system_prompt);
    if let Some(ref m) = model {
        session_config = session_config.with_model(m.clone());
    }
    if let Some(ref key) = api_key {
        session_config = session_config.with_api_key(key.clone());
    }

    // Create session manager
    let (session_manager, output_rx) = SessionManager::new(session_config);

    // Run the TUI
    run_chat_tui(
        &workspace_path,
        session_manager,
        output_rx,
        provider_type,
        auto_approve,
    ).await
}

/// Run the TUI-based chat interface
async fn run_chat_tui(
    workspace: &Path,
    session_manager: SessionManager,
    output_rx: cowork_core::session::OutputReceiver,
    provider_type: ProviderType,
    auto_approve: bool,
) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let provider_info = format!("{:?}", provider_type);
    let mut app = App::new(provider_info);

    if auto_approve {
        app.approve_all_session = true;
        app.add_message(Message::system("Auto-approve mode is ON"));
    }

    // Create event handler
    let mut events = EventHandler::new(output_rx);

    let session_id = "cli-session";

    // Main event loop
    let result = run_event_loop(
        &mut terminal,
        &mut app,
        &mut events,
        &session_manager,
        session_id,
        workspace,
    ).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    // Stop session
    let _ = session_manager.stop_all();

    result
}

/// Main event loop for the TUI
async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    events: &mut EventHandler,
    session_manager: &SessionManager,
    session_id: &str,
    workspace: &Path,
) -> anyhow::Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| tui::draw(frame, app))?;

        // Handle events
        if let Some(event) = events.next().await {
            match event {
                Event::Terminal(crossterm::event::Event::Key(key)) => {
                    // Only handle key press events, not release or repeat
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }
                    let action = match app.state {
                        AppState::Normal => handle_key_normal(key, &mut app.input),
                        AppState::Processing => {
                            // Allow typing while processing (queue input for later)
                            // But don't submit - just buffer the input
                            match key.code {
                                crossterm::event::KeyCode::Char('c')
                                    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
                                {
                                    KeyAction::Quit
                                }
                                crossterm::event::KeyCode::Enter => {
                                    // Don't submit while processing, just ignore Enter
                                    KeyAction::None
                                }
                                _ => {
                                    // Allow typing (buffer input)
                                    handle_key_normal(key, &mut app.input)
                                }
                            }
                        }
                        AppState::Interaction => {
                            match app.interactions.front_mut() {
                                Some(Interaction::ToolApproval(approval)) => handle_key_approval(key, approval),
                                Some(Interaction::Question(question)) => handle_key_question(key, question),
                                None => KeyAction::None,
                            }
                        }
                    };

                    match action {
                        KeyAction::Quit => {
                            app.should_quit = true;
                            break;
                        }
                        KeyAction::Submit(input) => {
                            handle_user_input(app, session_manager, session_id, workspace, &input).await?;
                        }
                        KeyAction::ApproveTool => {
                            if let Some(Interaction::ToolApproval(approval)) = app.interactions.pop_front() {
                                app.add_message(Message::system(format!("Approved: {}", approval.name)));
                                session_manager
                                    .push_message(session_id, SessionInput::approve_tool(&approval.id))
                                    .await?;
                                if app.interactions.is_empty() {
                                    app.state = AppState::Processing;
                                }
                            }
                        }
                        KeyAction::RejectTool => {
                            if let Some(Interaction::ToolApproval(approval)) = app.interactions.pop_front() {
                                app.add_message(Message::system(format!("Rejected: {}", approval.name)));
                                session_manager
                                    .push_message(session_id, SessionInput::reject_tool(&approval.id, None))
                                    .await?;
                                if app.interactions.is_empty() {
                                    app.state = AppState::Normal;
                                }
                            }
                        }
                        KeyAction::ApproveToolSession => {
                            if let Some(Interaction::ToolApproval(approval)) = app.interactions.pop_front() {
                                app.session_approved_tools.insert(approval.name.clone());
                                app.add_message(Message::system(format!(
                                    "Approved '{}' for session",
                                    approval.name
                                )));
                                session_manager
                                    .push_message(session_id, SessionInput::approve_tool(&approval.id))
                                    .await?;
                                if app.interactions.is_empty() {
                                    app.state = AppState::Processing;
                                }
                            }
                        }
                        KeyAction::ApproveAllSession => {
                            if let Some(Interaction::ToolApproval(approval)) = app.interactions.pop_front() {
                                app.approve_all_session = true;
                                app.add_message(Message::system("All tools approved for session"));
                                session_manager
                                    .push_message(session_id, SessionInput::approve_tool(&approval.id))
                                    .await?;
                                if app.interactions.is_empty() {
                                    app.state = AppState::Processing;
                                }
                            }
                        }
                        KeyAction::AnswerQuestion => {
                            if let Some(Interaction::Question(mut question)) = app.interactions.pop_front() {
                                // Build answer
                                let answer = if question.is_other_selected() {
                                    question.custom_input.take().unwrap_or_default()
                                } else if let Some(q) = question.current() {
                                    let selected = question.selected_options
                                        .get(question.current_question)
                                        .copied()
                                        .unwrap_or(0);
                                    q.options.get(selected)
                                        .map(|o| o.label.clone())
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };

                                // Store answer
                                question.answers.insert(
                                    question.current_question.to_string(),
                                    answer.clone(),
                                );

                                // Check if more questions in this set
                                if question.current_question + 1 < question.questions.len() {
                                    question.current_question += 1;
                                    app.interactions.push_front(Interaction::Question(question));
                                } else {
                                    // All questions in this set answered
                                    app.add_message(Message::system(format!("Answered: {}", answer)));
                                    session_manager
                                        .push_message(
                                            session_id,
                                            SessionInput::answer_question(
                                                question.request_id,
                                                question.answers,
                                            ),
                                        )
                                        .await?;
                                    
                                    if app.interactions.is_empty() {
                                        app.state = AppState::Processing;
                                    }
                                }
                            }
                        }
                        KeyAction::ScrollUp => app.scroll_up(),
                        KeyAction::ScrollDown => app.scroll_down(),
                        KeyAction::PageUp => {
                            for _ in 0..10 {
                                app.scroll_up();
                            }
                        }
                        KeyAction::PageDown => {
                            for _ in 0..10 {
                                app.scroll_down();
                            }
                        }
                        KeyAction::None => {}
                    }
                }
                Event::Terminal(crossterm::event::Event::Resize(_, _)) => {
                    // Terminal will redraw on next iteration
                }
                Event::Session(sid, output) => {
                    if sid == session_id {
                        // Check for auto-approval before handling
                        if let SessionOutput::ToolPending { ref id, ref name, .. } = output {
                            if app.should_auto_approve(name) {
                                app.add_message(Message::system(format!("Auto-approved: {}", name)));
                                session_manager
                                    .push_message(session_id, SessionInput::approve_tool(id))
                                    .await?;
                                // Don't show the approval modal
                                continue;
                            }
                        }
                        app.handle_session_output(output);
                    }
                }
                Event::Tick => {
                    // UI refresh tick - nothing to do
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Handle user input (commands and messages)
async fn handle_user_input(
    app: &mut App,
    session_manager: &SessionManager,
    session_id: &str,
    workspace: &Path,
    input: &str,
) -> anyhow::Result<()> {
    let input = input.trim();

    match input {
        "/exit" | "/quit" | "/q" => {
            app.should_quit = true;
        }
        "/help" | "/?" => {
            app.add_message(Message::system("Commands: /exit, /quit, /clear, /tools, /help"));
            app.add_message(Message::system("Use ! prefix for direct shell commands (e.g., ! ls -la)"));
            app.add_message(Message::system("Shortcuts: Ctrl+C to quit, Shift+Up/Down to scroll"));
        }
        "/tools" => {
            app.add_message(Message::system("Available tools: read_file, write_file, edit, glob, grep, execute_command, web_fetch, task, and more"));
        }
        "/clear" => {
            let _ = session_manager.stop_session(session_id);
            app.messages.clear();
            app.add_message(Message::system("Conversation cleared"));
        }
        cmd if cmd.starts_with('!') => {
            // Bash mode: run command directly
            let command = cmd[1..].trim();
            if command.is_empty() {
                app.add_message(Message::system("Usage: ! <command>"));
            } else {
                app.add_message(Message::user(format!("! {}", command)));
                let result = run_bash_command_quiet(workspace, command).await;
                match result {
                    Ok(output) => app.add_message(Message::system(output)),
                    Err(e) => app.add_message(Message::error(e.to_string())),
                }
            }
        }
        cmd if cmd.starts_with('/') && cmd.len() > 1 => {
            // Slash command: resolve skill template and inject as user message
            let skill_registry = SkillRegistry::with_builtins(workspace.to_path_buf());
            let parts: Vec<&str> = cmd[1..].splitn(2, ' ').collect();
            let skill_name = parts[0];
            let args = parts.get(1).copied().unwrap_or("");

            if let Some(skill) = skill_registry.get(skill_name) {
                app.add_message(Message::user(cmd));
                app.state = AppState::Processing;
                app.status = format!("Running /{skill_name}...");

                // Resolve the skill's prompt template with substitutions
                let template = skill.prompt_template();
                let prompt = template
                    .replace("$ARGUMENTS", args)
                    .replace("${ARGUMENTS}", args);
                let workspace_str = workspace.to_string_lossy().to_string();
                let resolved = substitute_commands(&prompt, None, Some(&workspace_str));

                let injected = format!(
                    "<command-name>/{skill_name}</command-name>\n\n{}",
                    resolved
                );

                session_manager
                    .push_message(session_id, SessionInput::user_message(&injected))
                    .await?;
            } else {
                app.add_message(Message::error(format!("Unknown command: /{skill_name}. Use /help to see available commands.")));
            }
        }
        _ => {
            // Regular message to AI
            app.add_message(Message::user(input));
            app.state = AppState::Processing;
            app.status = "Sending...".to_string();

            session_manager
                .push_message(session_id, SessionInput::user_message(input))
                .await?;
        }
    }

    Ok(())
}

/// Run a bash command and return output as string (for TUI mode)
async fn run_bash_command_quiet(workspace: &Path, command: &str) -> anyhow::Result<String> {
    let tool = ExecuteCommand::new(workspace.to_path_buf());
    let params = serde_json::json!({
        "command": command,
        "timeout": 120000
    });

    let result = tool.execute(params).await?;

    let mut output = String::new();
    if result.success {
        if let Some(stdout) = result.content.get("stdout").and_then(|v| v.as_str()) {
            if !stdout.is_empty() {
                output.push_str(stdout);
            }
        }
        if let Some(stderr) = result.content.get("stderr").and_then(|v| v.as_str()) {
            if !stderr.is_empty() {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str("[stderr] ");
                output.push_str(stderr);
            }
        }
    } else {
        if let Some(stderr) = result.content.get("stderr").and_then(|v| v.as_str()) {
            output.push_str(stderr);
        }
        if let Some(err) = result.error {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&err);
        }
    }

    if output.is_empty() {
        output = "Command completed".to_string();
    }

    Ok(output)
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
  model = "claude-sonnet-4-5-20250929"
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

