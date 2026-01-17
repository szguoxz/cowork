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
        // Block dangerous commands by default
        blocked.insert("rm -rf /".to_string());
        blocked.insert("sudo".to_string());
        blocked.insert("su".to_string());

        Self {
            allowed_commands: HashSet::new(),
            blocked_commands: blocked,
            timeout_seconds: 30,
            working_dir: None,
        }
    }
}
