//! Shared formatting utilities for tool results
//!
//! This module provides consistent formatting of tool outputs for display
//! in both CLI and UI interfaces.

use serde_json::Value;

/// Format a tool result for human-readable display
///
/// Routes to the appropriate formatter based on tool name, or auto-detects
/// the format based on JSON structure.
pub fn format_tool_result(tool_name: &str, result: &str) -> String {
    // Try to parse as JSON and format nicely
    if let Ok(json) = serde_json::from_str::<Value>(result) {
        match tool_name {
            "list_directory" => format_directory_result(&json),
            "Glob" | "glob" | "find_files" => format_glob_result(&json),
            "Grep" | "grep" | "search_code" | "ripgrep" => format_grep_result(&json),
            "Read" | "read_file" | "read_pdf" | "read_office_doc" => format_file_content(&json, result),
            "Bash" | "execute_command" | "shell" | "bash" => format_command_result(&json),
            "Write" | "write_file" | "Edit" | "edit_file" | "delete_file" | "move_file" | "edit" => {
                format_status_result(&json)
            }
            _ => format_generic_json(&json, result),
        }
    } else {
        // Not JSON, return truncated text
        truncate_result(result, 500)
    }
}

/// Format directory listing results
pub fn format_directory_result(json: &Value) -> String {
    if let (Some(count), Some(entries)) = (
        json.get("count"),
        json.get("entries").and_then(|e| e.as_array()),
    ) {
        let mut lines = vec![format!("{} items:", count)];

        // Sort: directories first, then alphabetically
        let mut sorted: Vec<_> = entries.iter().collect();
        sorted.sort_by(|a, b| {
            let a_dir = a.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
            let b_dir = b.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
            match (a_dir, b_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    a_name.cmp(b_name)
                }
            }
        });

        for entry in sorted.iter().take(30) {
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let is_dir = entry
                .get("is_dir")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let size = entry.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

            if is_dir {
                lines.push(format!("  📁 {}/", name));
            } else {
                lines.push(format!("  📄 {} ({})", name, format_size(size)));
            }
        }

        if sorted.len() > 30 {
            lines.push(format!("  ... and {} more", sorted.len() - 30));
        }

        lines.join("\n")
    } else {
        truncate_result(&json.to_string(), 500)
    }
}

/// Format glob/file search results
pub fn format_glob_result(json: &Value) -> String {
    if let (Some(count), Some(files)) = (
        json.get("count"),
        json.get("files").and_then(|f| f.as_array()),
    ) {
        let mut lines = vec![format!("{} files found:", count)];

        for file in files.iter().take(20) {
            if let Some(path) = file.as_str() {
                lines.push(format!("  📄 {}", path));
            }
        }

        if files.len() > 20 {
            lines.push(format!("  ... and {} more", files.len() - 20));
        }

        lines.join("\n")
    } else {
        truncate_result(&json.to_string(), 500)
    }
}

/// Format grep/code search results
pub fn format_grep_result(json: &Value) -> String {
    if let Some(matches) = json.get("matches").and_then(|m| m.as_array()) {
        let total = json
            .get("total_matches")
            .and_then(|t| t.as_u64())
            .unwrap_or(matches.len() as u64);
        let mut lines = vec![format!("{} matches in {} files:", total, matches.len())];

        for m in matches.iter().take(15) {
            let path = m.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let line_num = m.get("line_number").and_then(|v| v.as_u64());
            let count = m.get("count").and_then(|v| v.as_u64());

            if let Some(n) = line_num {
                lines.push(format!("  🔍 {}:{}", path, n));
            } else if let Some(c) = count {
                lines.push(format!("  🔍 {} ({} matches)", path, c));
            } else {
                lines.push(format!("  🔍 {}", path));
            }
        }

        if matches.len() > 15 {
            lines.push(format!("  ... and {} more files", matches.len() - 15));
        }

        lines.join("\n")
    } else {
        truncate_result(&json.to_string(), 500)
    }
}

/// Format file content results
pub fn format_file_content(json: &Value, raw: &str) -> String {
    // Check if it's JSON with content field
    if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
        let lines: Vec<&str> = content.lines().take(20).collect();
        let mut result = lines.join("\n");
        if content.lines().count() > 20 {
            result.push_str(&format!(
                "\n  ... ({} more lines)",
                content.lines().count() - 20
            ));
        }
        result
    } else {
        // Might be raw file content
        truncate_result(raw, 1000)
    }
}

/// Format command execution results
pub fn format_command_result(json: &Value) -> String {
    let mut lines = Vec::new();

    if let Some(exit_code) = json.get("exit_code").and_then(|c| c.as_i64()) {
        let status = if exit_code == 0 { "✓" } else { "✗" };
        lines.push(format!("{} Exit code: {}", status, exit_code));
    }

    if let Some(stdout) = json.get("stdout").and_then(|s| s.as_str()) {
        if !stdout.is_empty() {
            lines.push(truncate_result(stdout, 400));
        }
    }

    if let Some(stderr) = json.get("stderr").and_then(|s| s.as_str()) {
        if !stderr.is_empty() {
            lines.push(format!("stderr: {}", truncate_result(stderr, 200)));
        }
    }

    if lines.is_empty() {
        "Command executed".to_string()
    } else {
        lines.join("\n")
    }
}

/// Format success/error status results
pub fn format_status_result(json: &Value) -> String {
    if let Some(success) = json.get("success").and_then(|s| s.as_bool()) {
        let msg = json.get("message").and_then(|m| m.as_str()).unwrap_or("");
        if success {
            format!("✓ {}", if msg.is_empty() { "Success" } else { msg })
        } else {
            let err = json.get("error").and_then(|e| e.as_str()).unwrap_or(msg);
            format!("✗ {}", if err.is_empty() { "Failed" } else { err })
        }
    } else if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
        msg.to_string()
    } else {
        truncate_result(&json.to_string(), 200)
    }
}

/// Auto-detect and format JSON based on structure
pub fn format_generic_json(json: &Value, raw: &str) -> String {
    // Try to detect common patterns
    if json.get("entries").is_some() {
        return format_directory_result(json);
    }
    if json.get("matches").is_some() {
        return format_grep_result(json);
    }
    if json.get("files").is_some() {
        return format_glob_result(json);
    }
    if json.get("success").is_some() || json.get("error").is_some() {
        return format_status_result(json);
    }
    if json.get("stdout").is_some() || json.get("stderr").is_some() {
        return format_command_result(json);
    }

    truncate_result(raw, 500)
}

/// Format byte size to human-readable string
pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "-".to_string();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

/// Truncate a string to max length, adding ellipsis if needed
pub fn truncate_result(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}... (truncated)", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "-");
        assert_eq!(format_size(100), "100 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
    }

    #[test]
    fn test_truncate_result() {
        assert_eq!(truncate_result("hello", 10), "hello");
        assert_eq!(truncate_result("hello world", 5), "hello... (truncated)");
    }

    #[test]
    fn test_format_status_result_success() {
        let json = json!({"success": true, "message": "File written"});
        let result = format_status_result(&json);
        assert!(result.contains("✓"));
        assert!(result.contains("File written"));
    }

    #[test]
    fn test_format_status_result_failure() {
        let json = json!({"success": false, "error": "Permission denied"});
        let result = format_status_result(&json);
        assert!(result.contains("✗"));
        assert!(result.contains("Permission denied"));
    }

    #[test]
    fn test_format_command_result() {
        let json = json!({
            "exit_code": 0,
            "stdout": "Hello, World!",
            "stderr": ""
        });
        let result = format_command_result(&json);
        assert!(result.contains("✓"));
        assert!(result.contains("Exit code: 0"));
        assert!(result.contains("Hello, World!"));
    }

    #[test]
    fn test_format_command_result_error() {
        let json = json!({
            "exit_code": 1,
            "stdout": "",
            "stderr": "Not found"
        });
        let result = format_command_result(&json);
        assert!(result.contains("✗"));
        assert!(result.contains("Not found"));
    }

    #[test]
    fn test_format_directory_result() {
        let json = json!({
            "count": 2,
            "entries": [
                {"name": "file.txt", "is_dir": false, "size": 1024},
                {"name": "docs", "is_dir": true, "size": 0}
            ]
        });
        let result = format_directory_result(&json);
        assert!(result.contains("2 items:"));
        assert!(result.contains("📁 docs/"));
        assert!(result.contains("📄 file.txt"));
    }

    #[test]
    fn test_format_glob_result() {
        let json = json!({
            "count": 2,
            "files": ["src/main.rs", "src/lib.rs"]
        });
        let result = format_glob_result(&json);
        assert!(result.contains("2 files found:"));
        assert!(result.contains("📄 src/main.rs"));
        assert!(result.contains("📄 src/lib.rs"));
    }

    #[test]
    fn test_format_grep_result() {
        let json = json!({
            "matches": [
                {"path": "src/main.rs", "line_number": 10},
                {"path": "src/lib.rs", "count": 3}
            ],
            "total_matches": 4
        });
        let result = format_grep_result(&json);
        assert!(result.contains("4 matches"));
        assert!(result.contains("🔍 src/main.rs:10"));
        assert!(result.contains("🔍 src/lib.rs (3 matches)"));
    }

    #[test]
    fn test_format_file_content() {
        let json = json!({"content": "line 1\nline 2\nline 3"});
        let result = format_file_content(&json, "");
        assert!(result.contains("line 1"));
        assert!(result.contains("line 2"));
        assert!(result.contains("line 3"));
    }

    #[test]
    fn test_format_generic_json_auto_detect() {
        // Should detect directory result
        let json = json!({"entries": [], "count": 0});
        assert!(format_generic_json(&json, "").contains("0 items:"));

        // Should detect glob result
        let json = json!({"files": [], "count": 0});
        assert!(format_generic_json(&json, "").contains("0 files found:"));

        // Should detect grep result
        let json = json!({"matches": []});
        assert!(format_generic_json(&json, "").contains("0 matches"));

        // Should detect status result
        let json = json!({"success": true});
        assert!(format_generic_json(&json, "").contains("✓"));
    }

    #[test]
    fn test_format_tool_result() {
        // Test tool-specific routing
        let dir_json = r#"{"count": 1, "entries": [{"name": "test", "is_dir": true}]}"#;
        let result = format_tool_result("list_directory", dir_json);
        assert!(result.contains("1 items:"));

        // Test non-JSON fallback
        let result = format_tool_result("unknown", "plain text output");
        assert_eq!(result, "plain text output");
    }
}
