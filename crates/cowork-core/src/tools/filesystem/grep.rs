//! Grep content search tool - A powerful search tool built on regex
//!
//! Supports full regex syntax, file filtering, context lines, and multiple output modes.

use regex::{Regex, RegexBuilder};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// File type mappings (similar to ripgrep --type)
fn get_type_extensions(type_name: &str) -> Option<Vec<&'static str>> {
    let types: HashMap<&str, Vec<&str>> = [
        ("js", vec!["js", "mjs", "cjs", "jsx"]),
        ("ts", vec!["ts", "tsx", "mts", "cts"]),
        ("py", vec!["py", "pyi", "pyw"]),
        ("rust", vec!["rs"]),
        ("go", vec!["go"]),
        ("java", vec!["java"]),
        ("c", vec!["c", "h"]),
        ("cpp", vec!["cpp", "cc", "cxx", "hpp", "hh", "hxx", "c++", "h++"]),
        ("cs", vec!["cs"]),
        ("rb", vec!["rb", "rake", "gemspec"]),
        ("php", vec!["php", "phtml", "php3", "php4", "php5"]),
        ("html", vec!["html", "htm", "xhtml"]),
        ("css", vec!["css", "scss", "sass", "less"]),
        ("json", vec!["json", "jsonc"]),
        ("yaml", vec!["yaml", "yml"]),
        ("xml", vec!["xml", "xsd", "xsl", "xslt"]),
        ("md", vec!["md", "markdown"]),
        ("sh", vec!["sh", "bash", "zsh"]),
        ("sql", vec!["sql"]),
        ("swift", vec!["swift"]),
        ("kotlin", vec!["kt", "kts"]),
        ("scala", vec!["scala", "sc"]),
    ]
    .into_iter()
    .collect();

    types.get(type_name).cloned()
}

/// Tool for searching file contents with regex support
pub struct GrepFiles {
    workspace: PathBuf,
}

impl GrepFiles {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[derive(Debug, Clone)]
struct GrepMatch {
    file: String,
    line_number: usize,
    content: String,
    context_before: Vec<(usize, String)>,
    context_after: Vec<(usize, String)>,
}

impl Tool for GrepFiles {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "A powerful search tool built on ripgrep. \
         Supports full regex syntax (e.g., 'log.*Error', 'function\\s+\\w+'). \
         Filter files with glob parameter or type parameter. \
         Output modes: 'content' shows matching lines, 'files_with_matches' shows only file paths, \
         'count' shows match counts."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in. Defaults to workspace root."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g., '*.js', '**/*.tsx')"
                },
                "output_mode": {
                    "type": "string",
                    "description": "Output mode: 'content', 'files_with_matches', or 'count'",
                    "enum": ["content", "files_with_matches", "count"],
                    "default": "files_with_matches"
                },
                "-B": {
                    "type": "integer",
                    "description": "Number of lines to show before each match (like grep -B)",
                    "default": 0
                },
                "-A": {
                    "type": "integer",
                    "description": "Number of lines to show after each match (like grep -A)",
                    "default": 0
                },
                "-C": {
                    "type": "integer",
                    "description": "Number of lines to show before AND after each match (like grep -C)",
                    "default": 0
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers in output",
                    "default": true
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search",
                    "default": false
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (e.g., 'js', 'py', 'rust', 'go', 'java')"
                },
                "head_limit": {
                    "type": "integer",
                    "description": "Limit output to first N entries",
                    "default": 0
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip first N entries before applying head_limit",
                    "default": 0
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline mode where . matches newlines",
                    "default": false
                }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let pattern_str = params["pattern"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("pattern is required".into()))?;

            // Parse options
            let case_insensitive = params["-i"].as_bool().unwrap_or(false);
            let multiline = params["multiline"].as_bool().unwrap_or(false);
            let show_line_numbers = params["-n"].as_bool().unwrap_or(true);

            // Build regex
            let regex = RegexBuilder::new(pattern_str)
                .case_insensitive(case_insensitive)
                .multi_line(multiline)
                .dot_matches_new_line(multiline)
                .build()
                .map_err(|e| ToolError::InvalidParams(format!("Invalid regex: {}", e)))?;

            // Determine path
            let base_path = if let Some(path) = params["path"].as_str() {
                self.workspace.join(path)
            } else {
                self.workspace.clone()
            };

            // Context lines - -C overrides -A and -B
            let context_combined = params["-C"].as_u64().unwrap_or(0) as usize;
            let context_before = if context_combined > 0 {
                context_combined
            } else {
                params["-B"].as_u64().unwrap_or(0) as usize
            };
            let context_after = if context_combined > 0 {
                context_combined
            } else {
                params["-A"].as_u64().unwrap_or(0) as usize
            };

            let output_mode = params["output_mode"]
                .as_str()
                .unwrap_or("files_with_matches");
            let head_limit = params["head_limit"].as_u64().unwrap_or(0) as usize;
            let offset = params["offset"].as_u64().unwrap_or(0) as usize;

            // Get files to search
            let files = self.get_files_to_search(&base_path, &params).await?;

            // Process based on output mode
            match output_mode {
                "files_with_matches" => {
                    let mut matching_files = Vec::new();

                    for file_path in files {
                        if is_binary_file(&file_path).await {
                            continue;
                        }

                        if self.file_has_match(&file_path, &regex, multiline).await {
                            let relative = self.relative_path(&file_path);
                            matching_files.push(relative);
                        }
                    }

                    // Apply offset and limit
                    let total = matching_files.len();
                    let result: Vec<_> = matching_files
                        .into_iter()
                        .skip(offset)
                        .take(if head_limit > 0 { head_limit } else { usize::MAX })
                        .collect();

                    Ok(ToolOutput::success(json!({
                        "files": result,
                        "count": result.len(),
                        "total_matches": total,
                        "pattern": pattern_str
                    })))
                }
                "count" => {
                    let mut file_counts: Vec<(String, usize)> = Vec::new();
                    let mut total_count = 0;

                    for file_path in files {
                        if is_binary_file(&file_path).await {
                            continue;
                        }

                        let count = self.count_matches(&file_path, &regex, multiline).await;
                        if count > 0 {
                            let relative = self.relative_path(&file_path);
                            file_counts.push((relative, count));
                            total_count += count;
                        }
                    }

                    // Apply offset and limit
                    let result: Vec<_> = file_counts
                        .into_iter()
                        .skip(offset)
                        .take(if head_limit > 0 { head_limit } else { usize::MAX })
                        .map(|(f, c)| json!({ "file": f, "count": c }))
                        .collect();

                    Ok(ToolOutput::success(json!({
                        "counts": result,
                        "total_matches": total_count,
                        "pattern": pattern_str
                    })))
                }
                "content" | _ => {
                    let mut matches: Vec<GrepMatch> = Vec::new();

                    for file_path in files {
                        if is_binary_file(&file_path).await {
                            continue;
                        }

                        let file_matches = self
                            .find_matches(&file_path, &regex, context_before, context_after, multiline)
                            .await;
                        matches.extend(file_matches);
                    }

                    // Apply offset and limit
                    let total = matches.len();
                    let matches: Vec<_> = matches
                        .into_iter()
                        .skip(offset)
                        .take(if head_limit > 0 { head_limit } else { usize::MAX })
                        .collect();

                    // Format output
                    let formatted: Vec<Value> = matches
                        .iter()
                        .map(|m| {
                            let mut entry = json!({
                                "file": m.file,
                                "content": m.content,
                            });

                            if show_line_numbers {
                                entry["line"] = json!(m.line_number);
                            }

                            if !m.context_before.is_empty() {
                                entry["context_before"] = json!(m
                                    .context_before
                                    .iter()
                                    .map(|(n, s)| if show_line_numbers {
                                        json!({ "line": n, "content": s })
                                    } else {
                                        json!(s)
                                    })
                                    .collect::<Vec<_>>());
                            }

                            if !m.context_after.is_empty() {
                                entry["context_after"] = json!(m
                                    .context_after
                                    .iter()
                                    .map(|(n, s)| if show_line_numbers {
                                        json!({ "line": n, "content": s })
                                    } else {
                                        json!(s)
                                    })
                                    .collect::<Vec<_>>());
                            }

                            entry
                        })
                        .collect();

                    Ok(ToolOutput::success(json!({
                        "matches": formatted,
                        "count": formatted.len(),
                        "total_matches": total,
                        "pattern": pattern_str
                    })))
                }
            }
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}

impl GrepFiles {
    fn relative_path(&self, path: &PathBuf) -> String {
        path.strip_prefix(&self.workspace)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string())
    }

    async fn get_files_to_search(
        &self,
        base_path: &PathBuf,
        params: &Value,
    ) -> Result<Vec<PathBuf>, ToolError> {
        if base_path.is_file() {
            return Ok(vec![base_path.clone()]);
        }

        let file_glob = params["glob"].as_str();
        let file_type = params["type"].as_str();

        // Build glob pattern
        let glob_pattern = if let Some(glob) = file_glob {
            base_path.join(glob).to_string_lossy().to_string()
        } else if let Some(type_name) = file_type {
            if let Some(extensions) = get_type_extensions(type_name) {
                let ext_pattern = if extensions.len() == 1 {
                    extensions[0].to_string()
                } else {
                    format!("{{{}}}", extensions.join(","))
                };
                base_path
                    .join(format!("**/*.{}", ext_pattern))
                    .to_string_lossy()
                    .to_string()
            } else {
                // Unknown type, search all files
                base_path.join("**/*").to_string_lossy().to_string()
            }
        } else {
            base_path.join("**/*").to_string_lossy().to_string()
        };

        let files: Vec<PathBuf> = glob::glob(&glob_pattern)
            .map_err(|e| ToolError::InvalidParams(format!("Invalid glob: {}", e)))?
            .filter_map(|e| e.ok())
            .filter(|p| p.is_file())
            .collect();

        Ok(files)
    }

    async fn file_has_match(&self, path: &PathBuf, regex: &Regex, multiline: bool) -> bool {
        if multiline {
            // For multiline, read entire file
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                return regex.is_match(&content);
            }
        } else {
            // Line-by-line
            if let Ok(file) = tokio::fs::File::open(path).await {
                let reader = BufReader::new(file);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if regex.is_match(&line) {
                        return true;
                    }
                }
            }
        }
        false
    }

    async fn count_matches(&self, path: &PathBuf, regex: &Regex, multiline: bool) -> usize {
        if multiline {
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                return regex.find_iter(&content).count();
            }
        } else {
            if let Ok(file) = tokio::fs::File::open(path).await {
                let reader = BufReader::new(file);
                let mut lines = reader.lines();
                let mut count = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    count += regex.find_iter(&line).count();
                }
                return count;
            }
        }
        0
    }

    async fn find_matches(
        &self,
        path: &PathBuf,
        regex: &Regex,
        context_before: usize,
        context_after: usize,
        multiline: bool,
    ) -> Vec<GrepMatch> {
        let relative = self.relative_path(path);
        let mut matches = Vec::new();

        if multiline {
            // For multiline patterns, we need to handle differently
            if let Ok(content) = tokio::fs::read_to_string(path).await {
                let lines: Vec<&str> = content.lines().collect();

                for mat in regex.find_iter(&content) {
                    // Find which line(s) the match is on
                    let match_start = mat.start();
                    let mut current_pos = 0;
                    let mut match_line = 0;

                    for (i, line) in lines.iter().enumerate() {
                        let line_end = current_pos + line.len() + 1; // +1 for newline
                        if match_start < line_end {
                            match_line = i;
                            break;
                        }
                        current_pos = line_end;
                    }

                    let before: Vec<(usize, String)> = (match_line.saturating_sub(context_before)
                        ..match_line)
                        .map(|i| (i + 1, lines[i].to_string()))
                        .collect();

                    let after: Vec<(usize, String)> = ((match_line + 1)
                        ..std::cmp::min(match_line + 1 + context_after, lines.len()))
                        .map(|i| (i + 1, lines[i].to_string()))
                        .collect();

                    matches.push(GrepMatch {
                        file: relative.clone(),
                        line_number: match_line + 1,
                        content: mat.as_str().to_string(),
                        context_before: before,
                        context_after: after,
                    });
                }
            }
        } else {
            // Line-by-line matching
            if let Ok(file) = tokio::fs::File::open(path).await {
                let reader = BufReader::new(file);
                let mut lines = reader.lines();
                let mut line_buffer: Vec<(usize, String)> = Vec::new();
                let mut line_number = 0usize;

                while let Ok(Some(line)) = lines.next_line().await {
                    line_number += 1;

                    // Keep context buffer
                    line_buffer.push((line_number, line.clone()));
                    if line_buffer.len() > context_before + 1 {
                        line_buffer.remove(0);
                    }

                    if regex.is_match(&line) {
                        // Get context before (from buffer, excluding current line)
                        let before: Vec<(usize, String)> = line_buffer
                            .iter()
                            .take(line_buffer.len().saturating_sub(1))
                            .cloned()
                            .collect();

                        // Collect context after
                        let mut after: Vec<(usize, String)> = Vec::new();
                        for _ in 0..context_after {
                            if let Ok(Some(next_line)) = lines.next_line().await {
                                line_number += 1;
                                after.push((line_number, next_line));
                            } else {
                                break;
                            }
                        }

                        matches.push(GrepMatch {
                            file: relative.clone(),
                            line_number: line_number - after.len(),
                            content: line,
                            context_before: before,
                            context_after: after,
                        });

                        // Reset buffer for next match
                        line_buffer.clear();
                    }
                }
            }
        }

        matches
    }
}

/// Check if a file is likely binary
async fn is_binary_file(path: &PathBuf) -> bool {
    // Check extension first
    let binary_extensions = [
        "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg", "pdf", "doc", "docx", "xls",
        "xlsx", "ppt", "pptx", "zip", "tar", "gz", "bz2", "7z", "rar", "exe", "dll", "so", "dylib",
        "bin", "mp3", "mp4", "avi", "mov", "wav", "flac", "ttf", "otf", "woff", "woff2", "eot",
        "sqlite", "db",
    ];

    if let Some(ext) = path.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        if binary_extensions.contains(&ext_lower.as_str()) {
            return true;
        }
    }

    // Check first bytes for binary content
    if let Ok(mut file) = tokio::fs::File::open(path).await {
        let mut buffer = [0u8; 512];
        if let Ok(n) = file.read(&mut buffer).await {
            // Check for null bytes (common in binary files)
            if buffer[..n].contains(&0) {
                return true;
            }
        }
    }

    false
}
