//! Formatting utilities for tool display
//!
//! Provides consistent formatting for tool arguments and ephemeral status
//! across CLI and Tauri frontends.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single line in a diff preview
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffLine {
    /// Line number (1-based, if available)
    pub line_number: Option<u32>,
    /// The type of line: "context", "added", or "removed"
    pub line_type: String,
    /// The content of the line
    pub content: String,
}

impl DiffLine {
    pub fn context(line_number: u32, content: impl Into<String>) -> Self {
        Self {
            line_number: Some(line_number),
            line_type: "context".to_string(),
            content: content.into(),
        }
    }

    pub fn added(line_number: u32, content: impl Into<String>) -> Self {
        Self {
            line_number: Some(line_number),
            line_type: "added".to_string(),
            content: content.into(),
        }
    }

    pub fn removed(content: impl Into<String>) -> Self {
        Self {
            line_number: None,
            line_type: "removed".to_string(),
            content: content.into(),
        }
    }
}

/// Truncate a string to max length, adding "..." if truncated
pub fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

/// Format tool arguments into a concise single-line summary
pub fn format_tool_summary(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "Read" => args["file_path"].as_str().unwrap_or("?").to_string(),
        "Write" => args["file_path"].as_str().unwrap_or("?").to_string(),
        "Edit" => args["file_path"].as_str().unwrap_or("?").to_string(),
        "Glob" => args["pattern"].as_str().unwrap_or("?").to_string(),
        "Grep" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            let path = args["path"].as_str().unwrap_or(".");
            format!("{} in {}", pattern, path)
        }
        "Bash" => {
            let cmd = args["command"].as_str().unwrap_or("?");
            truncate_str(cmd, 100)
        }
        "Task" => {
            let desc = args["description"].as_str().unwrap_or("?");
            let agent = args["subagent_type"].as_str().unwrap_or("?");
            format!("[{}] {}", agent, desc)
        }
        "WebFetch" => args["url"].as_str().unwrap_or("?").to_string(),
        "WebSearch" => args["query"].as_str().unwrap_or("?").to_string(),
        "LSP" => {
            let op = args["operation"].as_str().unwrap_or("?");
            let file = args["filePath"].as_str().unwrap_or("?");
            format!("{} {}", op, file)
        }
        _ => serde_json::to_string(args).unwrap_or_default(),
    }
}

/// Format ephemeral display for tool execution (up to 3 lines)
pub fn format_ephemeral(tool_name: &str, args: &Value) -> String {
    let mut lines = Vec::new();

    match tool_name {
        "Read" | "Glob" => {
            let path = args["file_path"]
                .as_str()
                .or_else(|| args["pattern"].as_str())
                .unwrap_or("?");
            lines.push(format!("{}: {}", tool_name, truncate_str(path, 60)));
        }
        "Write" => {
            if let Some(path) = args["file_path"].as_str() {
                lines.push(format!("Write: {}", truncate_str(path, 60)));
            }
            if let Some(content) = args["content"].as_str() {
                let line_count = content.lines().count();
                lines.push(format!("  {} lines", line_count));
            }
        }
        "Edit" => {
            if let Some(path) = args["file_path"].as_str() {
                lines.push(format!("Edit: {}", truncate_str(path, 60)));
            }
            if let Some(old) = args["old_string"].as_str() {
                let preview = old.lines().next().unwrap_or("");
                lines.push(format!("  - {}", truncate_str(preview, 50)));
            }
            if let Some(new) = args["new_string"].as_str() {
                let preview = new.lines().next().unwrap_or("");
                lines.push(format!("  + {}", truncate_str(preview, 50)));
            }
        }
        "Grep" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            let path = args["path"].as_str().unwrap_or(".");
            lines.push(format!(
                "Grep: {} in {}",
                truncate_str(pattern, 30),
                truncate_str(path, 30)
            ));
        }
        "Bash" => {
            if let Some(cmd) = args["command"].as_str() {
                lines.push(format!(
                    "Bash: {}",
                    truncate_str(cmd.lines().next().unwrap_or(cmd), 60)
                ));
                if cmd.lines().count() > 1 {
                    lines.push(format!("  ({} lines)", cmd.lines().count()));
                }
            }
        }
        "Task" => {
            let desc = args["description"].as_str().unwrap_or("?");
            let agent = args["subagent_type"].as_str().unwrap_or("?");
            lines.push(format!("Task [{}]: {}", agent, truncate_str(desc, 50)));
        }
        _ => {
            let summary = format_tool_summary(tool_name, args);
            lines.push(format!("{}: {}", tool_name, truncate_str(&summary, 60)));
        }
    }

    // Limit to 3 lines
    lines.truncate(3);
    lines.join("\n")
}

/// Format a tool call in Claude Code style: `ToolName(param: value, ...)`
///
/// This produces a concise, human-readable format for display:
/// - `Read(/path/to/file.rs)`
/// - `Grep(pattern: "foo", path: "/src")`
/// - `Bash(cargo build)`
pub fn format_tool_call(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "Read" => {
            let path = args["file_path"].as_str().unwrap_or("?");
            format!("Read({})", path)
        }
        "Write" => {
            let path = args["file_path"].as_str().unwrap_or("?");
            let lines = args["content"]
                .as_str()
                .map(|c| c.lines().count())
                .unwrap_or(0);
            format!("Write({}, {} lines)", path, lines)
        }
        "Edit" => {
            let path = args["file_path"].as_str().unwrap_or("?");
            format!("Edit({})", path)
        }
        "Glob" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            if let Some(path) = args["path"].as_str() {
                format!("Glob(pattern: \"{}\", path: \"{}\")", pattern, path)
            } else {
                format!("Glob(\"{}\")", pattern)
            }
        }
        "Grep" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            let path = args["path"].as_str().unwrap_or(".");
            format!("Grep(pattern: \"{}\", path: \"{}\")", truncate_str(pattern, 30), path)
        }
        "Bash" => {
            let cmd = args["command"].as_str().unwrap_or("?");
            let first_line = cmd.lines().next().unwrap_or(cmd);
            format!("Bash({})", truncate_str(first_line, 60))
        }
        "Task" => {
            let desc = args["description"].as_str().unwrap_or("?");
            let agent = args["subagent_type"].as_str().unwrap_or("?");
            format!("Task({}: \"{}\")", agent, truncate_str(desc, 40))
        }
        "WebFetch" => {
            let url = args["url"].as_str().unwrap_or("?");
            format!("WebFetch({})", truncate_str(url, 60))
        }
        "WebSearch" => {
            let query = args["query"].as_str().unwrap_or("?");
            format!("WebSearch(\"{}\")", truncate_str(query, 50))
        }
        "LSP" => {
            let op = args["operation"].as_str().unwrap_or("?");
            let file = args["filePath"].as_str().unwrap_or("?");
            format!("LSP({}: {})", op, file)
        }
        "TodoWrite" => {
            if let Some(todos) = args["todos"].as_array() {
                format!("TodoWrite({} items)", todos.len())
            } else {
                "TodoWrite(...)".to_string()
            }
        }
        _ => {
            // Generic: show first few args
            if let Some(obj) = args.as_object() {
                let params: Vec<String> = obj
                    .iter()
                    .take(2)
                    .map(|(k, v)| {
                        let val = match v {
                            Value::String(s) => truncate_str(s, 20),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => "...".to_string(),
                        };
                        format!("{}: {}", k, val)
                    })
                    .collect();
                if obj.len() > 2 {
                    format!("{}({}, ...)", tool_name, params.join(", "))
                } else if params.is_empty() {
                    format!("{}()", tool_name)
                } else {
                    format!("{}({})", tool_name, params.join(", "))
                }
            } else {
                format!("{}(...)", tool_name)
            }
        }
    }
}

/// Format a tool result summary: short, one-line description of what happened
///
/// Returns: (summary, optional diff lines for Edit tool)
pub fn format_tool_result_summary(
    tool_name: &str,
    success: bool,
    output: &str,
    args: &Value,
) -> (String, Option<Vec<DiffLine>>) {
    if !success {
        let err_preview = output.lines().next().unwrap_or("Error");
        return (format!("Error: {}", truncate_str(err_preview, 60)), None);
    }

    match tool_name {
        "Read" => {
            let line_count = output.lines().count();
            (format!("Read {} lines", line_count), None)
        }
        "Write" => {
            let line_count = args["content"]
                .as_str()
                .map(|c| c.lines().count())
                .unwrap_or(0);
            (format!("Wrote {} lines", line_count), None)
        }
        "Edit" => {
            // Generate diff preview
            let old_str = args["old_string"].as_str().unwrap_or("");
            let new_str = args["new_string"].as_str().unwrap_or("");
            let diff = generate_edit_diff(old_str, new_str);

            let old_lines = old_str.lines().count();
            let new_lines = new_str.lines().count();
            let summary = if new_lines > old_lines {
                format!("Added {} lines", new_lines - old_lines)
            } else if old_lines > new_lines {
                format!("Removed {} lines", old_lines - new_lines)
            } else {
                format!("Changed {} lines", old_lines)
            };
            (summary, Some(diff))
        }
        "Glob" => {
            let match_count = output.lines().filter(|l| !l.is_empty()).count();
            (format!("Found {} files", match_count), None)
        }
        "Grep" => {
            let match_count = output.lines().filter(|l| !l.is_empty()).count();
            if match_count == 0 {
                ("Found 0 matches".to_string(), None)
            } else {
                (format!("Found {} matches", match_count), None)
            }
        }
        "Bash" => {
            // Check for exit code in output
            let line_count = output.lines().count();
            if line_count == 0 {
                ("Completed".to_string(), None)
            } else if line_count <= 3 {
                // Show short output inline
                let preview = output.lines().take(3).collect::<Vec<_>>().join(" | ");
                (truncate_str(&preview, 60), None)
            } else {
                (format!("{} lines of output", line_count), None)
            }
        }
        "Task" => {
            let preview = output.lines().next().unwrap_or("Completed");
            (truncate_str(preview, 60), None)
        }
        "WebSearch" | "WebFetch" => {
            let preview = output.lines().next().unwrap_or("Completed");
            (truncate_str(preview, 60), None)
        }
        "TodoWrite" => {
            if let Some(todos) = args["todos"].as_array() {
                let completed = todos.iter()
                    .filter(|t| t["status"].as_str() == Some("completed"))
                    .count();
                let in_progress = todos.iter()
                    .filter(|t| t["status"].as_str() == Some("in_progress"))
                    .count();
                (format!("{} todos ({} done, {} active)", todos.len(), completed, in_progress), None)
            } else {
                ("Updated todos".to_string(), None)
            }
        }
        _ => {
            if output.is_empty() {
                ("Done".to_string(), None)
            } else {
                let preview = output.lines().next().unwrap_or("Done");
                (truncate_str(preview, 60), None)
            }
        }
    }
}

/// Generate diff lines for an Edit tool operation
fn generate_edit_diff(old_str: &str, new_str: &str) -> Vec<DiffLine> {
    let mut diff_lines = Vec::new();
    let old_lines: Vec<&str> = old_str.lines().collect();
    let new_lines: Vec<&str> = new_str.lines().collect();

    // Simple diff: show removed lines, then added lines
    // Limit to 10 lines total for preview
    let max_lines = 10;
    let mut count = 0;

    // Show removed lines (max 5)
    for (i, line) in old_lines.iter().take(5).enumerate() {
        if count >= max_lines {
            break;
        }
        diff_lines.push(DiffLine::removed(line.to_string()));
        count += 1;
        if i == 4 && old_lines.len() > 5 {
            diff_lines.push(DiffLine {
                line_number: None,
                line_type: "context".to_string(),
                content: format!("... {} more removed", old_lines.len() - 5),
            });
            count += 1;
        }
    }

    // Show added lines (max 5)
    for (i, line) in new_lines.iter().take(5).enumerate() {
        if count >= max_lines {
            break;
        }
        // Line numbers start at 1, and we don't know the actual line number in the file
        // So we just use relative positions
        diff_lines.push(DiffLine::added((i + 1) as u32, line.to_string()));
        count += 1;
        if i == 4 && new_lines.len() > 5 {
            diff_lines.push(DiffLine {
                line_number: None,
                line_type: "context".to_string(),
                content: format!("... {} more added", new_lines.len() - 5),
            });
        }
    }

    diff_lines
}

/// Format tool arguments for approval modal display (multi-line, readable)
pub fn format_approval_args(tool_name: &str, args: &Value) -> Vec<String> {
    let mut lines = Vec::new();

    match tool_name {
        "Write" => {
            if let Some(path) = args["file_path"].as_str() {
                lines.push(format!("File: {}", path));
            }
            if let Some(content) = args["content"].as_str() {
                let preview = content.lines().take(5).collect::<Vec<_>>().join("\n");
                let total_lines = content.lines().count();
                lines.push(format!("Content ({} lines):", total_lines));
                for line in preview.lines().take(5) {
                    lines.push(format!("  {}", truncate_str(line, 60)));
                }
                if total_lines > 5 {
                    lines.push(format!("  ... ({} more lines)", total_lines - 5));
                }
            }
        }
        "Edit" => {
            if let Some(path) = args["file_path"].as_str() {
                lines.push(format!("File: {}", path));
            }
            if let Some(old) = args["old_string"].as_str() {
                lines.push("Old:".to_string());
                for line in old.lines().take(3) {
                    lines.push(format!("  - {}", truncate_str(line, 50)));
                }
                if old.lines().count() > 3 {
                    lines.push(format!("  ... ({} more lines)", old.lines().count() - 3));
                }
            }
            if let Some(new) = args["new_string"].as_str() {
                lines.push("New:".to_string());
                for line in new.lines().take(3) {
                    lines.push(format!("  + {}", truncate_str(line, 50)));
                }
                if new.lines().count() > 3 {
                    lines.push(format!("  ... ({} more lines)", new.lines().count() - 3));
                }
            }
        }
        "Bash" => {
            if let Some(cmd) = args["command"].as_str() {
                lines.push("Command:".to_string());
                for line in cmd.lines().take(5) {
                    lines.push(format!("  {}", truncate_str(line, 60)));
                }
                if cmd.lines().count() > 5 {
                    lines.push(format!("  ... ({} more lines)", cmd.lines().count() - 5));
                }
            }
            if let Some(desc) = args["description"].as_str() {
                lines.push(format!("Description: {}", truncate_str(desc, 60)));
            }
        }
        _ => {
            // Generic: show each key-value, truncated
            if let Some(obj) = args.as_object() {
                for (key, value) in obj.iter().take(6) {
                    let val_str = match value {
                        Value::String(s) => truncate_str(s, 50),
                        Value::Null => "null".to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Number(n) => n.to_string(),
                        _ => truncate_str(&value.to_string(), 50),
                    };
                    lines.push(format!("{}: {}", key, val_str));
                }
                if obj.len() > 6 {
                    lines.push(format!("... ({} more fields)", obj.len() - 6));
                }
            }
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_format_tool_summary() {
        let args = json!({"file_path": "/foo/bar.rs"});
        assert_eq!(format_tool_summary("Read", &args), "/foo/bar.rs");

        let args = json!({"pattern": "*.rs", "path": "/src"});
        assert_eq!(format_tool_summary("Grep", &args), "*.rs in /src");
    }

    #[test]
    fn test_format_ephemeral() {
        let args = json!({"file_path": "/foo/bar.rs", "content": "line1\nline2\nline3"});
        let result = format_ephemeral("Write", &args);
        assert!(result.contains("Write: /foo/bar.rs"));
        assert!(result.contains("3 lines"));
    }

    #[test]
    fn test_format_approval_args() {
        let args = json!({"file_path": "/foo/bar.rs", "old_string": "old", "new_string": "new"});
        let lines = format_approval_args("Edit", &args);
        assert!(lines.iter().any(|l| l.contains("File:")));
        assert!(lines.iter().any(|l| l.contains("Old:")));
        assert!(lines.iter().any(|l| l.contains("New:")));
    }

    #[test]
    fn test_format_tool_call() {
        let args = json!({"file_path": "/foo/bar.rs"});
        assert_eq!(format_tool_call("Read", &args), "Read(/foo/bar.rs)");

        let args = json!({"pattern": "*.rs", "path": "/src"});
        assert_eq!(
            format_tool_call("Grep", &args),
            "Grep(pattern: \"*.rs\", path: \"/src\")"
        );

        let args = json!({"command": "cargo build"});
        assert_eq!(format_tool_call("Bash", &args), "Bash(cargo build)");
    }

    #[test]
    fn test_format_tool_result_summary() {
        // Read tool
        let (summary, diff) = format_tool_result_summary(
            "Read",
            true,
            "line1\nline2\nline3",
            &json!({}),
        );
        assert_eq!(summary, "Read 3 lines");
        assert!(diff.is_none());

        // Edit tool with diff
        let (summary, diff) = format_tool_result_summary(
            "Edit",
            true,
            "ok",
            &json!({"old_string": "old", "new_string": "new\nline"}),
        );
        assert!(summary.contains("Added"));
        assert!(diff.is_some());
        let diff = diff.unwrap();
        assert!(diff.iter().any(|d| d.line_type == "removed"));
        assert!(diff.iter().any(|d| d.line_type == "added"));

        // Error case
        let (summary, _) = format_tool_result_summary("Read", false, "File not found", &json!({}));
        assert!(summary.starts_with("Error:"));
    }

    #[test]
    fn test_diff_line() {
        let added = DiffLine::added(5, "new content");
        assert_eq!(added.line_number, Some(5));
        assert_eq!(added.line_type, "added");

        let removed = DiffLine::removed("old content");
        assert!(removed.line_number.is_none());
        assert_eq!(removed.line_type, "removed");
    }
}
