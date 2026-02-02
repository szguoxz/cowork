//! Document export tool for generating PDF, Word, Excel, and HTML slides
//!
//! This module provides functionality to export content to various document formats:
//! - PDF: Using genpdf for rich text layout
//! - Word (DOCX): Using docx-rs for Office-compatible documents
//! - Excel (XLSX): Using rust_xlsxwriter for spreadsheets
//! - PowerPoint (HTML): HTML-based slides that can be opened in any browser

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

use super::{normalize_path, path_to_display, validate_path};

/// Tool for exporting content to document formats (PDF, DOCX, XLSX, HTML slides)
pub struct ExportDocument {
    workspace: PathBuf,
}

impl ExportDocument {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Tool for ExportDocument {
    fn name(&self) -> &str {
        "ExportDocument"
    }

    fn description(&self) -> &str {
        r#"Export content to document formats (PDF, Word, Excel, or HTML slides).

Usage:
- file_path: Absolute path for the output file. Extension determines format:
  - .pdf: PDF document
  - .docx: Word document
  - .xlsx: Excel spreadsheet
  - .pptx.html or .slides.html: HTML presentation slides

- content: The content to export. Format depends on document type:
  - For PDF/Word: Plain text or markdown-like content (paragraphs separated by blank lines)
  - For Excel: TSV (tab-separated values) or JSON array of rows
  - For slides: JSON array of slide objects with "title" and "content" fields

Examples:

PDF/Word:
{
  "file_path": "/path/to/report.pdf",
  "title": "Quarterly Report",
  "content": "First paragraph.\n\nSecond paragraph with more details."
}

Excel:
{
  "file_path": "/path/to/data.xlsx",
  "content": "Name\tAge\tCity\nAlice\t30\tNYC\nBob\t25\tLA"
}

Or Excel with JSON:
{
  "file_path": "/path/to/data.xlsx",
  "content": "[{\"Name\":\"Alice\",\"Age\":30},{\"Name\":\"Bob\",\"Age\":25}]"
}

HTML Slides:
{
  "file_path": "/path/to/presentation.slides.html",
  "title": "My Presentation",
  "content": "[{\"title\":\"Introduction\",\"content\":\"Welcome to the presentation\"},{\"title\":\"Conclusion\",\"content\":\"Thank you!\"}]"
}"#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path for the output file. Extension determines format (.pdf, .docx, .xlsx, .slides.html)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to export (text for PDF/Word, TSV/JSON for Excel, JSON array for slides)"
                },
                "title": {
                    "type": "string",
                    "description": "Optional document title"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    fn execute(&self, params: Value, _ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let path_str = params["file_path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("file_path is required".into()))?;

            let content = params["content"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;

            let title = params["title"].as_str().unwrap_or("Document");

            let path = self.workspace.join(path_str);

            // Normalize and validate path
            let normalized_path = normalize_path(&path);
            let normalized_workspace = normalize_path(&self.workspace);

            if !normalized_path.starts_with(&normalized_workspace) {
                return Err(ToolError::PermissionDenied(format!(
                    "Path {} is outside workspace",
                    path.display()
                )));
            }

            // Create parent directories if needed
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    tokio::fs::create_dir_all(parent).await.map_err(ToolError::Io)?;
                } else {
                    validate_path(parent, &self.workspace)?;
                }
            }

            // Determine format from extension
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let path_lower = path.to_string_lossy().to_lowercase();

            let (format, bytes_written) = if path_lower.ends_with(".slides.html") || path_lower.ends_with(".pptx.html") {
                let bytes = export_html_slides(&path, title, content)?;
                ("html_slides", bytes)
            } else {
                match ext.as_str() {
                    "pdf" => {
                        let bytes = export_pdf(&path, title, content)?;
                        ("pdf", bytes)
                    }
                    "docx" => {
                        let bytes = export_docx(&path, title, content)?;
                        ("docx", bytes)
                    }
                    "xlsx" => {
                        let bytes = export_xlsx(&path, title, content)?;
                        ("xlsx", bytes)
                    }
                    "html" => {
                        let bytes = export_html_slides(&path, title, content)?;
                        ("html_slides", bytes)
                    }
                    _ => {
                        return Err(ToolError::InvalidParams(format!(
                            "Unsupported export format: .{}. Use .pdf, .docx, .xlsx, or .slides.html",
                            ext
                        )));
                    }
                }
            };

            Ok(ToolOutput::success(json!({
                "path": path_to_display(&path),
                "format": format,
                "bytes_written": bytes_written
            })))
        })
    }
}

/// Export content to PDF using genpdf
fn export_pdf(path: &std::path::Path, title: &str, content: &str) -> Result<usize, ToolError> {
    use genpdf::{elements, fonts, style, Document, Element};

    // Try to load system fonts from common locations
    let font_family = fonts::from_files("/usr/share/fonts/truetype/dejavu", "DejaVuSans", None)
        .or_else(|_| fonts::from_files("/usr/share/fonts/truetype/liberation", "LiberationSans", None))
        .or_else(|_| fonts::from_files("/System/Library/Fonts", "Helvetica", None))
        .or_else(|_| fonts::from_files("C:\\Windows\\Fonts", "arial", None))
        .map_err(|e| ToolError::ExecutionFailed(format!(
            "No suitable fonts found for PDF generation. Please install dejavu-fonts or liberation-fonts. Error: {}",
            e
        )))?;

    let mut doc = Document::new(font_family);
    doc.set_title(title);

    // Add title
    let title_style = style::Style::new().with_font_size(18).bold();
    doc.push(elements::Paragraph::new(title).styled(title_style));
    doc.push(elements::Break::new(1));

    // Add content paragraphs
    for paragraph in content.split("\n\n") {
        let trimmed = paragraph.trim();
        if !trimmed.is_empty() {
            // Check if it's a heading (starts with #)
            if trimmed.starts_with("# ") {
                let heading = &trimmed[2..];
                let heading_style = style::Style::new().with_font_size(16).bold();
                doc.push(elements::Paragraph::new(heading).styled(heading_style));
            } else if trimmed.starts_with("## ") {
                let heading = &trimmed[3..];
                let heading_style = style::Style::new().with_font_size(14).bold();
                doc.push(elements::Paragraph::new(heading).styled(heading_style));
            } else if trimmed.starts_with("### ") {
                let heading = &trimmed[4..];
                let heading_style = style::Style::new().with_font_size(12).bold();
                doc.push(elements::Paragraph::new(heading).styled(heading_style));
            } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                // Bullet list
                for line in trimmed.lines() {
                    let item = line.trim_start_matches(|c| c == '-' || c == '*' || c == ' ');
                    doc.push(elements::Paragraph::new(format!("• {}", item)));
                }
            } else {
                doc.push(elements::Paragraph::new(trimmed));
            }
            doc.push(elements::Break::new(0.5));
        }
    }

    // Render to file
    doc.render_to_file(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to generate PDF: {}", e)))?;

    // Get file size
    let metadata = std::fs::metadata(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read PDF metadata: {}", e)))?;

    Ok(metadata.len() as usize)
}

/// Export content to Word document using docx-rs
fn export_docx(path: &std::path::Path, title: &str, content: &str) -> Result<usize, ToolError> {
    use docx_rs::*;

    let mut doc = Docx::new();

    // Add title
    doc = doc.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text(title).bold().size(36))
    );

    // Add empty line after title
    doc = doc.add_paragraph(Paragraph::new());

    // Add content paragraphs
    for paragraph in content.split("\n\n") {
        let trimmed = paragraph.trim();
        if !trimmed.is_empty() {
            if trimmed.starts_with("# ") {
                // H1 heading
                let heading = &trimmed[2..];
                doc = doc.add_paragraph(
                    Paragraph::new()
                        .add_run(Run::new().add_text(heading).bold().size(32))
                );
            } else if trimmed.starts_with("## ") {
                // H2 heading
                let heading = &trimmed[3..];
                doc = doc.add_paragraph(
                    Paragraph::new()
                        .add_run(Run::new().add_text(heading).bold().size(28))
                );
            } else if trimmed.starts_with("### ") {
                // H3 heading
                let heading = &trimmed[4..];
                doc = doc.add_paragraph(
                    Paragraph::new()
                        .add_run(Run::new().add_text(heading).bold().size(24))
                );
            } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                // Bullet list items
                for line in trimmed.lines() {
                    let item = line.trim_start_matches(|c| c == '-' || c == '*' || c == ' ');
                    doc = doc.add_paragraph(
                        Paragraph::new()
                            .add_run(Run::new().add_text(format!("• {}", item)))
                    );
                }
            } else {
                // Regular paragraph - handle line breaks within
                for line in trimmed.lines() {
                    doc = doc.add_paragraph(
                        Paragraph::new()
                            .add_run(Run::new().add_text(line))
                    );
                }
            }
        }
    }

    // Write to file
    let file = std::fs::File::create(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create DOCX file: {}", e)))?;

    doc.build()
        .pack(file)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write DOCX: {}", e)))?;

    // Get file size
    let metadata = std::fs::metadata(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read DOCX metadata: {}", e)))?;

    Ok(metadata.len() as usize)
}

/// Export content to Excel spreadsheet using rust_xlsxwriter
fn export_xlsx(path: &std::path::Path, title: &str, content: &str) -> Result<usize, ToolError> {
    use rust_xlsxwriter::*;

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    // Set worksheet name (max 31 chars)
    let sheet_name = if title.len() > 31 { &title[..31] } else { title };
    worksheet.set_name(sheet_name)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to set sheet name: {}", e)))?;

    // Try to parse as JSON array first
    if content.trim().starts_with('[') {
        if let Ok(rows) = serde_json::from_str::<Vec<serde_json::Map<String, Value>>>(content) {
            if !rows.is_empty() {
                // Write headers from first row's keys
                let headers: Vec<&String> = rows[0].keys().collect();
                let header_format = Format::new().set_bold();

                for (col, header) in headers.iter().enumerate() {
                    worksheet.write_string_with_format(0, col as u16, *header, &header_format)
                        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write header: {}", e)))?;
                }

                // Write data rows
                for (row_idx, row) in rows.iter().enumerate() {
                    for (col_idx, header) in headers.iter().enumerate() {
                        if let Some(value) = row.get(*header) {
                            let cell_value = match value {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                _ => value.to_string(),
                            };
                            worksheet.write_string((row_idx + 1) as u32, col_idx as u16, &cell_value)
                                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write cell: {}", e)))?;
                        }
                    }
                }
            }
        } else {
            // JSON parse failed, treat as TSV
            write_tsv_to_worksheet(worksheet, content)?;
        }
    } else {
        // Treat as TSV (tab-separated values)
        write_tsv_to_worksheet(worksheet, content)?;
    }

    // Auto-fit columns (estimate based on content)
    worksheet.autofit();

    // Save workbook
    workbook.save(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to save XLSX: {}", e)))?;

    // Get file size
    let metadata = std::fs::metadata(path)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read XLSX metadata: {}", e)))?;

    Ok(metadata.len() as usize)
}

/// Helper to write TSV content to Excel worksheet
fn write_tsv_to_worksheet(worksheet: &mut rust_xlsxwriter::Worksheet, content: &str) -> Result<(), ToolError> {
    use rust_xlsxwriter::Format;

    let lines: Vec<&str> = content.lines().collect();
    let header_format = Format::new().set_bold();

    for (row_idx, line) in lines.iter().enumerate() {
        let cells: Vec<&str> = line.split('\t').collect();
        for (col_idx, cell) in cells.iter().enumerate() {
            if row_idx == 0 {
                // First row as header
                worksheet.write_string_with_format(row_idx as u32, col_idx as u16, *cell, &header_format)
                    .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write header: {}", e)))?;
            } else {
                // Try to parse as number
                if let Ok(num) = cell.parse::<f64>() {
                    worksheet.write_number(row_idx as u32, col_idx as u16, num)
                        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write number: {}", e)))?;
                } else {
                    worksheet.write_string(row_idx as u32, col_idx as u16, *cell)
                        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write cell: {}", e)))?;
                }
            }
        }
    }

    Ok(())
}

/// Export content to HTML slides
fn export_html_slides(path: &std::path::Path, title: &str, content: &str) -> Result<usize, ToolError> {
    // Try to parse as JSON array of slides
    let slides: Vec<Slide> = if content.trim().starts_with('[') {
        serde_json::from_str(content)
            .map_err(|e| ToolError::ExecutionFailed(format!(
                "Failed to parse slides JSON: {}. Expected format: [{{\"title\": \"...\", \"content\": \"...\"}}]",
                e
            )))?
    } else {
        // Treat as simple text - split by "---" for slide breaks
        content
            .split("---")
            .enumerate()
            .map(|(i, slide_content)| {
                let lines: Vec<&str> = slide_content.trim().lines().collect();
                let slide_title = lines.first()
                    .map(|s| s.trim_start_matches('#').trim())
                    .unwrap_or(&format!("Slide {}", i + 1))
                    .to_string();
                let slide_body = lines.get(1..)
                    .map(|l| l.join("\n"))
                    .unwrap_or_default();
                Slide {
                    title: slide_title,
                    content: slide_body,
                }
            })
            .collect()
    };

    let html = generate_slides_html(title, &slides);

    std::fs::write(path, &html)
        .map_err(|e| ToolError::ExecutionFailed(format!("Failed to write HTML slides: {}", e)))?;

    Ok(html.len())
}

#[derive(serde::Deserialize)]
struct Slide {
    title: String,
    content: String,
}

/// Generate HTML for presentation slides
fn generate_slides_html(title: &str, slides: &[Slide]) -> String {
    let slides_html: String = slides
        .iter()
        .enumerate()
        .map(|(i, slide)| {
            let content_html = markdown_to_html(&slide.content);
            format!(
                r#"
        <section class="slide" id="slide-{idx}">
            <h2>{title}</h2>
            <div class="content">{content}</div>
        </section>"#,
                idx = i + 1,
                title = html_escape(&slide.title),
                content = content_html
            )
        })
        .collect();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: #1a1a2e;
            color: #eee;
            min-height: 100vh;
        }}
        .slide {{
            min-height: 100vh;
            padding: 60px 80px;
            display: flex;
            flex-direction: column;
            justify-content: center;
            border-bottom: 1px solid #333;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
        }}
        .slide h2 {{
            font-size: 3rem;
            margin-bottom: 40px;
            color: #00d4ff;
            text-shadow: 0 0 30px rgba(0, 212, 255, 0.3);
        }}
        .slide .content {{
            font-size: 1.5rem;
            line-height: 1.8;
            max-width: 1200px;
        }}
        .slide .content p {{
            margin-bottom: 1em;
        }}
        .slide .content ul, .slide .content ol {{
            margin-left: 40px;
            margin-bottom: 1em;
        }}
        .slide .content li {{
            margin-bottom: 0.5em;
        }}
        .slide .content code {{
            background: #2d2d44;
            padding: 2px 8px;
            border-radius: 4px;
            font-family: 'Fira Code', monospace;
        }}
        .slide .content pre {{
            background: #2d2d44;
            padding: 20px;
            border-radius: 8px;
            overflow-x: auto;
            margin: 1em 0;
        }}
        .slide .content h3 {{
            font-size: 2rem;
            color: #ff6b6b;
            margin: 1.5em 0 0.5em;
        }}
        .nav {{
            position: fixed;
            bottom: 20px;
            right: 20px;
            display: flex;
            gap: 10px;
            z-index: 1000;
        }}
        .nav button {{
            background: rgba(0, 212, 255, 0.2);
            border: 1px solid #00d4ff;
            color: #00d4ff;
            padding: 10px 20px;
            cursor: pointer;
            border-radius: 5px;
            font-size: 1rem;
            transition: all 0.3s;
        }}
        .nav button:hover {{
            background: rgba(0, 212, 255, 0.4);
        }}
        .slide-counter {{
            position: fixed;
            bottom: 20px;
            left: 20px;
            color: #666;
            font-size: 1rem;
        }}
        @media print {{
            .slide {{
                page-break-after: always;
                min-height: 100vh;
            }}
            .nav, .slide-counter {{
                display: none;
            }}
        }}
    </style>
</head>
<body>
    <div class="slides">
        <section class="slide" id="slide-0">
            <h2 style="font-size: 4rem;">{title}</h2>
        </section>
{slides}
    </div>

    <div class="nav">
        <button onclick="prevSlide()">← Previous</button>
        <button onclick="nextSlide()">Next →</button>
    </div>
    <div class="slide-counter">
        <span id="current">1</span> / <span id="total">{slide_count}</span>
    </div>

    <script>
        const slides = document.querySelectorAll('.slide');
        const total = slides.length;
        document.getElementById('total').textContent = total;

        let currentSlide = 0;

        function showSlide(n) {{
            currentSlide = Math.max(0, Math.min(n, total - 1));
            slides[currentSlide].scrollIntoView({{ behavior: 'smooth' }});
            document.getElementById('current').textContent = currentSlide + 1;
        }}

        function nextSlide() {{ showSlide(currentSlide + 1); }}
        function prevSlide() {{ showSlide(currentSlide - 1); }}

        document.addEventListener('keydown', (e) => {{
            if (e.key === 'ArrowRight' || e.key === ' ') nextSlide();
            if (e.key === 'ArrowLeft') prevSlide();
        }});

        // Update counter on scroll
        const observer = new IntersectionObserver((entries) => {{
            entries.forEach(entry => {{
                if (entry.isIntersecting) {{
                    const idx = Array.from(slides).indexOf(entry.target);
                    document.getElementById('current').textContent = idx + 1;
                    currentSlide = idx;
                }}
            }});
        }}, {{ threshold: 0.5 }});

        slides.forEach(slide => observer.observe(slide));
    </script>
</body>
</html>"#,
        title = html_escape(title),
        slides = slides_html,
        slide_count = slides.len() + 1  // +1 for title slide
    )
}

/// Simple markdown to HTML conversion for slide content
fn markdown_to_html(md: &str) -> String {
    let mut html = String::new();
    let mut in_list = false;
    let mut in_code_block = false;

    for line in md.lines() {
        let trimmed = line.trim();

        // Code blocks
        if trimmed.starts_with("```") {
            if in_code_block {
                html.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                html.push_str("<pre><code>");
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            html.push_str(&html_escape(line));
            html.push('\n');
            continue;
        }

        // Lists
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            if !in_list {
                html.push_str("<ul>\n");
                in_list = true;
            }
            let item = trimmed.trim_start_matches(|c| c == '-' || c == '*' || c == ' ');
            html.push_str(&format!("<li>{}</li>\n", html_escape(item)));
            continue;
        } else if in_list {
            html.push_str("</ul>\n");
            in_list = false;
        }

        // Headings
        if trimmed.starts_with("### ") {
            html.push_str(&format!("<h3>{}</h3>\n", html_escape(&trimmed[4..])));
        } else if trimmed.starts_with("## ") {
            html.push_str(&format!("<h3>{}</h3>\n", html_escape(&trimmed[3..])));
        } else if trimmed.starts_with("# ") {
            html.push_str(&format!("<h3>{}</h3>\n", html_escape(&trimmed[2..])));
        } else if !trimmed.is_empty() {
            // Regular paragraph
            html.push_str(&format!("<p>{}</p>\n", html_escape(trimmed)));
        }
    }

    if in_list {
        html.push_str("</ul>\n");
    }
    if in_code_block {
        html.push_str("</code></pre>\n");
    }

    html
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
