//! List directory tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::{path_to_display, validate_path};

/// Tool for listing directory contents
pub struct ListDirectory {
    workspace: PathBuf,
}

impl ListDirectory {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for ListDirectory {
    fn name(&self) -> &str {
        "ListDirectory"
    }

    fn description(&self) -> &str {
        "List contents of a directory. Returns file and directory names with metadata."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the directory to list (relative to workspace)",
                    "default": "."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively",
                    "default": false
                },
                "include_hidden": {
                    "type": "boolean",
                    "description": "Include hidden files (starting with .)",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path_str = params["path"].as_str().unwrap_or(".");
            let recursive = params["recursive"].as_bool().unwrap_or(false);
            let include_hidden = params["include_hidden"].as_bool().unwrap_or(false);

            let path = self.workspace.join(path_str);
            let validated = validate_path(&path, &self.workspace)?;

            let mut entries = Vec::new();

            if recursive {
                for entry in walkdir::WalkDir::new(&validated)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !include_hidden && name.starts_with('.') {
                        continue;
                    }

                    let metadata = entry.metadata().ok();
                    entries.push(json!({
                        "name": name,
                        "path": entry.path().strip_prefix(&self.workspace)
                            .map(path_to_display)
                            .unwrap_or_else(|_| path_to_display(entry.path())),
                        "is_dir": entry.file_type().is_dir(),
                        "size": metadata.as_ref().map(|m| m.len()),
                    }));
                }
            } else {
                let mut dir = tokio::fs::read_dir(&validated).await.map_err(ToolError::Io)?;

                while let Some(entry) = dir.next_entry().await.map_err(ToolError::Io)? {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !include_hidden && name.starts_with('.') {
                        continue;
                    }

                    let metadata = entry.metadata().await.ok();
                    let file_type = entry.file_type().await.ok();

                    entries.push(json!({
                        "name": name,
                        "path": entry.path().strip_prefix(&self.workspace)
                            .map(path_to_display)
                            .unwrap_or_else(|_| path_to_display(&entry.path())),
                        "is_dir": file_type.map(|t| t.is_dir()).unwrap_or(false),
                        "size": metadata.as_ref().map(|m| m.len()),
                    }));
                }
            }

            Ok(ToolOutput::success(json!({
                "entries": entries,
                "count": entries.len()
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
