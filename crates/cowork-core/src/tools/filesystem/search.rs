//! Search files tool

use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{Tool, ToolOutput};

use super::validate_path;

/// Tool for searching files by name or content
pub struct SearchFiles {
    workspace: PathBuf,
}

impl SearchFiles {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for SearchFiles {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for files by name pattern or content. Returns matching file paths."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to search in (relative to workspace)",
                    "default": "."
                },
                "pattern": {
                    "type": "string",
                    "description": "Filename pattern (glob-style: *.rs, **/*.txt)"
                },
                "content": {
                    "type": "string",
                    "description": "Search for files containing this text/regex"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "default": 100
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
        let path_str = params["path"].as_str().unwrap_or(".");
        let pattern = params["pattern"].as_str();
        let content_search = params["content"].as_str();
        let max_results = params["max_results"].as_u64().unwrap_or(100) as usize;

        let path = self.workspace.join(path_str);
        let validated = validate_path(&path, &self.workspace)?;

        let mut results = Vec::new();
        let glob_pattern: Option<glob::Pattern> = pattern.and_then(|p| glob::Pattern::new(p).ok());
        let content_regex: Option<Regex> = content_search.and_then(|c| Regex::new(c).ok());

        for entry in walkdir::WalkDir::new(&validated)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if results.len() >= max_results {
                break;
            }

            let file_name = entry.file_name().to_string_lossy();

            // Check filename pattern
            if let Some(ref glob) = glob_pattern {
                if !glob.matches(&file_name) {
                    continue;
                }
            }

            // Check content
            if let Some(ref regex) = content_regex {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if !regex.is_match(&content) {
                        continue;
                    }
                } else {
                    continue; // Skip binary files
                }
            }

            let rel_path = entry
                .path()
                .strip_prefix(&self.workspace)
                .unwrap_or(entry.path());

            results.push(json!({
                "path": rel_path.display().to_string(),
                "name": file_name,
            }));
        }

        Ok(ToolOutput::success(json!({
            "results": results,
            "count": results.len(),
            "truncated": results.len() >= max_results
        })))
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}
