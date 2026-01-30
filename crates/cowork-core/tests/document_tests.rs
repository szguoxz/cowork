//! Document reading tests
//!
//! Tests for reading PDF and Office documents through the Read tool.
//! Note: Full integration tests require actual PDF/Office files.

use cowork_core::tools::filesystem::ReadFile;
use cowork_core::tools::{Tool, ToolExecutionContext};
use serde_json::json;
use tempfile::TempDir;

fn test_ctx() -> ToolExecutionContext {
    ToolExecutionContext::standalone("test", "test")
}

/// Create a temporary test directory
fn setup_test_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

mod read_pdf_tests {
    use super::*;

    #[tokio::test]
    async fn test_pdf_file_not_found() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "file_path": "nonexistent.pdf"
            }), test_ctx())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pdf_invalid_content() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        // Create a file with .pdf extension but invalid content
        std::fs::write(dir.path().join("fake.pdf"), "not a real pdf").unwrap();

        let result = tool
            .execute(json!({
                "file_path": "fake.pdf"
            }), test_ctx())
            .await;

        // Should fail because the content is not a valid PDF
        assert!(result.is_err());
    }
}

mod read_office_tests {
    use super::*;

    #[tokio::test]
    async fn test_docx_file_not_found() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "file_path": "nonexistent.docx"
            }), test_ctx())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_xlsx_file_not_found() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "file_path": "nonexistent.xlsx"
            }), test_ctx())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pptx_file_not_found() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let result = tool
            .execute(json!({
                "file_path": "nonexistent.pptx"
            }), test_ctx())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_regular_text_file_still_works() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        std::fs::write(dir.path().join("test.txt"), "Hello world\nLine 2").unwrap();

        let result = tool
            .execute(json!({
                "file_path": "test.txt"
            }), test_ctx())
            .await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
        let content = output.content["content"].as_str().unwrap();
        assert!(content.contains("Hello world"));
        assert!(content.contains("Line 2"));
    }

    #[tokio::test]
    async fn test_document_extension_dispatches_to_extractor() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        // Create a file with .docx extension but invalid content
        std::fs::write(dir.path().join("fake.docx"), "not a real docx").unwrap();

        let result = tool
            .execute(json!({
                "file_path": "fake.docx"
            }), test_ctx())
            .await;

        // Should fail with a document extraction error (not a text file error)
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // The error should be about parsing the DOCX, not about invalid UTF-8
        assert!(
            err.contains("DOCX") || err.contains("Failed"),
            "Unexpected error: {}",
            err
        );
    }
}
