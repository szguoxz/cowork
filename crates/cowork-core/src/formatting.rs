//! Formatting utilities for tool display and results
//!
//! This module provides consistent formatting of tool calls and results
//! for both UI display and LLM consumption.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// Truncation utilities
// ============================================================================

/// Truncate a string to max length, adding "..." if truncated
pub fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
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

/// Truncate a tool result to prevent context overflow
///
/// Large tool outputs (e.g., listing 3000+ files) can exceed the model's
/// context limit in a single response. This function truncates results
/// to a safe size while preserving useful information.
///
/// For JSON results, we truncate safely by summarizing the structure
/// rather than cutting mid-string which would produce invalid JSON.
pub fn truncate_tool_result(result: &str, max_size: usize) -> String {
    if result.len() <= max_size {
        return result.to_string();
    }

    let trimmed = result.trim();

    // Check if this looks like JSON
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        // Try to parse and summarize the JSON safely
        if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
            return truncate_json_value(&json, max_size);
        }
        // If parsing fails, it might be malformed JSON - fall through to line-based truncation
    }

    // For non-JSON or malformed JSON, truncate at line boundaries to avoid breaking structure
    truncate_at_line_boundary(result, max_size)
}

/// Truncate a JSON value safely, preserving valid JSON structure
fn truncate_json_value(value: &Value, max_size: usize) -> String {
    match value {
        Value::Array(arr) => {
            // For arrays, include first N elements that fit
            let mut result = Vec::new();
            let mut current_size = 2; // Account for [ ]

            for (i, item) in arr.iter().enumerate() {
                let item_str = serde_json::to_string(item).unwrap_or_default();
                let item_size = item_str.len() + 2; // +2 for comma and space

                if current_size + item_size > max_size && !result.is_empty() {
                    // Add truncation notice
                    let remaining = arr.len() - i;
                    let notice = format!(
                        "\n\n[Array truncated - showing {} of {} items, {} more not shown]",
                        i, arr.len(), remaining
                    );
                    let partial_json = serde_json::to_string_pretty(&Value::Array(result))
                        .unwrap_or_else(|_| "[]".to_string());
                    return format!("{}{}", partial_json, notice);
                }

                result.push(item.clone());
                current_size += item_size;
            }

            serde_json::to_string_pretty(&Value::Array(result)).unwrap_or_else(|_| "[]".to_string())
        }
        Value::Object(obj) => {
            // For objects, include first N key-value pairs that fit
            let mut result = serde_json::Map::new();
            let mut current_size = 2; // Account for { }

            for (i, (key, val)) in obj.iter().enumerate() {
                let pair_str = format!("\"{}\": {}", key, serde_json::to_string(val).unwrap_or_default());
                let pair_size = pair_str.len() + 2; // +2 for comma and space

                if current_size + pair_size > max_size && !result.is_empty() {
                    let remaining = obj.len() - i;
                    let notice = format!(
                        "\n\n[Object truncated - showing {} of {} keys, {} more not shown]",
                        i, obj.len(), remaining
                    );
                    let partial_json = serde_json::to_string_pretty(&Value::Object(result))
                        .unwrap_or_else(|_| "{}".to_string());
                    return format!("{}{}", partial_json, notice);
                }

                result.insert(key.clone(), val.clone());
                current_size += pair_size;
            }

            serde_json::to_string_pretty(&Value::Object(result)).unwrap_or_else(|_| "{}".to_string())
        }
        Value::String(s) => {
            // For large strings, truncate the string content
            if s.len() > max_size {
                let truncate_at = s
                    .char_indices()
                    .take_while(|(i, _)| *i < max_size - 50)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(max_size - 50);
                format!(
                    "\"{}...\"\n\n[String truncated - {} chars total]",
                    &s[..truncate_at],
                    s.len()
                )
            } else {
                serde_json::to_string(value).unwrap_or_default()
            }
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Truncate at a line boundary to avoid breaking mid-line or mid-string
fn truncate_at_line_boundary(result: &str, max_size: usize) -> String {
    let mut truncate_at = 0;
    let mut last_newline = 0;

    for (i, c) in result.char_indices() {
        if i >= max_size {
            break;
        }
        if c == '\n' {
            last_newline = i;
        }
        truncate_at = i + c.len_utf8();
    }

    // Prefer truncating at last newline if it's reasonably close
    let cut_point = if last_newline > max_size / 2 {
        last_newline
    } else {
        truncate_at
    };

    format!(
        "{}\n\n[Result truncated - {} chars total, showing first {}]",
        &result[..cut_point],
        result.len(),
        cut_point
    )
}

// ============================================================================
// Tool call formatting (for UI display)
// ============================================================================

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

    lines.truncate(3);
    lines.join("\n")
}

/// Format a tool call in Claude Code style: `ToolName(param: value, ...)`
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
        "ExportDocument" => {
            let path = args["file_path"].as_str().unwrap_or("?");
            let filename = path.rsplit('/').next().unwrap_or(path);
            format!("ExportDocument({})", filename)
        }
        _ => {
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

// ============================================================================
// Tool result formatting (for LLM consumption)
// ============================================================================

/// Format a tool result for human-readable display
///
/// Routes to the appropriate formatter based on tool name, or auto-detects
/// the format based on JSON structure.
pub fn format_tool_result(tool_name: &str, result: &str) -> String {
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
        truncate_str(result, 500)
    }
}

/// Format directory listing results
pub fn format_directory_result(json: &Value) -> String {
    if let (Some(count), Some(entries)) = (
        json.get("count"),
        json.get("entries").and_then(|e| e.as_array()),
    ) {
        let mut lines = vec![format!("{} items:", count)];

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
            let is_dir = entry.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
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
        truncate_str(&json.to_string(), 500)
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
        truncate_str(&json.to_string(), 500)
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
        truncate_str(&json.to_string(), 500)
    }
}

/// Format file content results
pub fn format_file_content(json: &Value, raw: &str) -> String {
    if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
        let lines: Vec<&str> = content.lines().take(20).collect();
        let mut result = lines.join("\n");
        if content.lines().count() > 20 {
            result.push_str(&format!("\n  ... ({} more lines)", content.lines().count() - 20));
        }
        result
    } else {
        truncate_str(raw, 1000)
    }
}

/// Format command execution results
pub fn format_command_result(json: &Value) -> String {
    let mut lines = Vec::new();

    if let Some(exit_code) = json.get("exit_code").and_then(|c| c.as_i64()) {
        let status = if exit_code == 0 { "✓" } else { "✗" };
        lines.push(format!("{} Exit code: {}", status, exit_code));
    }

    if let Some(stdout) = json.get("stdout").and_then(|s| s.as_str())
        && !stdout.is_empty()
    {
        lines.push(truncate_str(stdout, 400));
    }

    if let Some(stderr) = json.get("stderr").and_then(|s| s.as_str())
        && !stderr.is_empty()
    {
        lines.push(format!("stderr: {}", truncate_str(stderr, 200)));
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
        truncate_str(&json.to_string(), 200)
    }
}

/// Auto-detect and format JSON based on structure
pub fn format_generic_json(json: &Value, raw: &str) -> String {
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

    truncate_str(raw, 500)
}

// ============================================================================
// Tool result summary (for UI status display)
// ============================================================================

/// A single line in a diff preview
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffLine {
    pub line_number: Option<u32>,
    pub line_type: String,
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

/// Format a tool result summary: short, one-line description of what happened
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
            let line_count = output.lines().count();
            if line_count == 0 {
                ("Completed".to_string(), None)
            } else if line_count <= 3 {
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
                let completed = todos
                    .iter()
                    .filter(|t| t["status"].as_str() == Some("completed"))
                    .count();
                let in_progress = todos
                    .iter()
                    .filter(|t| t["status"].as_str() == Some("in_progress"))
                    .count();
                (
                    format!("{} todos ({} done, {} active)", todos.len(), completed, in_progress),
                    None,
                )
            } else {
                ("Updated todos".to_string(), None)
            }
        }
        "ExportDocument" => {
            // Parse the output JSON to get format and path
            if let Ok(json) = serde_json::from_str::<Value>(output) {
                let format = json.get("format").and_then(|f| f.as_str()).unwrap_or("document");
                let path = json.get("path").and_then(|p| p.as_str()).unwrap_or("");
                let bytes = json.get("bytes_written").and_then(|b| b.as_u64()).unwrap_or(0);

                let format_label = match format {
                    "pdf" => "PDF",
                    "docx" => "Word",
                    "xlsx" => "Excel",
                    "html_slides" => "HTML Slides",
                    _ => format,
                };

                let filename = path.rsplit('/').next().unwrap_or(path);
                (format!("Created {} ({}) - {}", format_label, format_size(bytes), filename), None)
            } else {
                ("Document exported".to_string(), None)
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
    fn test_format_size() {
        assert_eq!(format_size(0), "-");
        assert_eq!(format_size(100), "100 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
    }

    #[test]
    fn test_format_tool_summary() {
        let args = json!({"file_path": "/foo/bar.rs"});
        assert_eq!(format_tool_summary("Read", &args), "/foo/bar.rs");
    }

    #[test]
    fn test_format_tool_call() {
        let args = json!({"file_path": "/foo/bar.rs"});
        assert_eq!(format_tool_call("Read", &args), "Read(/foo/bar.rs)");
    }

    #[test]
    fn test_format_tool_result_summary() {
        let (summary, diff) = format_tool_result_summary("Read", true, "line1\nline2", &json!({}));
        assert_eq!(summary, "Read 2 lines");
        assert!(diff.is_none());
    }

    #[test]
    fn test_format_status_result() {
        let json = json!({"success": true, "message": "Done"});
        assert!(format_status_result(&json).contains("✓"));
    }

    #[test]
    fn test_format_command_result() {
        let json = json!({"exit_code": 0, "stdout": "Hello"});
        let result = format_command_result(&json);
        assert!(result.contains("✓"));
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_truncate_tool_result_json_array() {
        // Create a large JSON array
        let items: Vec<serde_json::Value> = (0..100)
            .map(|i| json!({"id": i, "name": format!("item_{}", i)}))
            .collect();
        let json_str = serde_json::to_string(&items).unwrap();

        // Truncate to a small size
        let truncated = truncate_tool_result(&json_str, 500);

        // Should be valid JSON or have a truncation notice
        assert!(truncated.contains("[Array truncated") || truncated.len() <= 500);
        // Should not have broken JSON (no unmatched quotes in the JSON portion)
        if let Some(json_end) = truncated.find("\n\n[Array truncated") {
            let json_part = &truncated[..json_end];
            assert!(serde_json::from_str::<serde_json::Value>(json_part).is_ok());
        }
    }

    #[test]
    fn test_truncate_tool_result_json_object() {
        // Create a large JSON object
        let mut obj = serde_json::Map::new();
        for i in 0..50 {
            obj.insert(
                format!("key_{}", i),
                json!({"value": format!("some_long_value_{}", i)}),
            );
        }
        let json_str = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();

        let truncated = truncate_tool_result(&json_str, 500);

        assert!(truncated.contains("[Object truncated") || truncated.len() <= 500);
    }

    #[test]
    fn test_truncate_tool_result_plain_text() {
        let text = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\n";
        let repeated = text.repeat(100);

        let truncated = truncate_tool_result(&repeated, 100);

        // Should have truncation notice
        assert!(truncated.contains("[Result truncated"));
        // Should be shorter than original
        assert!(truncated.len() < repeated.len());
        // Should not cut in the middle of "line" word (indicating mid-line cut)
        let content_end = truncated.find("\n\n[Result truncated").unwrap_or(truncated.len());
        let content = &truncated[..content_end];
        // Content should not end with partial "lin" (mid-word)
        assert!(!content.ends_with("lin"), "Should not cut mid-word");
    }

    #[test]
    fn test_truncate_tool_result_small_input() {
        let small = "small result";
        let result = truncate_tool_result(small, 1000);
        assert_eq!(result, small);
    }
}
