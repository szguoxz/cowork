//! MCP (Model Context Protocol) server management skill
//!
//! Provides commands for managing MCP servers:
//! - /mcp list - List configured MCP servers and their status
//! - /mcp add <name> <command> [args...] - Add a new stdio MCP server
//! - /mcp add <name> <url> - Add a new HTTP MCP server
//! - /mcp remove <name> - Remove an MCP server
//! - /mcp start <name> - Start a specific server
//! - /mcp stop <name> - Stop a specific server
//! - /mcp tools [server] - List tools from MCP servers

use std::sync::Arc;

use super::{BoxFuture, Skill, SkillContext, SkillInfo, SkillResult};
use crate::config::{ConfigManager, McpServerConfig};
use crate::mcp_manager::{McpServerManager, McpServerStatus};

/// MCP server management skill
pub struct McpSkill {
    manager: Arc<McpServerManager>,
}

impl McpSkill {
    pub fn new(manager: Arc<McpServerManager>) -> Self {
        Self { manager }
    }

    /// Parse subcommand from args
    fn parse_subcommand(args: &str) -> (&str, Vec<&str>) {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.is_empty() {
            ("list", vec![])
        } else {
            (parts[0], parts[1..].to_vec())
        }
    }

    /// List all MCP servers
    fn cmd_list(&self) -> SkillResult {
        let servers = self.manager.list_servers();

        if servers.is_empty() {
            return SkillResult::success(
                "No MCP servers configured.\n\nUse `/mcp add <name> <command> [args...]` to add one."
            );
        }

        let mut output = String::from("MCP Servers:\n\n");

        for server in servers {
            let status_icon = match &server.status {
                McpServerStatus::Stopped => "\u{25cf}",  // Filled circle
                McpServerStatus::Starting => "\u{25cb}", // Empty circle
                McpServerStatus::Running => "\u{2713}",  // Checkmark
                McpServerStatus::Failed(_) => "\u{2717}", // X mark
            };

            let status_text = match &server.status {
                McpServerStatus::Stopped => "stopped",
                McpServerStatus::Starting => "starting",
                McpServerStatus::Running => "running",
                McpServerStatus::Failed(msg) => msg.as_str(),
            };

            let enabled_text = if server.enabled { "" } else { " (disabled)" };

            // Show URL for HTTP servers, command for stdio
            let connection_info = if server.command.starts_with("http://") || server.command.starts_with("https://") {
                format!("URL: {}", server.command)
            } else {
                format!("Command: {}", server.command)
            };

            output.push_str(&format!(
                "  {} {} - {}{}\n    {}\n    Tools: {}\n\n",
                status_icon,
                server.name,
                status_text,
                enabled_text,
                connection_info,
                server.tool_count
            ));
        }

        SkillResult::success(output.trim())
    }

    /// Add a new MCP server
    fn cmd_add(&self, args: Vec<&str>) -> SkillResult {
        if args.len() < 2 {
            return SkillResult::error(
                "Usage:\n  /mcp add <name> <command> [args...]  - Add stdio server\n  /mcp add <name> <url>                 - Add HTTP server\n\nExamples:\n  /mcp add filesystem npx @modelcontextprotocol/server-filesystem\n  /mcp add remote https://mcp.example.com/api"
            );
        }

        let name = args[0].to_string();
        let second_arg = args[1];

        // Check if server already exists
        let existing = self.manager.list_servers();
        if existing.iter().any(|s| s.name == name) {
            return SkillResult::error(format!(
                "MCP server '{}' already exists. Use `/mcp remove {}` first.",
                name, name
            ));
        }

        // Detect if this is a URL (HTTP transport) or command (stdio transport)
        let is_url = second_arg.starts_with("http://") || second_arg.starts_with("https://");

        let (config, success_msg) = if is_url {
            // HTTP transport
            let url = second_arg.to_string();
            let config = McpServerConfig::new_http(url.clone());
            let msg = format!(
                "MCP server '{}' added (HTTP transport).\n\nURL: {}\n\nServer will start automatically when its tools are used.",
                name, url
            );
            (config, msg)
        } else {
            // Stdio transport
            let command = second_arg.to_string();
            let server_args: Vec<String> = args[2..].iter().map(|s| s.to_string()).collect();
            let config = McpServerConfig::new(command.clone())
                .with_args(server_args.clone());
            let msg = format!(
                "MCP server '{}' added.\n\nCommand: {} {}\n\nServer will start automatically when its tools are used.",
                name, command, server_args.join(" ")
            );
            (config, msg)
        };

        // Add to manager
        self.manager.add_server(name.clone(), config.clone());

        // Also save to config file
        if let Ok(mut config_manager) = ConfigManager::new() {
            config_manager.config_mut().mcp_servers.insert(name.clone(), config);
            if let Err(e) = config_manager.save() {
                tracing::warn!("Failed to save MCP config: {}", e);
            }
        }

        SkillResult::success(success_msg)
    }

    /// Remove an MCP server
    fn cmd_remove(&self, args: Vec<&str>) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error("Usage: /mcp remove <name>");
        }

        let name = args[0];

        // Check if server exists
        let existing = self.manager.list_servers();
        if !existing.iter().any(|s| s.name == name) {
            return SkillResult::error(format!("MCP server '{}' not found.", name));
        }

        // Remove from manager
        if let Err(e) = self.manager.remove_server(name) {
            return SkillResult::error(format!("Failed to remove server: {}", e));
        }

        // Also remove from config file
        if let Ok(mut config_manager) = ConfigManager::new() {
            config_manager.config_mut().mcp_servers.remove(name);
            let _ = config_manager.save();
        }

        SkillResult::success(format!("MCP server '{}' removed.", name))
    }

    /// Start an MCP server
    fn cmd_start(&self, args: Vec<&str>) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error("Usage: /mcp start <name>");
        }

        let name = args[0];

        match self.manager.start_server(name) {
            Ok(_) => {
                // Get tool count
                let tools = self.manager.get_server_tools(name).unwrap_or_default();
                SkillResult::success(format!(
                    "MCP server '{}' started.\n\nDiscovered {} tools.",
                    name, tools.len()
                ))
            }
            Err(e) => SkillResult::error(format!("Failed to start server '{}': {}", name, e)),
        }
    }

    /// Stop an MCP server
    fn cmd_stop(&self, args: Vec<&str>) -> SkillResult {
        if args.is_empty() {
            return SkillResult::error("Usage: /mcp stop <name>");
        }

        let name = args[0];

        match self.manager.stop_server(name) {
            Ok(_) => SkillResult::success(format!("MCP server '{}' stopped.", name)),
            Err(e) => SkillResult::error(format!("Failed to stop server '{}': {}", name, e)),
        }
    }

    /// List tools from MCP servers
    fn cmd_tools(&self, args: Vec<&str>) -> SkillResult {
        let tools = if let Some(server_name) = args.first() {
            // Get tools from specific server
            match self.manager.get_server_tools(server_name) {
                Some(tools) => tools,
                None => return SkillResult::error(format!("MCP server '{}' not found.", server_name)),
            }
        } else {
            // Get all tools
            self.manager.get_all_tools()
        };

        if tools.is_empty() {
            let msg = if args.is_empty() {
                "No tools available from MCP servers.\n\nMake sure servers are running with `/mcp list`."
            } else {
                "No tools available from this server.\n\nMake sure it's running with `/mcp start`."
            };
            return SkillResult::success(msg);
        }

        let mut output = String::from("MCP Tools:\n\n");

        for tool in tools {
            output.push_str(&format!(
                "  {} (from {})\n    {}\n\n",
                tool.name,
                tool.server,
                if tool.description.is_empty() { "(no description)" } else { &tool.description }
            ));
        }

        SkillResult::success(output.trim())
    }
}

impl Skill for McpSkill {
    fn info(&self) -> SkillInfo {
        SkillInfo {
            name: "mcp".to_string(),
            display_name: "MCP Server Management".to_string(),
            description: "Manage MCP (Model Context Protocol) servers".to_string(),
            usage: "/mcp <list|add|remove|start|stop|tools> [args...]".to_string(),
            user_invocable: true,
        }
    }

    fn execute(&self, ctx: SkillContext) -> BoxFuture<'_, SkillResult> {
        Box::pin(async move {
            let (subcommand, args) = Self::parse_subcommand(&ctx.args);

            match subcommand {
                "list" | "ls" => self.cmd_list(),
                "add" => self.cmd_add(args),
                "remove" | "rm" | "delete" => self.cmd_remove(args),
                "start" => self.cmd_start(args),
                "stop" => self.cmd_stop(args),
                "tools" => self.cmd_tools(args),
                "help" | "?" => SkillResult::success(HELP_TEXT),
                _ => SkillResult::error(format!(
                    "Unknown subcommand: '{}'\n\n{}",
                    subcommand, HELP_TEXT
                )),
            }
        })
    }

    fn prompt_template(&self) -> &str {
        "" // MCP skill doesn't need AI processing
    }
}

const HELP_TEXT: &str = r#"MCP Server Management Commands:

  /mcp list                    - List configured MCP servers
  /mcp add <name> <cmd> [args] - Add a stdio MCP server
  /mcp add <name> <url>        - Add an HTTP MCP server
  /mcp remove <name>           - Remove an MCP server
  /mcp start <name>            - Start/connect an MCP server
  /mcp stop <name>             - Stop/disconnect a server
  /mcp tools [server]          - List tools from servers

Examples:
  # Stdio transport (local process)
  /mcp add filesystem npx @modelcontextprotocol/server-filesystem
  /mcp start filesystem

  # HTTP transport (remote server)
  /mcp add remote https://mcp.example.com/api
  /mcp start remote

  /mcp tools
  /mcp stop filesystem"#;
