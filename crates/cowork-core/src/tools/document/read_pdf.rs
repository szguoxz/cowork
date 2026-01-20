//! PDF reading tool

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::filesystem::{path_to_display, validate_path};
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// Tool for reading PDF documents
pub struct ReadPdf {
    workspace: PathBuf,
}

impl ReadPdf {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for ReadPdf {
    fn name(&self) -> &str {
        "read_pdf"
    }

    fn description(&self) -> &str {
        "Extract text content from a PDF file."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the PDF file"
                },
                "pages": {
                    "type": "string",
                    "description": "Page range to extract (e.g., '1-5', 'all')",
                    "default": "all"
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path_str = params["path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("path is required".into()))?;

            let pages_param = params["pages"].as_str().unwrap_or("all");

            let path = self.workspace.join(path_str);
            let validated = validate_path(&path, &self.workspace)?;

            // Check file exists
            if !validated.exists() {
                return Err(ToolError::ExecutionFailed(format!(
                    "File not found: {}",
                    validated.display()
                )));
            }

            // Check extension
            let ext = validated
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext != "pdf" {
                return Err(ToolError::InvalidParams(format!(
                    "Expected PDF file, got .{} extension",
                    ext
                )));
            }

            // Extract text using pdf-extract
            let extracted_text = match pdf_extract::extract_text(&validated) {
                Ok(text) => text,
                Err(e) => {
                    return Err(ToolError::ExecutionFailed(format!(
                        "Failed to extract PDF text: {}",
                        e
                    )));
                }
            };

            // Count approximate pages (pdf-extract doesn't expose page count easily,
            // but we can estimate from form feeds or just return the full text)
            let page_count = extracted_text.matches('\u{0C}').count() + 1; // Form feed = page break

            // Handle page range filtering if specified
            let output_text = if pages_param == "all" {
                extracted_text
            } else {
                // Parse page range like "1-5" or "3"
                let (start, end) = parse_page_range(pages_param, page_count)?;

                // Split by form feeds and extract requested pages
                let pages: Vec<&str> = extracted_text.split('\u{0C}').collect();
                let selected: Vec<&str> = pages
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i >= start - 1 && *i < end)
                    .map(|(_, p)| *p)
                    .collect();
                selected.join("\n\n--- Page Break ---\n\n")
            };

            Ok(ToolOutput::success(json!({
                "path": path_to_display(&validated),
                "text": output_text,
                "pages": page_count,
                "extracted_pages": pages_param
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}

/// Parse a page range string like "1-5" or "3" into (start, end) 1-indexed
fn parse_page_range(range: &str, max_pages: usize) -> Result<(usize, usize), ToolError> {
    let range = range.trim();

    if range.contains('-') {
        let parts: Vec<&str> = range.split('-').collect();
        if parts.len() != 2 {
            return Err(ToolError::InvalidParams(
                "Invalid page range format. Use '1-5' or 'all'".into(),
            ));
        }
        let start: usize = parts[0]
            .trim()
            .parse()
            .map_err(|_| ToolError::InvalidParams("Invalid start page number".into()))?;
        let end: usize = parts[1]
            .trim()
            .parse()
            .map_err(|_| ToolError::InvalidParams("Invalid end page number".into()))?;

        if start == 0 || end == 0 {
            return Err(ToolError::InvalidParams(
                "Page numbers are 1-indexed".into(),
            ));
        }
        if start > end {
            return Err(ToolError::InvalidParams(
                "Start page must be <= end page".into(),
            ));
        }
        if start > max_pages {
            return Err(ToolError::InvalidParams(format!(
                "Start page {} exceeds document pages ({})",
                start, max_pages
            )));
        }

        Ok((start, end.min(max_pages)))
    } else {
        // Single page
        let page: usize = range
            .parse()
            .map_err(|_| ToolError::InvalidParams("Invalid page number".into()))?;
        if page == 0 {
            return Err(ToolError::InvalidParams(
                "Page numbers are 1-indexed".into(),
            ));
        }
        if page > max_pages {
            return Err(ToolError::InvalidParams(format!(
                "Page {} exceeds document pages ({})",
                page, max_pages
            )));
        }
        Ok((page, page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page_range_all() {
        // "all" should be handled before parse_page_range is called
    }

    #[test]
    fn test_parse_page_range_single() {
        assert_eq!(parse_page_range("3", 10).unwrap(), (3, 3));
        assert_eq!(parse_page_range("1", 5).unwrap(), (1, 1));
    }

    #[test]
    fn test_parse_page_range_range() {
        assert_eq!(parse_page_range("1-5", 10).unwrap(), (1, 5));
        assert_eq!(parse_page_range("2-8", 5).unwrap(), (2, 5)); // Capped at max
    }

    #[test]
    fn test_parse_page_range_invalid() {
        assert!(parse_page_range("0", 10).is_err()); // 0 is invalid
        assert!(parse_page_range("15", 10).is_err()); // Exceeds max
        assert!(parse_page_range("5-3", 10).is_err()); // Start > end
        assert!(parse_page_range("abc", 10).is_err()); // Not a number
    }
}
