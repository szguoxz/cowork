//! Formatting utilities for tool display
//!
//! Provides consistent formatting for tool arguments and ephemeral status
//! across CLI and Tauri frontends.

use serde_json::Value;

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
}
