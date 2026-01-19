//! Shell tools for command execution

mod execute;
mod kill;

pub use execute::ExecuteCommand;
pub use kill::{BackgroundShell, KillShell, ShellProcessRegistry, ShellStatus};

use std::collections::HashSet;

/// Configuration for shell execution security
#[derive(Debug, Clone)]
pub struct ShellConfig {
    /// Commands that are always allowed
    pub allowed_commands: HashSet<String>,
    /// Commands that are always blocked
    pub blocked_commands: HashSet<String>,
    /// Maximum execution time in seconds
    pub timeout_seconds: u64,
    /// Working directory for command execution
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let mut blocked = HashSet::new();

        // Block dangerous Unix commands
        blocked.insert("rm -rf /".to_string());
        blocked.insert("sudo".to_string());
        blocked.insert("su".to_string());
        blocked.insert("mkfs".to_string());
        blocked.insert("dd if=/dev".to_string());
        blocked.insert(":(){:|:&};:".to_string()); // Fork bomb

        // Block dangerous Windows commands
        blocked.insert("format".to_string());
        blocked.insert("del /f /s /q c:\\".to_string());
        blocked.insert("rd /s /q c:\\".to_string());
        blocked.insert("rmdir /s /q c:\\".to_string());
        blocked.insert("del /f /s /q C:\\".to_string());
        blocked.insert("rd /s /q C:\\".to_string());
        blocked.insert("rmdir /s /q C:\\".to_string());
        blocked.insert("reg delete".to_string());
        blocked.insert("bcdedit".to_string());

        Self {
            allowed_commands: HashSet::new(),
            blocked_commands: blocked,
            timeout_seconds: 30,
            working_dir: None,
        }
    }
}
