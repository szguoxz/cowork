//! Cowork CLI - Multi-agent assistant command line tool

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input};
use indicatif::{ProgressBar, ProgressStyle};

use cowork_core::config::ConfigManager;
use cowork_core::provider::{CompletionResult, GenAIProvider, LlmMessage, ProviderType};
use cowork_core::skills::SkillRegistry;
use cowork_core::tools::filesystem::{
    DeleteFile, EditFile, GlobFiles, GrepFiles, ListDirectory, MoveFile, ReadFile, SearchFiles,
    WriteFile,
};
use cowork_core::tools::lsp::LspTool;
use cowork_core::tools::notebook::NotebookEdit;
use cowork_core::tools::shell::ExecuteCommand;
use cowork_core::tools::task::{AgentInstanceRegistry, TaskOutputTool, TaskTool, TodoWrite};
use cowork_core::tools::web::{WebFetch, WebSearch};
use cowork_core::tools::{Tool, ToolDefinition, ToolRegistry};

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

    /// LLM Provider (anthropic, openai)
    #[arg(short, long, default_value = "anthropic")]
    provider: String,

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

    let workspace = cli.workspace.canonicalize().unwrap_or(cli.workspace);

    // Parse provider type
    let provider_type = match cli.provider.to_lowercase().as_str() {
        "openai" => ProviderType::OpenAI,
        "gemini" => ProviderType::Gemini,
        "anthropic" | _ => ProviderType::Anthropic,
    };

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
    workspace: &PathBuf,
    provider_type: ProviderType,
    model: Option<&str>,
    prompt: &str,
    auto_approve: bool,
) -> anyhow::Result<()> {
    // Load config
    let config_manager = ConfigManager::new()?;

    // Create provider from config or environment
    let provider = create_provider_from_config(&config_manager, provider_type, model)?
        .with_system_prompt(SYSTEM_PROMPT);

    // Create tool registry
    let tool_registry = create_tool_registry(workspace);
    let tool_definitions = tool_registry.list();

    // Chat history
    let mut messages: Vec<LlmMessage> = Vec::new();

    // Process the single message
    process_ai_message(
        prompt,
        &provider,
        &tool_registry,
        &tool_definitions,
        &mut messages,
        auto_approve,
    )
    .await?;

    Ok(())
}

async fn run_chat(
    workspace: &PathBuf,
    provider_type: ProviderType,
    model: Option<&str>,
    auto_approve: bool,
) -> anyhow::Result<()> {
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
    println!(
        "{}",
        style("Type 'help' for commands, 'exit' to quit, or just chat with the AI").dim()
    );
    println!();

    // Load config
    let config_manager = ConfigManager::new()?;

    // Create provider from config or environment
    let provider = match create_provider_from_config(&config_manager, provider_type, model) {
        Ok(p) => p.with_system_prompt(SYSTEM_PROMPT),
        Err(e) => {
            println!(
                "{}",
                style(format!("Warning: {}. The AI may not work.", e)).yellow()
            );
            println!();
            GenAIProvider::new(provider_type, model).with_system_prompt(SYSTEM_PROMPT)
        }
    };

    // Create tool registry
    let tool_registry = create_tool_registry(workspace);
    let tool_definitions = tool_registry.list();

    // Create skill registry for slash commands
    let skill_registry = SkillRegistry::with_builtins(workspace.clone());

    // Chat history
    let mut messages: Vec<LlmMessage> = Vec::new();

    loop {
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("You")
            .interact_text()?;

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        match input {
            "exit" | "quit" | "q" => {
                println!("{}", style("Goodbye!").green());
                break;
            }
            "help" | "?" => {
                print_help();
            }
            "tools" => {
                show_tools();
            }
            "clear" => {
                messages.clear();
                println!("{}", style("Conversation cleared.").green());
            }
            cmd if cmd.starts_with('/') => {
                // Handle slash commands via skill registry
                handle_slash_command(cmd, workspace, &skill_registry).await;
            }
            cmd if cmd.starts_with("run ") => {
                let command = &cmd[4..];
                run_command(workspace, command).await?;
            }
            cmd if cmd.starts_with("ls ") || cmd.starts_with("list ") => {
                let path = cmd.split_whitespace().nth(1).unwrap_or(".");
                list_files(workspace, path).await?;
            }
            cmd if cmd.starts_with("cat ") || cmd.starts_with("read ") => {
                let path = cmd.split_whitespace().nth(1).unwrap_or("");
                if path.is_empty() {
                    println!("{}", style("Usage: read <file>").yellow());
                } else {
                    read_file(workspace, path).await?;
                }
            }
            cmd if cmd.starts_with("search ") || cmd.starts_with("find ") => {
                let pattern = &cmd[cmd.find(' ').unwrap_or(0) + 1..];
                search_files(workspace, pattern, false).await?;
            }
            _ => {
                // Process with AI
                process_ai_message(
                    input,
                    &provider,
                    &tool_registry,
                    &tool_definitions,
                    &mut messages,
                    auto_approve,
                )
                .await?;
            }
        }

        println!();
    }

    Ok(())
}

/// Process a message through the AI
async fn process_ai_message(
    input: &str,
    provider: &GenAIProvider,
    tool_registry: &ToolRegistry,
    tool_definitions: &[ToolDefinition],
    messages: &mut Vec<LlmMessage>,
    auto_approve: bool,
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
                    style(format!("AI wants to use {} tool(s):", calls.len())).cyan()
                );

                let mut tool_results = Vec::new();

                for call in &calls {
                    println!();
                    println!("  {} {}", style("Tool:").bold(), style(&call.name).yellow());
                    println!(
                        "  {} {}",
                        style("Args:").bold(),
                        serde_json::to_string_pretty(&call.arguments)
                            .unwrap_or_else(|_| call.arguments.to_string())
                    );

                    // Check if tool needs approval
                    let needs_approval = tool_needs_approval(&call.name);

                    let approved = if auto_approve {
                        // Auto-approve all tools
                        println!("  {} (auto-approved)", style("Auto-approve mode").yellow());
                        true
                    } else if needs_approval {
                        Confirm::with_theme(&ColorfulTheme::default())
                            .with_prompt("Approve this tool call?")
                            .default(true)
                            .interact()?
                    } else {
                        // Auto-approve read-only tools
                        println!("  {} (auto-approved)", style("Read-only").dim());
                        true
                    };

                    if approved {
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
                                    let truncated = if result_str.len() > 500 {
                                        format!("{}... (truncated)", &result_str[..500])
                                    } else {
                                        result_str.clone()
                                    };
                                    println!("  {} {}", style("Result:").bold(), truncated);

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
                        println!("  {}", style("Rejected by user").yellow());
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

/// Check if a tool needs user approval
fn tool_needs_approval(tool_name: &str) -> bool {
    match tool_name {
        // Read-only tools - auto-approve
        "read_file" | "glob" | "grep" | "list_directory" | "search_files" | "web_fetch"
        | "web_search" | "todo_write" | "lsp" | "task_output" => false,
        // Write/execute tools - need approval
        _ => true,
    }
}

/// Handle slash commands
async fn handle_slash_command(cmd: &str, workspace: &PathBuf, registry: &SkillRegistry) {
    let result = registry.execute_command(cmd, workspace.clone()).await;
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
fn create_tool_registry(workspace: &PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Filesystem tools
    registry.register(std::sync::Arc::new(ReadFile::new(workspace.clone())));
    registry.register(std::sync::Arc::new(WriteFile::new(workspace.clone())));
    registry.register(std::sync::Arc::new(EditFile::new(workspace.clone())));
    registry.register(std::sync::Arc::new(GlobFiles::new(workspace.clone())));
    registry.register(std::sync::Arc::new(GrepFiles::new(workspace.clone())));
    registry.register(std::sync::Arc::new(ListDirectory::new(workspace.clone())));
    registry.register(std::sync::Arc::new(SearchFiles::new(workspace.clone())));
    registry.register(std::sync::Arc::new(DeleteFile::new(workspace.clone())));
    registry.register(std::sync::Arc::new(MoveFile::new(workspace.clone())));

    // Shell tools
    registry.register(std::sync::Arc::new(ExecuteCommand::new(workspace.clone())));

    // Web tools
    registry.register(std::sync::Arc::new(WebFetch::new()));
    registry.register(std::sync::Arc::new(WebSearch::new()));

    // Notebook tools
    registry.register(std::sync::Arc::new(NotebookEdit::new(workspace.clone())));

    // Task management tools
    registry.register(std::sync::Arc::new(TodoWrite::new()));

    // Code intelligence tools
    registry.register(std::sync::Arc::new(LspTool::new(workspace.clone())));

    // Agent/Task tools
    let agent_registry = std::sync::Arc::new(AgentInstanceRegistry::new());
    registry.register(std::sync::Arc::new(TaskTool::new(agent_registry.clone())));
    registry.register(std::sync::Arc::new(TaskOutputTool::new(agent_registry)));

    registry
}

/// Create a provider from config, falling back to environment variables
fn create_provider_from_config(
    config_manager: &ConfigManager,
    provider_type: ProviderType,
    model: Option<&str>,
) -> anyhow::Result<GenAIProvider> {
    let provider_name = provider_type.to_string();

    // Try to get provider config from config file
    if let Some(provider_config) = config_manager.config().providers.get(&provider_name) {
        // Get API key from config or environment
        let api_key = provider_config.get_api_key().ok_or_else(|| {
            anyhow::anyhow!(
                "No API key configured for {}. Set it in config or via {}",
                provider_name,
                provider_type.api_key_env().unwrap_or("environment variable")
            )
        })?;

        // Use model from argument, or from config
        let model = model.unwrap_or(&provider_config.model);

        // Create provider with config (supports custom base_url)
        Ok(GenAIProvider::with_config(
            provider_type,
            &api_key,
            Some(model),
            provider_config.base_url.as_deref(),
        ))
    } else {
        // No config for this provider, try environment variable
        if let Some(env_var) = provider_type.api_key_env() {
            if let Ok(api_key) = std::env::var(env_var) {
                return Ok(GenAIProvider::with_api_key(provider_type, &api_key, model));
            }
        }

        Err(anyhow::anyhow!(
            "No configuration found for provider '{}'. Add it to config file or set {}",
            provider_name,
            provider_type.api_key_env().unwrap_or("API key")
        ))
    }
}

fn print_help() {
    println!("{}", style("Commands:").bold());
    println!("  {}      - Run a shell command", style("run <cmd>").green());
    println!(
        "  {}    - List directory contents",
        style("ls <path>").green()
    );
    println!("  {}  - Read file contents", style("read <file>").green());
    println!(
        "  {} - Search for files",
        style("search <pattern>").green()
    );
    println!("  {}        - Show available tools", style("tools").green());
    println!("  {}        - Clear conversation history", style("clear").green());
    println!("  {}         - Show this help", style("help").green());
    println!("  {}         - Exit the program", style("exit").green());
    println!();
    println!("{}", style("Slash Commands:").bold());
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
    println!("  {}        - Show slash command help", style("/help").green());
    println!();
    println!(
        "{}",
        style("Or just type what you want to do - the AI will help!").dim()
    );
}

async fn run_command(workspace: &PathBuf, command: &str) -> anyhow::Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.blue} {msg}")
            .unwrap(),
    );
    spinner.set_message(format!("Running: {}", command));
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let tool = ExecuteCommand::new(workspace.clone());
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

async fn list_files(workspace: &PathBuf, path: &str) -> anyhow::Result<()> {
    let tool = ListDirectory::new(workspace.clone());
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
                                .map(|s| format_size(s))
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

async fn read_file(workspace: &PathBuf, path: &str) -> anyhow::Result<()> {
    let tool = ReadFile::new(workspace.clone());
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

async fn search_files(workspace: &PathBuf, pattern: &str, in_content: bool) -> anyhow::Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.blue} Searching...")
            .unwrap(),
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let tool = SearchFiles::new(workspace.clone());
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

fn show_config(workspace: &PathBuf) {
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

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

const SYSTEM_PROMPT: &str = r#"You are Cowork, an AI coding assistant. You help developers with software engineering tasks.

## Available Tools

### File Operations
- read_file: Read file contents (supports offset/limit for large files)
- write_file: Create or completely overwrite a file
- edit: Surgical string replacement in files. PREFER THIS over write_file for modifications - requires unique old_string or use replace_all for renaming
- glob: Find files by pattern (e.g., "**/*.rs", "src/**/*.ts")
- grep: Search file contents with regex patterns
- list_directory: List directory contents
- search_files: Search for files by name or content
- delete_file: Delete a file
- move_file: Move or rename a file

### Shell Execution
- execute_command: Run shell commands (build, test, git, etc.)

### Web Access
- web_fetch: Fetch URL content and extract text
- web_search: Search the web (requires API key configuration)

### Jupyter Notebooks
- notebook_edit: Edit, insert, or delete cells in .ipynb files

### Task Management
- todo_write: Track progress with a structured todo list

### Code Intelligence (LSP)
- lsp: Language Server Protocol operations
  - goToDefinition: Find where a symbol is defined
  - findReferences: Find all usages of a symbol
  - hover: Get type info and documentation
  - documentSymbol: List all symbols in a file
  - workspaceSymbol: Search symbols across workspace

### Sub-Agents
- task: Launch specialized subagents for complex tasks
  - Bash: Command execution specialist
  - general-purpose: Research and multi-step tasks
  - Explore: Fast codebase exploration
  - Plan: Software architecture and planning
- task_output: Get output from running/completed agents

## Workflow Guidelines

1. **Understand first**: Use read-only tools (read_file, glob, grep) to understand the codebase before making changes
2. **Use edit for modifications**: When changing existing files, use the `edit` tool with old_string/new_string for surgical precision. Only use write_file for creating new files.
3. **Be precise with edit**: The old_string must be unique in the file, or use replace_all=true. Include enough context (surrounding lines) to make it unique.
4. **Verify changes**: After modifications, verify your changes worked (read the file, run tests, etc.)
5. **Explain your reasoning**: Tell the user what you're doing and why

## Slash Commands
Users can use slash commands like /commit, /pr, /review, /help for common workflows.

Be concise and helpful. Follow existing code style. Ask for clarification if needed."#;
