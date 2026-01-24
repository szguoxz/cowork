//! Document extraction helper for the Read tool
//!
//! Supports extracting text from PDF, Word, Excel, and PowerPoint files.

use std::io::{BufReader, Read};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde_json::json;

use crate::error::ToolError;
use crate::tools::ToolOutput;

use super::path_to_display;

/// Maximum output size in bytes to prevent DoS from very large documents
const MAX_OUTPUT_SIZE: usize = 512 * 1024; // 512 KB

/// Check if a file extension corresponds to a supported document format.
pub fn is_document(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "pptx"
    )
}

/// Extract text content from a document file, dispatching to the appropriate extractor.
pub fn extract_document(path: &Path) -> Result<ToolOutput, ToolError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (content, format) = match ext.as_str() {
        "pdf" => (extract_pdf_text(path)?, "pdf"),
        "doc" | "docx" => (extract_word_text(path)?, "word"),
        "xls" | "xlsx" => (extract_excel_text(path)?, "excel"),
        "pptx" => (extract_pptx_text(path)?, "powerpoint"),
        _ => {
            return Err(ToolError::InvalidParams(format!(
                "Unsupported document format: .{}",
                ext
            )));
        }
    };

    Ok(ToolOutput::success(json!({
        "path": path_to_display(path),
        "format": format,
        "content": content,
    })))
}

/// Extract text from a PDF file using pdf-extract
fn extract_pdf_text(path: &Path) -> Result<String, ToolError> {
    let mut text = pdf_extract::extract_text(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to extract PDF text: {}", e)))?;

    if text.len() > MAX_OUTPUT_SIZE {
        text.truncate(MAX_OUTPUT_SIZE);
        text.push_str("\n\n... [Content truncated due to size limit]");
    }

    Ok(text)
}

/// Extract text from a Word document (.docx) using dotext
fn extract_word_text(path: &Path) -> Result<String, ToolError> {
    use dotext::*;

    let mut text = String::new();
    Docx::open(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse DOCX: {}", e)))?
        .read_to_string(&mut text)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read DOCX content: {}", e)))?;

    if text.len() > MAX_OUTPUT_SIZE {
        text.truncate(MAX_OUTPUT_SIZE);
        text.push_str("\n\n... [Content truncated due to size limit]");
    }

    Ok(text)
}

/// Extract text from an Excel spreadsheet (.xlsx, .xls) using calamine
fn extract_excel_text(path: &Path) -> Result<String, ToolError> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open Excel file: {}", e)))?;

    let mut output = String::new();
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut truncated = false;

    'outer: for sheet_name in sheet_names {
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            output.push_str(&format!("=== Sheet: {} ===\n", sheet_name));

            for row in range.rows() {
                if output.len() >= MAX_OUTPUT_SIZE {
                    truncated = true;
                    break 'outer;
                }

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

                if cells.iter().all(|c| c.is_empty()) {
                    continue;
                }

                output.push_str(&cells.join("\t"));
                output.push('\n');
            }
            output.push('\n');
        }
    }

    if truncated {
        output.push_str("\n... [Content truncated due to size limit]");
    }

    Ok(output)
}

/// Extract text from a PowerPoint presentation (.pptx)
fn extract_pptx_text(path: &Path) -> Result<String, ToolError> {
    let file = std::fs::File::open(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open file: {}", e)))?;

    let reader = BufReader::new(file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open PPTX archive: {}", e)))?;

    let mut output = String::new();
    let mut slide_num = 1;
    let mut truncated = false;

    loop {
        if output.len() >= MAX_OUTPUT_SIZE {
            truncated = true;
            break;
        }

        let slide_path = format!("ppt/slides/slide{}.xml", slide_num);

        match archive.by_name(&slide_path) {
            Ok(mut slide_file) => {
                let mut xml_content = String::new();
                slide_file.read_to_string(&mut xml_content).map_err(|e| {
                    ToolError::ExecutionFailed(format!("Failed to read slide: {}", e))
                })?;

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
    } else if truncated {
        output.push_str("\n... [Content truncated due to size limit]");
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
                if e.name().as_ref() == b"a:t" {
                    in_text_element = true;
                }
            }
            Ok(Event::Text(e)) => {
                if in_text_element {
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
