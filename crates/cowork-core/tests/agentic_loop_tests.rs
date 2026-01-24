//! Agentic loop integration tests
//!
//! Tests for tool execution and registry.

use cowork_core::tools::{Tool, ToolRegistry, ToolDefinition, ToolOutput};
use cowork_core::tools::filesystem::{ReadFile, WriteFile, GlobFiles, GrepFiles};
use cowork_core::tools::shell::ExecuteCommand;
use serde_json::json;
use tempfile::TempDir;
use std::fs;
use std::sync::Arc;

/// Create a test workspace with sample files
fn setup_workspace() -> TempDir {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let base = dir.path();

    fs::create_dir_all(base.join("src")).unwrap();

    fs::write(
        base.join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
}
"#,
    ).unwrap();

    fs::write(
        base.join("README.md"),
        "# Test Project\n\nThis is a test project.\n",
    ).unwrap();

    dir
}

/// Create a tool registry for testing
fn create_tool_registry(workspace: &std::path::Path) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    registry.register(Arc::new(ReadFile::new(workspace.to_path_buf())));
    registry.register(Arc::new(WriteFile::new(workspace.to_path_buf())));
    registry.register(Arc::new(GlobFiles::new(workspace.to_path_buf())));
    registry.register(Arc::new(GrepFiles::new(workspace.to_path_buf())));
    registry.register(Arc::new(ExecuteCommand::new(workspace.to_path_buf())));

    registry
}

mod tool_registry_tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        assert!(registry.get("Read").is_some());
        assert!(registry.get("Write").is_some());
        assert!(registry.get("Glob").is_some());
        assert!(registry.get("Grep").is_some());
        assert!(registry.get("Bash").is_some());
    }

    #[test]
    fn test_list_tools() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tools = registry.list();
        assert!(tools.len() >= 5, "Should have at least 5 tools");

        for tool_def in tools {
            assert!(!tool_def.name.is_empty());
            assert!(!tool_def.description.is_empty());
        }
    }

    #[test]
    fn test_get_nonexistent_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_all_tools() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let all_tools = registry.all();
        assert!(all_tools.len() >= 5);
    }

    #[tokio::test]
    async fn test_execute_tool_directly() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("Read").unwrap();
        let result = tool.execute(json!({
            "file_path": "README.md"
        })).await;

        assert!(result.is_ok(), "Read failed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.success);
    }
}

mod tool_output_tests {
    use super::*;

    #[test]
    fn test_success_output() {
        let output = ToolOutput::success(json!({
            "message": "Operation completed"
        }));

        assert!(output.success);
        assert!(output.error.is_none());
    }

    #[test]
    fn test_error_output() {
        let output = ToolOutput::error("Something went wrong");

        assert!(!output.success);
        assert!(output.error.is_some());
        assert!(output.error.unwrap().contains("wrong"));
    }

    #[test]
    fn test_output_with_metadata() {
        let output = ToolOutput::success(json!({"data": "test"}))
            .with_metadata("key", "value")
            .with_metadata("count", 42);

        assert!(output.success);
        assert!(output.metadata.contains_key("key"));
        assert!(output.metadata.contains_key("count"));
    }
}

mod tool_definition_tests {
    use super::*;

    #[test]
    fn test_tool_definition_structure() {
        let def = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string"
                    }
                }
            }),
        };

        assert_eq!(def.name, "test_tool");
        assert!(!def.description.is_empty());
        assert!(def.parameters.is_object());
    }
}

mod tool_execution_tests {
    use super::*;

    #[tokio::test]
    async fn test_glob_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("Glob").unwrap();
        let result = tool.execute(json!({
            "pattern": "**/*.rs"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_grep_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("Grep").unwrap();
        let result = tool.execute(json!({
            "pattern": "fn main"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_execute_command_tool() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("Bash").unwrap();
        let result = tool.execute(json!({
            "command": "echo 'test'"
        })).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_tool_error_handling() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let tool = registry.get("Read").unwrap();
        let result = tool.execute(json!({
            "file_path": "nonexistent_file.txt"
        })).await;

        // Should return an error
        assert!(result.is_err(), "Should fail for nonexistent file");
    }
}

mod approval_level_tests {
    use super::*;
    use cowork_core::approval::ApprovalLevel;

    #[test]
    fn test_read_file_approval() {
        let dir = setup_workspace();
        let tool = ReadFile::new(dir.path().to_path_buf());

        let level = tool.approval_level();
        assert!(
            matches!(level, ApprovalLevel::None | ApprovalLevel::Low),
            "Read should have low approval"
        );
    }

    #[test]
    fn test_write_file_approval() {
        let dir = setup_workspace();
        let tool = WriteFile::new(dir.path().to_path_buf());

        let level = tool.approval_level();
        // WriteFile has Low approval since it's less destructive than Edit or Delete
        assert!(
            matches!(level, ApprovalLevel::Low | ApprovalLevel::Medium | ApprovalLevel::High),
            "Write should require some approval"
        );
    }

    #[test]
    fn test_approval_level_from_str() {
        // Test standard level names
        assert_eq!("none".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::None);
        assert_eq!("low".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::Low);
        assert_eq!("medium".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::Medium);
        assert_eq!("high".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::High);
        assert_eq!("critical".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::Critical);

        // Test case insensitivity
        assert_eq!("NONE".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::None);
        assert_eq!("Low".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::Low);
        assert_eq!("MEDIUM".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::Medium);
        assert_eq!("High".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::High);
        assert_eq!("CRITICAL".parse::<ApprovalLevel>().unwrap(), ApprovalLevel::Critical);

        // Test unknown level
        assert!("unknown".parse::<ApprovalLevel>().is_err());
        assert!("".parse::<ApprovalLevel>().is_err());
    }

    #[test]
    fn test_approval_level_display_roundtrip() {
        let levels = [
            ApprovalLevel::None,
            ApprovalLevel::Low,
            ApprovalLevel::Medium,
            ApprovalLevel::High,
            ApprovalLevel::Critical,
        ];

        for level in levels {
            let s = level.to_string();
            let parsed: ApprovalLevel = s.parse().unwrap();
            assert_eq!(parsed, level, "Roundtrip failed for {:?}", level);
        }
    }
}

mod message_conversion_tests {
    use cowork_core::context::{Message, MessageRole, messages_from_ui};
    use chrono::Utc;

    #[test]
    fn test_message_role_parse() {
        assert_eq!(MessageRole::parse("user"), MessageRole::User);
        assert_eq!(MessageRole::parse("assistant"), MessageRole::Assistant);
        assert_eq!(MessageRole::parse("system"), MessageRole::System);
        assert_eq!(MessageRole::parse("tool"), MessageRole::Tool);
        // Unknown should default to Tool
        assert_eq!(MessageRole::parse("unknown"), MessageRole::Tool);
        assert_eq!(MessageRole::parse(""), MessageRole::Tool);
    }

    #[test]
    fn test_message_role_as_str() {
        assert_eq!(MessageRole::User.as_str(), "user");
        assert_eq!(MessageRole::Assistant.as_str(), "assistant");
        assert_eq!(MessageRole::System.as_str(), "system");
        assert_eq!(MessageRole::Tool.as_str(), "tool");
    }

    #[test]
    fn test_message_role_display() {
        assert_eq!(MessageRole::User.to_string(), "user");
        assert_eq!(MessageRole::Assistant.to_string(), "assistant");
        assert_eq!(MessageRole::System.to_string(), "system");
        assert_eq!(MessageRole::Tool.to_string(), "tool");
    }

    #[test]
    fn test_message_role_fromstr_trait() {
        let user: MessageRole = "user".parse().unwrap();
        assert_eq!(user, MessageRole::User);

        let assistant: MessageRole = "assistant".parse().unwrap();
        assert_eq!(assistant, MessageRole::Assistant);
    }

    #[test]
    fn test_message_new() {
        let msg = Message::new(MessageRole::User, "Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        // Timestamp should be recent
        assert!(msg.timestamp <= Utc::now());
    }

    #[test]
    fn test_message_with_timestamp() {
        let ts = Utc::now();
        let msg = Message::with_timestamp(MessageRole::Assistant, "Response", ts);
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "Response");
        assert_eq!(msg.timestamp, ts);
    }

    #[test]
    fn test_message_from_str_role() {
        let ts = Utc::now();
        let msg = Message::from_str_role("user", "Test message", ts);
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Test message");
        assert_eq!(msg.timestamp, ts);

        // Unknown role should default to Tool
        let msg2 = Message::from_str_role("unknown_role", "Unknown", ts);
        assert_eq!(msg2.role, MessageRole::Tool);
    }

    #[test]
    fn test_message_role_str() {
        let msg = Message::new(MessageRole::User, "Test");
        assert_eq!(msg.role_str(), "user");

        let msg2 = Message::new(MessageRole::Assistant, "Test");
        assert_eq!(msg2.role_str(), "assistant");
    }

    #[test]
    fn test_messages_from_ui() {
        // Simulate UI message structure
        struct UiMessage {
            role: String,
            content: String,
            timestamp: chrono::DateTime<Utc>,
        }

        let ts = Utc::now();
        let ui_messages = vec![
            UiMessage { role: "user".to_string(), content: "Hello".to_string(), timestamp: ts },
            UiMessage { role: "assistant".to_string(), content: "Hi!".to_string(), timestamp: ts },
            UiMessage { role: "system".to_string(), content: "Context".to_string(), timestamp: ts },
        ];

        let messages = messages_from_ui(&ui_messages, |m| {
            (m.role.as_str(), m.content.as_str(), m.timestamp)
        });

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert_eq!(messages[1].content, "Hi!");
        assert_eq!(messages[2].role, MessageRole::System);
        assert_eq!(messages[2].content, "Context");
    }

    #[test]
    fn test_message_role_roundtrip() {
        let roles = [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::System,
            MessageRole::Tool,
        ];

        for role in roles {
            let s = role.as_str();
            let parsed = MessageRole::parse(s);
            assert_eq!(parsed, role, "Roundtrip failed for {:?}", role);
        }
    }
}

mod parallel_tool_execution_tests {
    use super::*;

    /// Test that multiple tools can be executed in parallel and results batched
    #[tokio::test]
    async fn test_parallel_read_operations() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        // Execute multiple read operations in parallel
        let read_tool = registry.get("Read").unwrap();

        let (result1, result2) = tokio::join!(
            read_tool.execute(json!({"file_path": "README.md"})),
            read_tool.execute(json!({"file_path": "src/main.rs"}))
        );

        // Both should succeed
        assert!(result1.is_ok(), "First read failed: {:?}", result1.err());
        assert!(result2.is_ok(), "Second read failed: {:?}", result2.err());

        let output1 = result1.unwrap();
        let output2 = result2.unwrap();
        assert!(output1.success);
        assert!(output2.success);
    }

    /// Test that different tools can be executed in parallel
    #[tokio::test]
    async fn test_parallel_different_tools() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let read_tool = registry.get("Read").unwrap();
        let glob_tool = registry.get("Glob").unwrap();
        let grep_tool = registry.get("Grep").unwrap();

        // Execute Read, Glob, and Grep in parallel
        let (read_result, glob_result, grep_result) = tokio::join!(
            read_tool.execute(json!({"file_path": "README.md"})),
            glob_tool.execute(json!({"pattern": "**/*.rs"})),
            grep_tool.execute(json!({"pattern": "fn main"}))
        );

        // All should succeed
        assert!(read_result.is_ok());
        assert!(glob_result.is_ok());
        assert!(grep_result.is_ok());
    }

    /// Test that parallel execution handles mixed success/failure
    #[tokio::test]
    async fn test_parallel_mixed_results() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let read_tool = registry.get("Read").unwrap();

        // Execute one valid and one invalid read in parallel
        let (good_result, bad_result) = tokio::join!(
            read_tool.execute(json!({"file_path": "README.md"})),
            read_tool.execute(json!({"file_path": "nonexistent.txt"}))
        );

        // First should succeed
        assert!(good_result.is_ok());
        assert!(good_result.unwrap().success);

        // Second should fail
        assert!(bad_result.is_err());
    }

    /// Test that results can be collected for batching
    #[tokio::test]
    async fn test_collect_parallel_results_for_batching() {
        let dir = setup_workspace();
        let registry = create_tool_registry(dir.path());

        let read_tool = registry.get("Read").unwrap();

        // Simulate what execute_tools_batched does
        let tool_calls = vec![
            ("call_1", json!({"file_path": "README.md"})),
            ("call_2", json!({"file_path": "src/main.rs"})),
        ];

        let mut results: Vec<(String, String, bool)> = Vec::new();

        for (id, params) in tool_calls {
            let result = read_tool.execute(params).await;
            let (output, is_error) = match result {
                Ok(output) => (output.content.to_string(), !output.success),
                Err(e) => (format!("Error: {}", e), true),
            };
            results.push((id.to_string(), output, is_error));
        }

        // Should have 2 results
        assert_eq!(results.len(), 2);

        // Both should be successful
        assert!(!results[0].2, "First result should not be an error");
        assert!(!results[1].2, "Second result should not be an error");

        // IDs should be preserved
        assert_eq!(results[0].0, "call_1");
        assert_eq!(results[1].0, "call_2");
    }
}

mod question_parsing_tests {
    use cowork_core::tools::interaction::{
        parse_questions, parse_questions_lenient, validate_questions,
        format_answer_response, format_answer_response_with_id, Question, QuestionOption,
    };
    use serde_json::json;
    use std::collections::HashMap;

    fn make_valid_question() -> serde_json::Value {
        json!({
            "questions": [{
                "question": "What is your preferred language?",
                "header": "Language",
                "multiSelect": false,
                "options": [
                    { "label": "Rust", "description": "Systems language" },
                    { "label": "Python", "description": "Scripting language" }
                ]
            }]
        })
    }

    #[test]
    fn test_parse_questions_valid() {
        let args = make_valid_question();
        let questions = parse_questions(&args).unwrap();

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "What is your preferred language?");
        assert_eq!(questions[0].header, "Language");
        assert!(!questions[0].multi_select);
        assert_eq!(questions[0].options.len(), 2);
        assert_eq!(questions[0].options[0].label, "Rust");
        assert_eq!(questions[0].options[0].description, "Systems language");
    }

    #[test]
    fn test_parse_questions_missing_field() {
        let args = json!({});
        let result = parse_questions(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing questions field"));
    }

    #[test]
    fn test_parse_questions_empty() {
        let args = json!({ "questions": [] });
        let result = parse_questions(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 1 question"));
    }

    #[test]
    fn test_parse_questions_too_many() {
        let args = json!({
            "questions": [
                { "question": "Q1", "header": "H1", "multiSelect": false, "options": [
                    { "label": "A", "description": "D" },
                    { "label": "B", "description": "D" }
                ]},
                { "question": "Q2", "header": "H2", "multiSelect": false, "options": [
                    { "label": "A", "description": "D" },
                    { "label": "B", "description": "D" }
                ]},
                { "question": "Q3", "header": "H3", "multiSelect": false, "options": [
                    { "label": "A", "description": "D" },
                    { "label": "B", "description": "D" }
                ]},
                { "question": "Q4", "header": "H4", "multiSelect": false, "options": [
                    { "label": "A", "description": "D" },
                    { "label": "B", "description": "D" }
                ]},
                { "question": "Q5", "header": "H5", "multiSelect": false, "options": [
                    { "label": "A", "description": "D" },
                    { "label": "B", "description": "D" }
                ]}
            ]
        });
        let result = parse_questions(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at most 4 questions"));
    }

    #[test]
    fn test_parse_questions_too_few_options() {
        let args = json!({
            "questions": [{
                "question": "Q1",
                "header": "H1",
                "multiSelect": false,
                "options": [{ "label": "A", "description": "D" }]
            }]
        });
        let result = parse_questions(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 2 options"));
    }

    #[test]
    fn test_parse_questions_lenient() {
        // Missing some fields, but lenient parser should handle it
        let args = json!({
            "questions": [{
                "question": "What?",
                "options": [
                    { "label": "Yes" },
                    { "label": "No", "description": "Nope" }
                ]
            }]
        });

        let questions = parse_questions_lenient(&args).unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "What?");
        assert_eq!(questions[0].header, ""); // defaulted
        assert!(!questions[0].multi_select); // defaulted
        assert_eq!(questions[0].options[0].description, ""); // defaulted
        assert_eq!(questions[0].options[1].description, "Nope");
    }

    #[test]
    fn test_validate_questions() {
        let valid = vec![Question {
            question: "Test?".to_string(),
            header: "Test".to_string(),
            multi_select: false,
            options: vec![
                QuestionOption { label: "A".to_string(), description: "D".to_string() },
                QuestionOption { label: "B".to_string(), description: "D".to_string() },
            ],
        }];
        assert!(validate_questions(&valid).is_ok());

        // Empty list
        assert!(validate_questions(&[]).is_err());
    }

    #[test]
    fn test_format_answer_response() {
        let mut answers = HashMap::new();
        answers.insert("0".to_string(), "Rust".to_string());

        let response = format_answer_response(answers);
        assert_eq!(response["answered"], true);
        assert_eq!(response["answers"]["0"], "Rust");
    }

    #[test]
    fn test_format_answer_response_with_id() {
        let mut answers = HashMap::new();
        answers.insert("0".to_string(), "Python".to_string());

        let response = format_answer_response_with_id("req-123", answers);
        assert_eq!(response["answered"], true);
        assert_eq!(response["request_id"], "req-123");
        assert_eq!(response["answers"]["0"], "Python");
    }
}
