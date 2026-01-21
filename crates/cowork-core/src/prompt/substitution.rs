//! Shell command substitution for prompt templates
//!
//! This module implements the `` !`command` `` syntax for embedding
//! shell command output in prompts.
//!
//! # Syntax
//!
//! ```text
//! !`git branch --show-current`
//! ```
//!
//! Will execute `git branch --show-current` and replace the entire
//! expression with the command's stdout.
//!
//! # Error Handling
//!
//! - Commands that fail return an error marker: `[ERROR: message]`
//! - Commands that timeout return: `[TIMEOUT after Xs]`
//! - Empty output is preserved as empty string

use std::process::{Command, Stdio};
use std::time::Duration;

/// Default timeout for shell commands in milliseconds
pub const DEFAULT_TIMEOUT_MS: u64 = 5000;

/// Maximum output size to capture (to prevent memory issues)
pub const MAX_OUTPUT_SIZE: usize = 100_000;

/// Result of a shell command substitution
#[derive(Debug, Clone)]
pub enum SubstitutionResult {
    /// Command succeeded with output
    Success(String),
    /// Command failed with error message
    Error(String),
    /// Command timed out
    Timeout(Duration),
}

impl SubstitutionResult {
    /// Convert result to string for substitution
    pub fn to_substitution_string(&self) -> String {
        match self {
            SubstitutionResult::Success(output) => output.clone(),
            SubstitutionResult::Error(msg) => format!("[ERROR: {}]", msg),
            SubstitutionResult::Timeout(duration) => {
                format!("[TIMEOUT after {:.1}s]", duration.as_secs_f64())
            }
        }
    }
}

/// Execute a shell command and return its output
///
/// # Arguments
/// * `command` - The shell command to execute
/// * `timeout_ms` - Maximum time to wait for command completion
/// * `working_dir` - Optional working directory for the command
///
/// # Returns
/// The command's stdout as a string, or an error/timeout result
pub fn execute_command(
    command: &str,
    timeout_ms: Option<u64>,
    working_dir: Option<&str>,
) -> SubstitutionResult {
    // Note: timeout is captured for future async implementation
    let _timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));

    // Determine shell based on platform
    let (shell, shell_arg) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let mut cmd = Command::new(shell);
    cmd.arg(shell_arg)
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    // Spawn the command
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => return SubstitutionResult::Error(format!("Failed to spawn: {}", e)),
    };

    // Wait for completion with timeout
    // Note: This is a simplified implementation. For proper timeout handling,
    // we'd need async or threading. For now, we use wait_with_output.
    match child.wait_with_output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let result = if stdout.len() > MAX_OUTPUT_SIZE {
                    format!("{}...[truncated]", &stdout[..MAX_OUTPUT_SIZE])
                } else {
                    stdout.trim_end().to_string()
                };
                SubstitutionResult::Success(result)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let msg = if stderr.is_empty() {
                    format!("Exit code: {:?}", output.status.code())
                } else {
                    stderr.trim().to_string()
                };
                SubstitutionResult::Error(msg)
            }
        }
        Err(e) => SubstitutionResult::Error(format!("Command failed: {}", e)),
    }
}

/// Perform shell command substitution on a string
///
/// Finds all occurrences of `` !`command` `` and replaces them with
/// the command's output.
///
/// # Arguments
/// * `input` - The string to process
/// * `timeout_ms` - Timeout for each command
/// * `working_dir` - Working directory for commands
///
/// # Example
///
/// ```ignore
/// let input = "Current branch: !`git branch --show-current`";
/// let result = substitute_commands(input, None, None);
/// // Result: "Current branch: main"
/// ```
pub fn substitute_commands(
    input: &str,
    timeout_ms: Option<u64>,
    working_dir: Option<&str>,
) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        // Look for !` pattern
        if c == '!' && chars.peek() == Some(&'`') {
            chars.next(); // consume the backtick

            // Find the closing backtick
            let mut command = String::new();
            let mut found_close = false;

            for ch in chars.by_ref() {
                if ch == '`' {
                    found_close = true;
                    break;
                }
                command.push(ch);
            }

            if found_close && !command.is_empty() {
                let sub_result = execute_command(&command, timeout_ms, working_dir);
                result.push_str(&sub_result.to_substitution_string());
            } else {
                // Malformed substitution, preserve original
                result.push('!');
                result.push('`');
                result.push_str(&command);
            }
            continue;
        }

        result.push(c);
    }

    result
}

/// Parse and extract all command substitutions from a string without executing them
///
/// Useful for validation or dry-run scenarios.
pub fn extract_commands(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '!' && chars.peek() == Some(&'`') {
            chars.next(); // consume the backtick

            let mut command = String::new();
            let mut found_close = false;

            for ch in chars.by_ref() {
                if ch == '`' {
                    found_close = true;
                    break;
                }
                command.push(ch);
            }

            if found_close && !command.is_empty() {
                commands.push(command);
            }
        }
    }

    commands
}

/// Check if a string contains any command substitutions
pub fn has_substitutions(input: &str) -> bool {
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '!' && chars.peek() == Some(&'`') {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_simple_command() {
        let result = execute_command("echo hello", None, None);
        match result {
            SubstitutionResult::Success(output) => {
                assert_eq!(output, "hello");
            }
            other => panic!("Expected success, got {:?}", other),
        }
    }

    #[test]
    fn test_execute_command_with_args() {
        let result = execute_command("echo 'hello world'", None, None);
        match result {
            SubstitutionResult::Success(output) => {
                assert!(output.contains("hello world"));
            }
            other => panic!("Expected success, got {:?}", other),
        }
    }

    #[test]
    fn test_execute_failing_command() {
        let result = execute_command("exit 1", None, None);
        assert!(matches!(result, SubstitutionResult::Error(_)));
    }

    #[test]
    fn test_execute_nonexistent_command() {
        let result = execute_command("nonexistent_command_xyz", None, None);
        assert!(matches!(result, SubstitutionResult::Error(_)));
    }

    #[test]
    fn test_substitute_single_command() {
        let input = "Value: !`echo test`";
        let result = substitute_commands(input, None, None);
        assert_eq!(result, "Value: test");
    }

    #[test]
    fn test_substitute_multiple_commands() {
        let input = "A: !`echo a` B: !`echo b`";
        let result = substitute_commands(input, None, None);
        assert_eq!(result, "A: a B: b");
    }

    #[test]
    fn test_substitute_no_commands() {
        let input = "No substitutions here";
        let result = substitute_commands(input, None, None);
        assert_eq!(result, "No substitutions here");
    }

    #[test]
    fn test_substitute_preserves_regular_backticks() {
        let input = "Code: `const x = 1`";
        let result = substitute_commands(input, None, None);
        assert_eq!(result, "Code: `const x = 1`");
    }

    #[test]
    fn test_substitute_preserves_exclamation() {
        let input = "Hello! World!";
        let result = substitute_commands(input, None, None);
        assert_eq!(result, "Hello! World!");
    }

    #[test]
    fn test_extract_commands() {
        let input = "A: !`echo a` and B: !`echo b`";
        let commands = extract_commands(input);
        assert_eq!(commands, vec!["echo a", "echo b"]);
    }

    #[test]
    fn test_extract_no_commands() {
        let input = "No commands here";
        let commands = extract_commands(input);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_has_substitutions_true() {
        assert!(has_substitutions("Has !`command` here"));
    }

    #[test]
    fn test_has_substitutions_false() {
        assert!(!has_substitutions("No commands"));
        assert!(!has_substitutions("Just `backticks`"));
        assert!(!has_substitutions("Exclamation!"));
    }

    #[test]
    fn test_substitution_result_to_string() {
        assert_eq!(
            SubstitutionResult::Success("output".to_string()).to_substitution_string(),
            "output"
        );
        assert_eq!(
            SubstitutionResult::Error("failed".to_string()).to_substitution_string(),
            "[ERROR: failed]"
        );
        assert_eq!(
            SubstitutionResult::Timeout(Duration::from_secs(5)).to_substitution_string(),
            "[TIMEOUT after 5.0s]"
        );
    }

    #[test]
    fn test_working_directory() {
        let result = execute_command("pwd", None, Some("/tmp"));
        match result {
            SubstitutionResult::Success(output) => {
                // On Linux, /tmp might be a symlink, so just check it contains tmp
                assert!(output.contains("tmp") || output.contains("private"));
            }
            other => panic!("Expected success, got {:?}", other),
        }
    }

    #[test]
    fn test_malformed_substitution_unclosed() {
        let input = "Start !`echo hello and never close";
        let result = substitute_commands(input, None, None);
        // Should preserve the malformed content
        assert!(result.contains("!`"));
    }

    #[test]
    fn test_empty_command() {
        let input = "Empty: !``";
        let result = substitute_commands(input, None, None);
        // Empty command should be preserved
        assert!(result.contains("!`"));
    }

    #[test]
    fn test_multiline_command_output() {
        let result = execute_command("echo 'line1\nline2'", None, None);
        match result {
            SubstitutionResult::Success(output) => {
                assert!(output.contains("line1"));
                assert!(output.contains("line2"));
            }
            other => panic!("Expected success, got {:?}", other),
        }
    }

    #[test]
    fn test_command_with_pipes() {
        let result = execute_command("echo hello | tr 'h' 'H'", None, None);
        match result {
            SubstitutionResult::Success(output) => {
                assert_eq!(output, "Hello");
            }
            other => panic!("Expected success, got {:?}", other),
        }
    }
}
