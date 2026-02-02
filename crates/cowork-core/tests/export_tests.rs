//! Tests for document export functionality

use cowork_core::tools::filesystem::ExportDocument;
use cowork_core::tools::{Tool, ToolExecutionContext};
use serde_json::json;
use tempfile::tempdir;

/// Create a mock execution context for testing
fn mock_context() -> ToolExecutionContext {
    ToolExecutionContext::test_auto_approve("test-call-id", "ExportDocument")
}

#[tokio::test]
async fn test_export_docx() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("test.docx");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "title": "Test Document",
        "content": "# Heading 1\n\nThis is a paragraph.\n\n## Heading 2\n\n- Item 1\n- Item 2"
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_ok(), "DOCX export failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.success);
    assert!(output_path.exists(), "DOCX file was not created");

    // Check file has content
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "DOCX file is empty");
}

#[tokio::test]
async fn test_export_xlsx_tsv() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("test.xlsx");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "title": "Test Sheet",
        "content": "Name\tAge\tCity\nAlice\t30\tNYC\nBob\t25\tLA"
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_ok(), "XLSX export failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.success);
    assert!(output_path.exists(), "XLSX file was not created");

    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0, "XLSX file is empty");
}

#[tokio::test]
async fn test_export_xlsx_json() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("test_json.xlsx");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "title": "Test Sheet",
        "content": r#"[{"Name": "Alice", "Age": 30}, {"Name": "Bob", "Age": 25}]"#
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_ok(), "XLSX JSON export failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.success);
    assert!(output_path.exists(), "XLSX file was not created");
}

#[tokio::test]
async fn test_export_html_slides_json() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("presentation.slides.html");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "title": "Test Presentation",
        "content": r#"[{"title": "Introduction", "content": "Welcome!"}, {"title": "Conclusion", "content": "Thank you!"}]"#
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_ok(), "HTML slides export failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.success);
    assert!(output_path.exists(), "HTML file was not created");

    // Check content
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("Test Presentation"), "Title not found");
    assert!(content.contains("Introduction"), "Slide title not found");
    assert!(content.contains("Welcome!"), "Slide content not found");
}

#[tokio::test]
async fn test_export_html_slides_markdown() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("presentation2.slides.html");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "title": "Markdown Presentation",
        "content": "# Slide One\nFirst slide content\n---\n# Slide Two\nSecond slide content"
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_ok(), "HTML slides markdown export failed: {:?}", result.err());

    let output = result.unwrap();
    assert!(output.success);
    assert!(output_path.exists(), "HTML file was not created");

    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("Slide One"), "First slide title not found");
    assert!(content.contains("Slide Two"), "Second slide title not found");
}

#[tokio::test]
async fn test_export_unsupported_format() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("test.txt");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "content": "Test content"
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_err(), "Should fail for unsupported format");
}

#[tokio::test]
async fn test_export_pdf() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace.clone());

    let output_path = workspace.join("test.pdf");
    let params = json!({
        "file_path": output_path.to_str().unwrap(),
        "title": "Test PDF",
        "content": "# Heading\n\nThis is a test PDF document.\n\n## Section\n\n- Point 1\n- Point 2"
    });

    let result = tool.execute(params, mock_context()).await;

    // PDF export may fail if no fonts are available - that's OK in CI
    if result.is_ok() {
        let output = result.unwrap();
        assert!(output.success);
        assert!(output_path.exists(), "PDF file was not created");

        let metadata = std::fs::metadata(&output_path).unwrap();
        assert!(metadata.len() > 0, "PDF file is empty");
    } else {
        // If it fails due to font issues, that's acceptable
        let err = result.err().unwrap();
        let err_str = format!("{:?}", err);
        assert!(
            err_str.contains("font") || err_str.contains("Font"),
            "PDF export failed for unexpected reason: {}",
            err_str
        );
    }
}

#[tokio::test]
async fn test_export_outside_workspace() {
    let temp_dir = tempdir().unwrap();
    let workspace = temp_dir.path().to_path_buf();
    let tool = ExportDocument::new(workspace);

    let params = json!({
        "file_path": "/tmp/outside.docx",
        "content": "Test content"
    });

    let result = tool.execute(params, mock_context()).await;
    assert!(result.is_err(), "Should fail for path outside workspace");
}
