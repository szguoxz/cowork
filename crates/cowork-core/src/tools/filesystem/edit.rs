//! Edit file tool - surgical string replacement

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

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
        "Performs exact string replacements in files.\n\n\
         Usage:\n\
         - You must use your Read tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file.\n\
         - When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: spaces + line number + tab. Everything after that tab is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.\n\
         - ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.\n\
         - Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.\n\
         - The edit will FAIL if old_string is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use replace_all to change every instance of old_string.\n\
         - Use replace_all for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance."
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

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
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

            // Count occurrences
            let occurrences = content.matches(old_string).count();

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

            // Perform replacement
            let new_content = if replace_all {
                content.replace(old_string, new_string)
            } else {
                content.replacen(old_string, new_string, 1)
            };

            // Calculate diff info
            let old_lines = content.lines().count();
            let new_lines = new_content.lines().count();
            let lines_changed = (new_lines as i64 - old_lines as i64).abs();

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

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::High
    }
}
