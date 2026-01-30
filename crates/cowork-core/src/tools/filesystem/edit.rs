//! Edit file tool - surgical string replacement

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

use super::validate_path;

/// Tool for performing exact string replacements in files
pub struct EditFile {
    workspace: PathBuf,
}

impl EditFile {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for EditFile {
    fn name(&self) -> &str {
        "Edit"
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::EDIT
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_string (default false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let file_path = params["file_path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("file_path is required".into()))?;

            let old_string = params["old_string"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("old_string is required".into()))?;

            let new_string = params["new_string"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("new_string is required".into()))?;

            let replace_all = params["replace_all"].as_bool().unwrap_or(false);

            // Validate that old_string and new_string are different
            if old_string == new_string {
                return Err(ToolError::InvalidParams(
                    "old_string and new_string must be different".into(),
                ));
            }

            // Validate path
            let path = self.workspace.join(file_path);
            let validated = validate_path(&path, &self.workspace)?;

            // Read current content
            let content = tokio::fs::read_to_string(&validated)
                .await
                .map_err(ToolError::Io)?;

            // Detect original line ending style (for preserving on write)
            let uses_crlf = content.contains("\r\n");

            // Normalize line endings to LF for matching
            // This handles Windows files (CRLF) when the LLM sends LF
            let content_normalized = content.replace("\r\n", "\n");
            let old_string_normalized = old_string.replace("\r\n", "\n");
            let new_string_normalized = new_string.replace("\r\n", "\n");

            // Count occurrences using normalized strings
            let occurrences = content_normalized.matches(&old_string_normalized).count();

            if occurrences == 0 {
                return Err(ToolError::InvalidParams(
                    "old_string not found in file. Make sure to match the exact content including whitespace and indentation.".into()
                ));
            }

            if !replace_all && occurrences > 1 {
                return Err(ToolError::InvalidParams(format!(
                    "old_string appears {} times in the file. Either provide more context to make it unique, \
                     or set replace_all=true to replace all occurrences.",
                    occurrences
                )));
            }

            // Perform replacement on normalized content
            let new_content_normalized = if replace_all {
                content_normalized.replace(&old_string_normalized, &new_string_normalized)
            } else {
                content_normalized.replacen(&old_string_normalized, &new_string_normalized, 1)
            };

            // Calculate diff info (before moving new_content_normalized)
            let old_lines = content_normalized.lines().count();
            let new_lines = new_content_normalized.lines().count();
            let lines_changed = (new_lines as i64 - old_lines as i64).abs();

            // Restore original line ending style if the file used CRLF
            let new_content = if uses_crlf {
                new_content_normalized.replace("\n", "\r\n")
            } else {
                new_content_normalized
            };

            // Write back
            tokio::fs::write(&validated, &new_content)
                .await
                .map_err(ToolError::Io)?;

            Ok(ToolOutput::success(json!({
                "success": true,
                "path": file_path,
                "occurrences_replaced": if replace_all { occurrences } else { 1 },
                "old_line_count": old_lines,
                "new_line_count": new_lines,
                "lines_changed": lines_changed
            })))
        })
    }
}
