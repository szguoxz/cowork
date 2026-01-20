//! Document tool tests
//!
//! Tests for ReadPdf and ReadOfficeDoc tools.
//! Note: Full integration tests require actual PDF/Office files.

use cowork_core::tools::document::{ReadOfficeDoc, ReadPdf};
use cowork_core::tools::Tool;
use serde_json::json;
use tempfile::TempDir;

/// Create a temporary test directory
fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

mod read_pdf_tests {
    use super::*;

    #[tokio::test]
    async fn test_pdf_tool_exists() {
        let dir = setup_test_dir();
        let tool = ReadPdf::new(dir.path().to_path_buf());

        // Verify tool name and description
        assert_eq!(tool.name(), "read_pdf");
        assert!(tool.description().contains("PDF"));
    }

    #[tokio::test]
    async fn test_pdf_missing_path_param() {
        let dir = setup_test_dir();
        let tool = ReadPdf::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "pages": "all"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("path is required"));
    }

    #[tokio::test]
    async fn test_pdf_file_not_found() {
        let dir = setup_test_dir();
        let tool = ReadPdf::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "path": "nonexistent.pdf"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_pdf_wrong_extension() {
        let dir = setup_test_dir();
        let tool = ReadPdf::new(dir.path().to_path_buf());

        // Create a .txt file
        std::fs::write(dir.path().join("test.txt"), "Hello world").unwrap();

        let result = tool
            .execute(json!({
                "path": "test.txt"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected PDF"));
    }

    #[tokio::test]
    async fn test_pdf_parameters_schema() {
        let dir = setup_test_dir();
        let tool = ReadPdf::new(dir.path().to_path_buf());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("pages").is_some());
    }
}

mod read_office_tests {
    use super::*;

    #[tokio::test]
    async fn test_office_tool_exists() {
        let dir = setup_test_dir();
        let tool = ReadOfficeDoc::new(dir.path().to_path_buf());

        // Verify tool name and description
        assert_eq!(tool.name(), "read_office_doc");
        assert!(tool.description().contains("Office"));
    }

    #[tokio::test]
    async fn test_office_missing_path_param() {
        let dir = setup_test_dir();
        let tool = ReadOfficeDoc::new(dir.path().to_path_buf());

        let result = tool.execute(json!({})).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("path is required"));
    }

    #[tokio::test]
    async fn test_office_file_not_found() {
        let dir = setup_test_dir();
        let tool = ReadOfficeDoc::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "path": "nonexistent.docx"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_office_unsupported_format() {
        let dir = setup_test_dir();
        let tool = ReadOfficeDoc::new(dir.path().to_path_buf());

        // Create a .txt file (unsupported)
        std::fs::write(dir.path().join("test.txt"), "Hello world").unwrap();

        let result = tool
            .execute(json!({
                "path": "test.txt"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unsupported format"));
    }

    #[tokio::test]
    async fn test_office_parameters_schema() {
        let dir = setup_test_dir();
        let tool = ReadOfficeDoc::new(dir.path().to_path_buf());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("path").is_some());
        assert!(schema["properties"].get("extract_images").is_some());
    }
}

mod document_format_tests {
    use cowork_core::tools::document::DocumentFormat;

    #[test]
    fn test_pdf_format() {
        assert_eq!(DocumentFormat::from_extension("pdf"), DocumentFormat::Pdf);
        assert_eq!(DocumentFormat::from_extension("PDF"), DocumentFormat::Pdf);
    }

    #[test]
    fn test_word_format() {
        assert_eq!(DocumentFormat::from_extension("doc"), DocumentFormat::Word);
        assert_eq!(DocumentFormat::from_extension("docx"), DocumentFormat::Word);
        assert_eq!(DocumentFormat::from_extension("DOC"), DocumentFormat::Word);
        assert_eq!(DocumentFormat::from_extension("DOCX"), DocumentFormat::Word);
    }

    #[test]
    fn test_excel_format() {
        assert_eq!(DocumentFormat::from_extension("xls"), DocumentFormat::Excel);
        assert_eq!(
            DocumentFormat::from_extension("xlsx"),
            DocumentFormat::Excel
        );
        assert_eq!(DocumentFormat::from_extension("XLS"), DocumentFormat::Excel);
        assert_eq!(
            DocumentFormat::from_extension("XLSX"),
            DocumentFormat::Excel
        );
    }

    #[test]
    fn test_powerpoint_format() {
        assert_eq!(
            DocumentFormat::from_extension("ppt"),
            DocumentFormat::PowerPoint
        );
        assert_eq!(
            DocumentFormat::from_extension("pptx"),
            DocumentFormat::PowerPoint
        );
        assert_eq!(
            DocumentFormat::from_extension("PPT"),
            DocumentFormat::PowerPoint
        );
        assert_eq!(
            DocumentFormat::from_extension("PPTX"),
            DocumentFormat::PowerPoint
        );
    }

    #[test]
    fn test_text_format() {
        assert_eq!(DocumentFormat::from_extension("txt"), DocumentFormat::Text);
        assert_eq!(
            DocumentFormat::from_extension("md"),
            DocumentFormat::Markdown
        );
        assert_eq!(DocumentFormat::from_extension("html"), DocumentFormat::Html);
        assert_eq!(DocumentFormat::from_extension("htm"), DocumentFormat::Html);
    }

    #[test]
    fn test_unknown_format() {
        assert_eq!(
            DocumentFormat::from_extension("xyz"),
            DocumentFormat::Unknown
        );
        assert_eq!(
            DocumentFormat::from_extension(""),
            DocumentFormat::Unknown
        );
    }
}
