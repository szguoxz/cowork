//! Filesystem tool tests
//!
//! Tests for Read, Write, Edit, Glob, and Grep tools.

use cowork_core::tools::Tool;
use cowork_core::tools::filesystem::{ReadFile, WriteFile, EditFile, GlobFiles, GrepFiles};
use serde_json::json;
use tempfile::TempDir;
use std::fs;

/// Create a temporary test directory with sample files
fn setup_test_dir() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let base = dir.path();

    // Create directory structure
    fs::create_dir_all(base.join("src")).unwrap();
    fs::create_dir_all(base.join("tests")).unwrap();

    // Create sample files
    fs::write(
        base.join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
    let x = 42;
    do_something(x);
}

fn do_something(value: i32) {
    println!("Value is: {}", value);
}
"#,
    ).unwrap();

    fs::write(
        base.join("src/lib.rs"),
        r#"pub mod utils;
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#,
    ).unwrap();

    fs::write(base.join("README.md"), "# My Project\n\nThis is a test project.\n").unwrap();

    dir
}

mod read_file_tests {
    use super::*;

    #[tokio::test]
    async fn test_read_existing_file() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "path": "src/main.rs"
        })).await;

        assert!(result.is_ok(), "Failed to read file: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
        // Check that content contains expected text
        let content_str = output.content.to_string();
        assert!(content_str.contains("Hello, world!"));
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let dir = setup_test_dir();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "path": "nonexistent.txt"
        })).await;

        assert!(result.is_err(), "Should fail for nonexistent file");
    }
}

mod write_file_tests {
    use super::*;

    #[tokio::test]
    async fn test_write_new_file() {
        let dir = setup_test_dir();
        let tool = WriteFile::new(dir.path().to_path_buf());

        let content = "This is a new file.";
        let result = tool.execute(json!({
            "path": "new_file.txt",
            "content": content
        })).await;

        assert!(result.is_ok(), "Failed to write file: {:?}", result.err());

        // Verify file was written
        let written = fs::read_to_string(dir.path().join("new_file.txt")).unwrap();
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let dir = setup_test_dir();
        let tool = WriteFile::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "path": "deep/nested/dir/file.txt",
            "content": "nested content"
        })).await;

        assert!(result.is_ok(), "Should create parent directories");
        assert!(dir.path().join("deep/nested/dir/file.txt").exists());
    }
}

mod edit_file_tests {
    use super::*;

    #[tokio::test]
    async fn test_edit_replace_unique_string() {
        let dir = setup_test_dir();
        let tool = EditFile::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "path": "src/main.rs",
            "old_string": "Hello, world!",
            "new_string": "Hello, Rust!"
        })).await;

        assert!(result.is_ok(), "Edit failed: {:?}", result.err());

        let content = fs::read_to_string(dir.path().join("src/main.rs")).unwrap();
        assert!(content.contains("Hello, Rust!"));
        assert!(!content.contains("Hello, world!"));
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let dir = setup_test_dir();
        let tool = EditFile::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "path": "src/main.rs",
            "old_string": "println!",
            "new_string": "eprintln!",
            "replace_all": true
        })).await;

        assert!(result.is_ok(), "Replace all failed: {:?}", result.err());

        let content = fs::read_to_string(dir.path().join("src/main.rs")).unwrap();
        assert!(!content.contains("println!"));
        assert!(content.contains("eprintln!"));
    }
}

mod glob_tests {
    use super::*;

    #[tokio::test]
    async fn test_glob_rust_files() {
        let dir = setup_test_dir();
        let tool = GlobFiles::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "pattern": "**/*.rs"
        })).await;

        assert!(result.is_ok(), "Glob failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let dir = setup_test_dir();
        let tool = GlobFiles::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "pattern": "**/*.xyz"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }
}

mod grep_tests {
    use super::*;

    #[tokio::test]
    async fn test_grep_simple_pattern() {
        let dir = setup_test_dir();
        let tool = GrepFiles::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "pattern": "fn main"
        })).await;

        assert!(result.is_ok(), "Grep failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let dir = setup_test_dir();
        let tool = GrepFiles::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "pattern": "HELLO",
            "-i": true
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_grep_regex_pattern() {
        let dir = setup_test_dir();
        let tool = GrepFiles::new(dir.path().to_path_buf());

        let result = tool.execute(json!({
            "pattern": "fn\\s+\\w+"
        })).await;

        assert!(result.is_ok(), "Regex grep failed: {:?}", result.err());
    }
}
