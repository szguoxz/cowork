//! Office document reading tool

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde_json::{json, Value};
use std::io::{BufReader, Read};
use std::path::PathBuf;

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::filesystem::{path_to_display, validate_path};
use crate::tools::{BoxFuture, Tool, ToolOutput};

use super::DocumentFormat;

/// Tool for reading Office documents (Word, Excel, PowerPoint)
pub struct ReadOfficeDoc {
    workspace: PathBuf,
}

impl ReadOfficeDoc {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for ReadOfficeDoc {
    fn name(&self) -> &str {
        "read_office_doc"
    }

    fn description(&self) -> &str {
        "Extract content from Office documents (Word, Excel, PowerPoint)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the Office document"
                },
                "extract_images": {
                    "type": "boolean",
                    "description": "Also extract embedded images (not yet supported)",
                    "default": false
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

            let _extract_images = params["extract_images"].as_bool().unwrap_or(false);

            let path = self.workspace.join(path_str);
            let validated = validate_path(&path, &self.workspace)?;

            // Check file exists
            if !validated.exists() {
                return Err(ToolError::ExecutionFailed(format!(
                    "File not found: {}",
                    validated.display()
                )));
            }

            let ext = validated
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let format = DocumentFormat::from_extension(&ext);

            let content = match format {
                DocumentFormat::Word => extract_word_text(&validated)?,
                DocumentFormat::Excel => extract_excel_text(&validated)?,
                DocumentFormat::PowerPoint => extract_pptx_text(&validated)?,
                _ => {
                    return Err(ToolError::InvalidParams(format!(
                        "Unsupported format: .{} (expected .docx, .xlsx, or .pptx)",
                        ext
                    )));
                }
            };

            Ok(ToolOutput::success(json!({
                "path": path_to_display(&validated),
                "format": format!("{:?}", format),
                "content": content
            })))
        })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}

/// Extract text from a Word document (.docx)
fn extract_word_text(path: &std::path::Path) -> Result<String, ToolError> {
    use dotext::*;

    let mut text = String::new();
    Docx::open(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse DOCX: {}", e)))?
        .read_to_string(&mut text)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read DOCX content: {}", e)))?;

    Ok(text)
}

/// Extract text from an Excel spreadsheet (.xlsx, .xls)
fn extract_excel_text(path: &std::path::Path) -> Result<String, ToolError> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open Excel file: {}", e)))?;

    let mut output = String::new();
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

    for sheet_name in sheet_names {
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            output.push_str(&format!("=== Sheet: {} ===\n", sheet_name));

            for row in range.rows() {
                let cells: Vec<String> = row
                    .iter()
                    .map(|cell| match cell {
                        Data::Empty => String::new(),
                        Data::String(s) => s.clone(),
                        Data::Float(f) => f.to_string(),
                        Data::Int(i) => i.to_string(),
                        Data::Bool(b) => b.to_string(),
                        Data::Error(e) => format!("#ERR:{:?}", e),
                        Data::DateTime(dt) => format!("{}", dt),
                        Data::DateTimeIso(s) => s.clone(),
                        Data::DurationIso(s) => s.clone(),
                    })
                    .collect();

                // Skip empty rows
                if cells.iter().all(|c| c.is_empty()) {
                    continue;
                }

                output.push_str(&cells.join("\t"));
                output.push('\n');
            }
            output.push('\n');
        }
    }

    Ok(output)
}

/// Extract text from a PowerPoint presentation (.pptx)
fn extract_pptx_text(path: &std::path::Path) -> Result<String, ToolError> {
    let file = std::fs::File::open(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open file: {}", e)))?;

    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open PPTX archive: {}", e)))?;

    let mut output = String::new();
    let mut slide_num = 1;

    // PPTX files store slides in ppt/slides/slide1.xml, slide2.xml, etc.
    loop {
        let slide_path = format!("ppt/slides/slide{}.xml", slide_num);

        match archive.by_name(&slide_path) {
            Ok(mut slide_file) => {
                let mut xml_content = String::new();
                slide_file
                    .read_to_string(&mut xml_content)
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read slide: {}", e)))?;

                let slide_text = extract_text_from_pptx_xml(&xml_content)?;
                if !slide_text.trim().is_empty() {
                    output.push_str(&format!("=== Slide {} ===\n", slide_num));
                    output.push_str(&slide_text);
                    output.push_str("\n\n");
                }
                slide_num += 1;
            }
            Err(zip::result::ZipError::FileNotFound) => break,
            Err(e) => {
                return Err(ToolError::ExecutionFailed(format!(
                    "Failed to read slide {}: {}",
                    slide_num, e
                )));
            }
        }
    }

    if output.is_empty() {
        output = "(No text content found in presentation)".to_string();
    }

    Ok(output)
}

/// Extract text content from PPTX slide XML
fn extract_text_from_pptx_xml(xml: &str) -> Result<String, ToolError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut output = String::new();
    let mut in_text_element = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                // Text content in PPTX is in <a:t> elements
                if e.name().as_ref() == b"a:t" {
                    in_text_element = true;
                }
            }
            Ok(Event::Text(e)) => {
                if in_text_element {
                    // quick-xml 0.38+ replaced unescape() with decode()
                    // Entity references are now reported as Event::GeneralRef
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    if !text.trim().is_empty() {
                        output.push_str(&text);
                        output.push(' ');
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"a:t" {
                    in_text_element = false;
                }
                // Add newline after paragraphs
                if e.name().as_ref() == b"a:p" {
                    output.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ToolError::ExecutionFailed(format!(
                    "XML parsing error: {}",
                    e
                )));
            }
            _ => {}
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_format_detection() {
        assert_eq!(DocumentFormat::from_extension("docx"), DocumentFormat::Word);
        assert_eq!(DocumentFormat::from_extension("doc"), DocumentFormat::Word);
        assert_eq!(DocumentFormat::from_extension("xlsx"), DocumentFormat::Excel);
        assert_eq!(DocumentFormat::from_extension("xls"), DocumentFormat::Excel);
        assert_eq!(
            DocumentFormat::from_extension("pptx"),
            DocumentFormat::PowerPoint
        );
        assert_eq!(
            DocumentFormat::from_extension("ppt"),
            DocumentFormat::PowerPoint
        );
        assert_eq!(DocumentFormat::from_extension("txt"), DocumentFormat::Text);
        assert_eq!(
            DocumentFormat::from_extension("unknown"),
            DocumentFormat::Unknown
        );
    }
}
