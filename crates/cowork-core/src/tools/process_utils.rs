//! Process utility functions for cross-platform command execution
//!
//! This module provides helpers for spawning processes consistently across platforms,
//! with particular attention to Windows where we want to hide console windows.

use tokio::process::Command;

/// Windows creation flag to hide the console window
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Configure a Command to hide the console window on Windows
///
/// On Windows, this sets the CREATE_NO_WINDOW flag to prevent a cmd.exe
/// window from flashing when running commands. On other platforms, this
/// is a no-op.
#[cfg(windows)]
pub fn hide_console_window(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console_window(_cmd: &mut Command) {
    // No-op on non-Windows platforms
}

/// Create a shell command configured for the current platform
///
/// On Windows, uses `cmd /C` with hidden console window.
/// On Unix, uses `sh -c`.
///
/// # Arguments
/// * `command` - The command string to execute
///
/// # Returns
/// A configured `Command` ready for further customization (working_dir, stdout, etc.)
pub fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        hide_console_window(&mut cmd);
        cmd
    }

    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

/// Create a shell command for background execution with output redirection
///
/// The output will be redirected to the specified file.
///
/// # Arguments
/// * `command` - The command string to execute
/// * `output_file` - Path to redirect stdout and stderr to
///
/// # Returns
/// A configured `Command` ready for spawning
pub fn shell_command_background(command: &str, output_file: &str) -> Command {
    use crate::tools::filesystem::shell_escape_str;

    let escaped_output = shell_escape_str(output_file);
    let full_command = format!("{} > {} 2>&1", command, escaped_output);

    shell_command(&full_command)
}

/// Create a simple command (not through shell) with hidden console on Windows
///
/// Use this for direct program execution without shell interpretation.
///
/// # Arguments
/// * `program` - The program to execute
///
/// # Returns
/// A configured `Command` ready for further customization
pub fn direct_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    hide_console_window(&mut cmd);
    cmd
}

/// Create a command for getting OS version information
///
/// Returns a configured command that will output OS version info.
pub fn os_version_command() -> std::process::Command {
    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("uname");
        cmd.arg("-r");
        hide_std_console_window(&mut cmd);
        cmd
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("sw_vers");
        cmd.arg("-productVersion");
        hide_std_console_window(&mut cmd);
        cmd
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "ver"]);
        hide_std_console_window(&mut cmd);
        cmd
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let mut cmd = std::process::Command::new("echo");
        cmd.arg(std::env::consts::OS);
        cmd
    }
}

/// Configure a std::process::Command to hide the console window on Windows
#[cfg(windows)]
pub fn hide_std_console_window(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_std_console_window(_cmd: &mut std::process::Command) {
    // No-op on non-Windows platforms
}

/// Create a std::process::Command for shell execution with hidden console
///
/// This is the synchronous version for use with std::process::Command.
pub fn std_shell_command(command: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", command]);
        hide_std_console_window(&mut cmd);
        cmd
    }

    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

/// Create a std::process::Command for direct program execution with hidden console
pub fn std_direct_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    hide_std_console_window(&mut cmd);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_command_creation() {
        let cmd = shell_command("echo hello");
        // Just verify it creates without panic
        let _ = cmd;
    }

    #[test]
    fn test_direct_command_creation() {
        let cmd = direct_command("echo");
        let _ = cmd;
    }

    #[test]
    fn test_os_version_command_creation() {
        let cmd = os_version_command();
        let _ = cmd;
    }
}
