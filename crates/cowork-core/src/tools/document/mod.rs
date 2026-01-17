//! Document processing tools

mod read_pdf;
mod read_office;

pub use read_pdf::ReadPdf;
pub use read_office::ReadOfficeDoc;

/// Supported document formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    Pdf,
    Word,
    Excel,
    PowerPoint,
    Text,
    Markdown,
    Html,
    Unknown,
}

impl DocumentFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "pdf" => Self::Pdf,
            "doc" | "docx" => Self::Word,
            "xls" | "xlsx" => Self::Excel,
            "ppt" | "pptx" => Self::PowerPoint,
            "txt" => Self::Text,
            "md" => Self::Markdown,
            "html" | "htm" => Self::Html,
            _ => Self::Unknown,
        }
    }
}
