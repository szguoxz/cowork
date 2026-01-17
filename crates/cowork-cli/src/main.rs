//! Cowork CLI - Multi-agent assistant command line tool

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};

use cowork_core::context::{Context, Workspace};
use cowork_core::task::{Task, TaskStatus, TaskType};
use cowork_core::tools::filesystem::{ListDirectory, ReadFile, SearchFiles, WriteFile};
use cowork_core::tools::shell::ExecuteCommand;
use cowork_core::tools::Tool;

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

    match cli.command {
        Some(Commands::Chat) => run_chat(&workspace).await?,
        Some(Commands::Run { command }) => run_command(&workspace, &command).await?,
        Some(Commands::List { path }) => list_files(&workspace, &path).await?,
        Some(Commands::Read { path }) => read_file(&workspace, &path).await?,
        Some(Commands::Search { pattern, content }) => {
            search_files(&workspace, &pattern, content).await?
        }
        Some(Commands::Tools) => show_tools(),
        Some(Commands::Config) => show_config(&workspace),
        None => run_chat(&workspace).await?,
    }

    Ok(())
}

async fn run_chat(workspace: &PathBuf) -> anyhow::Result<()> {
    println!("{}", style("Cowork - Multi-Agent Assistant").bold().cyan());
    println!("{}", style("Type 'help' for commands, 'exit' to quit").dim());
    println!();

    loop {
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("You")
            .interact_text()?;

        let input = input.trim();

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
                // Treat as a task description
                println!(
                    "{}",
                    style("Task received. In a full implementation, this would be processed by the AI.").dim()
                );
                println!("{}", style(format!("Task: {}", input)).cyan());

                // Show what agents would handle this
                let task_type = infer_task_type(input);
                println!(
                    "{}",
                    style(format!("Inferred type: {:?}", task_type)).dim()
                );
            }
        }

        println!();
    }

    Ok(())
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
    println!("  {}         - Show this help", style("help").green());
    println!("  {}         - Exit the program", style("exit").green());
    println!();
    println!(
        "{}",
        style("Or just type what you want to do as a natural language task.").dim()
    );
}

fn infer_task_type(input: &str) -> TaskType {
    let input_lower = input.to_lowercase();

    if input_lower.contains("file")
        || input_lower.contains("read")
        || input_lower.contains("write")
        || input_lower.contains("create")
        || input_lower.contains("delete")
    {
        TaskType::FileOperation
    } else if input_lower.contains("run")
        || input_lower.contains("execute")
        || input_lower.contains("command")
        || input_lower.contains("shell")
    {
        TaskType::ShellCommand
    } else if input_lower.contains("search") || input_lower.contains("find") {
        TaskType::Search
    } else if input_lower.contains("build") || input_lower.contains("compile") {
        TaskType::Build
    } else if input_lower.contains("test") {
        TaskType::Test
    } else if input_lower.contains("browser")
        || input_lower.contains("web")
        || input_lower.contains("url")
    {
        TaskType::WebAutomation
    } else if input_lower.contains("pdf") || input_lower.contains("document") {
        TaskType::DocumentProcessing
    } else {
        TaskType::Custom(input.to_string())
    }
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
        ("read_file", "Read file contents", "None"),
        ("write_file", "Write content to a file", "Low"),
        ("list_directory", "List directory contents", "None"),
        ("delete_file", "Delete a file or directory", "High"),
        ("move_file", "Move or rename files", "Low"),
        ("search_files", "Search for files", "None"),
        ("execute_command", "Run shell commands", "Medium"),
        ("browser_navigate", "Navigate browser to URL", "Low"),
        ("browser_screenshot", "Take browser screenshot", "None"),
        ("browser_click", "Click element in browser", "Low"),
        ("read_pdf", "Extract text from PDF", "None"),
        ("read_office_doc", "Read Office documents", "None"),
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
