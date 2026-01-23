//! Session integration tests
//!
//! Tests for the session management system including:
//! - SessionManager creation and lifecycle
//! - Session input/output handling
//! - Multi-session management
//! - Session configuration

use cowork_core::approval::ToolApprovalConfig;
use cowork_core::provider::ProviderType;
use cowork_core::session::{SessionConfig, SessionInput, SessionManager, SessionOutput};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

fn test_config() -> SessionConfig {
    SessionConfig {
        workspace_path: std::env::current_dir().unwrap(),
        approval_config: ToolApprovalConfig::trust_all(),
        system_prompt: Some("You are a test assistant.".to_string()),
        provider_type: ProviderType::Anthropic,
        model: None,
        api_key: None,
        web_search_config: None,
        prompt_config: Default::default(),
        component_registry: None,
        tool_scope: None,
        enable_hooks: None,
        save_session: true,
    }
}

mod session_manager_tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_new() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        assert_eq!(manager.session_count(), 0);
    }

    #[tokio::test]
    async fn test_manager_list_sessions_empty() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        let sessions = manager.list_sessions();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_has_session_false() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        assert!(!manager.has_session("nonexistent"));
    }

    #[tokio::test]
    async fn test_stop_nonexistent_session() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        // Should not error when stopping a session that doesn't exist
        let result = manager.stop_session("nonexistent");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_all_empty() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        let result = manager.stop_all();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_sender_clone() {
        let (manager, _output_rx) = SessionManager::new(test_config());
        let sender = manager.output_sender();

        // Can use the sender to inject outputs
        let test_output = SessionOutput::ready();
        let _ = sender
            .send(("test-session".to_string(), test_output))
            .await;
    }
}

mod session_input_tests {
    use super::*;

    #[test]
    fn test_user_message_creation() {
        let input = SessionInput::user_message("Hello, world!");
        match input {
            SessionInput::UserMessage { content } => {
                assert_eq!(content, "Hello, world!");
            }
            _ => panic!("Expected UserMessage"),
        }
    }

    #[test]
    fn test_approve_tool_creation() {
        let input = SessionInput::approve_tool("tool-123");
        match input {
            SessionInput::ApproveTool { tool_call_id } => {
                assert_eq!(tool_call_id, "tool-123");
            }
            _ => panic!("Expected ApproveTool"),
        }
    }

    #[test]
    fn test_reject_tool_creation() {
        let input = SessionInput::reject_tool("tool-456", Some("Not allowed".to_string()));
        match input {
            SessionInput::RejectTool {
                tool_call_id,
                reason,
            } => {
                assert_eq!(tool_call_id, "tool-456");
                assert_eq!(reason, Some("Not allowed".to_string()));
            }
            _ => panic!("Expected RejectTool"),
        }
    }

    #[test]
    fn test_reject_tool_no_reason() {
        let input = SessionInput::reject_tool("tool-789", None);
        match input {
            SessionInput::RejectTool {
                tool_call_id,
                reason,
            } => {
                assert_eq!(tool_call_id, "tool-789");
                assert!(reason.is_none());
            }
            _ => panic!("Expected RejectTool"),
        }
    }

    #[test]
    fn test_answer_question_creation() {
        let mut answers = HashMap::new();
        answers.insert("q1".to_string(), "answer1".to_string());
        answers.insert("q2".to_string(), "answer2".to_string());

        let input = SessionInput::answer_question("req-123", answers.clone());
        match input {
            SessionInput::AnswerQuestion {
                request_id,
                answers: a,
            } => {
                assert_eq!(request_id, "req-123");
                assert_eq!(a.len(), 2);
                assert_eq!(a.get("q1"), Some(&"answer1".to_string()));
            }
            _ => panic!("Expected AnswerQuestion"),
        }
    }

    #[test]
    fn test_input_serialization_roundtrip() {
        let inputs = vec![
            SessionInput::user_message("test message"),
            SessionInput::approve_tool("tool-1"),
            SessionInput::reject_tool("tool-2", Some("reason".to_string())),
        ];

        for input in inputs {
            let json = serde_json::to_string(&input).expect("Serialization failed");
            let deserialized: SessionInput =
                serde_json::from_str(&json).expect("Deserialization failed");

            // Compare by re-serializing (since we can't directly compare enums)
            let json2 = serde_json::to_string(&deserialized).expect("Re-serialization failed");
            assert_eq!(json, json2);
        }
    }
}

mod session_output_tests {
    use super::*;

    #[test]
    fn test_ready_creation() {
        let output = SessionOutput::ready();
        assert!(matches!(output, SessionOutput::Ready));
    }

    #[test]
    fn test_idle_creation() {
        let output = SessionOutput::idle();
        assert!(matches!(output, SessionOutput::Idle));
    }

    #[test]
    fn test_user_message_echo() {
        let output = SessionOutput::user_message("msg-1", "Hello");
        match output {
            SessionOutput::UserMessage { id, content } => {
                assert_eq!(id, "msg-1");
                assert_eq!(content, "Hello");
            }
            _ => panic!("Expected UserMessage"),
        }
    }

    #[test]
    fn test_thinking_creation() {
        let output = SessionOutput::thinking("Processing...");
        match output {
            SessionOutput::Thinking { content } => {
                assert_eq!(content, "Processing...");
            }
            _ => panic!("Expected Thinking"),
        }
    }

    #[test]
    fn test_assistant_message_creation() {
        let output = SessionOutput::assistant_message("msg-2", "Here's my response");
        match output {
            SessionOutput::AssistantMessage { id, content } => {
                assert_eq!(id, "msg-2");
                assert_eq!(content, "Here's my response");
            }
            _ => panic!("Expected AssistantMessage"),
        }
    }

    #[test]
    fn test_tool_start_creation() {
        let args = serde_json::json!({"path": "test.txt"});
        let output = SessionOutput::tool_start("t1", "read_file", args.clone());
        match output {
            SessionOutput::ToolStart {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "read_file");
                assert_eq!(arguments, args);
            }
            _ => panic!("Expected ToolStart"),
        }
    }

    #[test]
    fn test_tool_pending_creation() {
        let args = serde_json::json!({"command": "ls"});
        let output =
            SessionOutput::tool_pending("t2", "execute_command", args.clone(), Some("List files".to_string()));
        match output {
            SessionOutput::ToolPending {
                id,
                name,
                arguments,
                description,
            } => {
                assert_eq!(id, "t2");
                assert_eq!(name, "execute_command");
                assert_eq!(arguments, args);
                assert_eq!(description, Some("List files".to_string()));
            }
            _ => panic!("Expected ToolPending"),
        }
    }

    #[test]
    fn test_tool_done_success() {
        let output = SessionOutput::tool_done("t3", "read_file", true, "file contents");
        match output {
            SessionOutput::ToolDone {
                id,
                name,
                success,
                output: out,
            } => {
                assert_eq!(id, "t3");
                assert_eq!(name, "read_file");
                assert!(success);
                assert_eq!(out, "file contents");
            }
            _ => panic!("Expected ToolDone"),
        }
    }

    #[test]
    fn test_tool_done_failure() {
        let output = SessionOutput::tool_done("t4", "write_file", false, "Permission denied");
        match output {
            SessionOutput::ToolDone {
                id,
                name,
                success,
                output: out,
            } => {
                assert_eq!(id, "t4");
                assert_eq!(name, "write_file");
                assert!(!success);
                assert_eq!(out, "Permission denied");
            }
            _ => panic!("Expected ToolDone"),
        }
    }

    #[test]
    fn test_error_creation() {
        let output = SessionOutput::error("Something went wrong");
        match output {
            SessionOutput::Error { message } => {
                assert_eq!(message, "Something went wrong");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_question_output_creation() {
        use cowork_core::session::{QuestionInfo, QuestionOption};

        let options = vec![
            QuestionOption {
                label: "Option A".to_string(),
                description: Some("First option".to_string()),
            },
            QuestionOption {
                label: "Option B".to_string(),
                description: None,
            },
        ];

        let question = QuestionInfo {
            question: "Which option do you prefer?".to_string(),
            header: Some("Preference".to_string()),
            options,
            multi_select: false,
        };

        let output = SessionOutput::Question {
            request_id: "q-123".to_string(),
            questions: vec![question],
        };

        match output {
            SessionOutput::Question { request_id, questions } => {
                assert_eq!(request_id, "q-123");
                assert_eq!(questions.len(), 1);
                assert_eq!(questions[0].question, "Which option do you prefer?");
                assert_eq!(questions[0].header, Some("Preference".to_string()));
                assert!(!questions[0].multi_select);
                assert_eq!(questions[0].options.len(), 2);
                assert_eq!(questions[0].options[0].label, "Option A");
                assert_eq!(questions[0].options[0].description, Some("First option".to_string()));
                assert_eq!(questions[0].options[1].label, "Option B");
                assert!(questions[0].options[1].description.is_none());
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_question_output_serialization() {
        use cowork_core::session::{QuestionInfo, QuestionOption};

        let question = QuestionInfo {
            question: "Test question?".to_string(),
            header: Some("Test".to_string()),
            options: vec![
                QuestionOption {
                    label: "Yes".to_string(),
                    description: Some("Confirm".to_string()),
                },
                QuestionOption {
                    label: "No".to_string(),
                    description: None,
                },
            ],
            multi_select: true,
        };

        let output = SessionOutput::Question {
            request_id: "req-1".to_string(),
            questions: vec![question],
        };

        let json = serde_json::to_string(&output).expect("Serialization failed");
        assert!(json.contains("question"));
        assert!(json.contains("req-1"));
        assert!(json.contains("Test question?"));

        let deserialized: SessionOutput = serde_json::from_str(&json).expect("Deserialization failed");
        match deserialized {
            SessionOutput::Question { request_id, questions } => {
                assert_eq!(request_id, "req-1");
                assert!(questions[0].multi_select);
            }
            _ => panic!("Deserialization failed"),
        }
    }

    #[test]
    fn test_output_serialization_roundtrip() {
        let outputs = vec![
            SessionOutput::ready(),
            SessionOutput::idle(),
            SessionOutput::user_message("m1", "test"),
            SessionOutput::thinking("thinking..."),
            SessionOutput::assistant_message("m2", "response"),
            SessionOutput::tool_start("t1", "tool", serde_json::json!({})),
            SessionOutput::tool_done("t1", "tool", true, "output"),
            SessionOutput::error("error message"),
        ];

        for output in outputs {
            let json = serde_json::to_string(&output).expect("Serialization failed");
            let deserialized: SessionOutput =
                serde_json::from_str(&json).expect("Deserialization failed");

            // Compare by re-serializing
            let json2 = serde_json::to_string(&deserialized).expect("Re-serialization failed");
            assert_eq!(json, json2);
        }
    }
}

mod session_config_tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SessionConfig::default();
        assert!(config.system_prompt.is_none());
        assert!(config.model.is_none());
        assert!(config.api_key.is_none());
        assert_eq!(config.provider_type, ProviderType::Anthropic);
    }

    #[test]
    fn test_config_new() {
        let config = SessionConfig::new("/tmp/workspace");
        assert_eq!(
            config.workspace_path,
            std::path::PathBuf::from("/tmp/workspace")
        );
    }

    #[test]
    fn test_config_builder_chain() {
        let config = SessionConfig::new("/workspace")
            .with_provider(ProviderType::OpenAI)
            .with_model("gpt-4")
            .with_api_key("sk-test-key")
            .with_system_prompt("Custom system prompt")
            .with_approval_config(ToolApprovalConfig::trust_all());

        assert_eq!(config.provider_type, ProviderType::OpenAI);
        assert_eq!(config.model, Some("gpt-4".to_string()));
        assert_eq!(config.api_key, Some("sk-test-key".to_string()));
        assert_eq!(
            config.system_prompt,
            Some("Custom system prompt".to_string())
        );
    }

    #[test]
    fn test_config_clone() {
        let config1 = SessionConfig::new("/workspace")
            .with_provider(ProviderType::DeepSeek)
            .with_model("deepseek-chat");

        let config2 = config1.clone();

        assert_eq!(config1.workspace_path, config2.workspace_path);
        assert_eq!(config1.provider_type, config2.provider_type);
        assert_eq!(config1.model, config2.model);
    }
}

mod question_types_tests {
    use cowork_core::session::{QuestionInfo, QuestionOption};

    #[test]
    fn test_question_option_creation() {
        let option = QuestionOption {
            label: "Test Label".to_string(),
            description: Some("Test Description".to_string()),
        };
        assert_eq!(option.label, "Test Label");
        assert_eq!(option.description, Some("Test Description".to_string()));
    }

    #[test]
    fn test_question_option_without_description() {
        let option = QuestionOption {
            label: "Simple Option".to_string(),
            description: None,
        };
        assert_eq!(option.label, "Simple Option");
        assert!(option.description.is_none());
    }

    #[test]
    fn test_question_info_single_select() {
        let options = vec![
            QuestionOption {
                label: "A".to_string(),
                description: None,
            },
            QuestionOption {
                label: "B".to_string(),
                description: None,
            },
        ];

        let question = QuestionInfo {
            question: "Choose one".to_string(),
            header: Some("Choice".to_string()),
            options,
            multi_select: false,
        };

        assert_eq!(question.question, "Choose one");
        assert_eq!(question.header, Some("Choice".to_string()));
        assert!(!question.multi_select);
        assert_eq!(question.options.len(), 2);
    }

    #[test]
    fn test_question_info_multi_select() {
        let options = vec![
            QuestionOption {
                label: "Option 1".to_string(),
                description: Some("First".to_string()),
            },
            QuestionOption {
                label: "Option 2".to_string(),
                description: Some("Second".to_string()),
            },
            QuestionOption {
                label: "Option 3".to_string(),
                description: Some("Third".to_string()),
            },
        ];

        let question = QuestionInfo {
            question: "Select multiple".to_string(),
            header: None,
            options,
            multi_select: true,
        };

        assert_eq!(question.question, "Select multiple");
        assert!(question.header.is_none());
        assert!(question.multi_select);
        assert_eq!(question.options.len(), 3);
    }

    #[test]
    fn test_question_option_serialization() {
        let option = QuestionOption {
            label: "Test".to_string(),
            description: Some("Desc".to_string()),
        };

        let json = serde_json::to_string(&option).expect("Serialization failed");
        assert!(json.contains("Test"));
        assert!(json.contains("Desc"));

        let deserialized: QuestionOption = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized.label, "Test");
        assert_eq!(deserialized.description, Some("Desc".to_string()));
    }

    #[test]
    fn test_question_info_serialization() {
        let question = QuestionInfo {
            question: "What?".to_string(),
            header: Some("Header".to_string()),
            options: vec![QuestionOption {
                label: "Yes".to_string(),
                description: None,
            }],
            multi_select: false,
        };

        let json = serde_json::to_string(&question).expect("Serialization failed");
        assert!(json.contains("What?"));
        assert!(json.contains("Header"));
        assert!(json.contains("Yes"));

        let deserialized: QuestionInfo = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized.question, "What?");
        assert_eq!(deserialized.header, Some("Header".to_string()));
        assert!(!deserialized.multi_select);
    }

    #[test]
    fn test_question_option_clone() {
        let option = QuestionOption {
            label: "Clone Me".to_string(),
            description: Some("Description".to_string()),
        };

        let cloned = option.clone();
        assert_eq!(cloned.label, option.label);
        assert_eq!(cloned.description, option.description);
    }

    #[test]
    fn test_question_info_clone() {
        let question = QuestionInfo {
            question: "Clone me?".to_string(),
            header: None,
            options: vec![
                QuestionOption {
                    label: "A".to_string(),
                    description: None,
                },
            ],
            multi_select: true,
        };

        let cloned = question.clone();
        assert_eq!(cloned.question, question.question);
        assert_eq!(cloned.header, question.header);
        assert_eq!(cloned.multi_select, question.multi_select);
        assert_eq!(cloned.options.len(), question.options.len());
    }
}

mod tool_result_format_tests {
    use cowork_core::orchestration::ChatSession;
    use cowork_core::provider::{ContentBlock, MessageContent};

    /// Test that single tool result creates proper content block format
    #[test]
    fn test_single_tool_result_format() {
        let mut session = ChatSession::new();

        // Add a user message
        session.add_user_message("Read the file");

        // Add assistant message with tool call
        let tool_calls = vec![
            cowork_core::orchestration::ToolCallInfo::new(
                "call_123",
                "Read",
                serde_json::json!({"file_path": "/test.txt"})
            )
        ];
        session.add_assistant_message("I'll read that file", tool_calls);

        // Add tool result
        session.add_tool_result("call_123", "File contents here");

        // Convert to LLM messages
        let llm_messages = session.to_llm_messages();

        // Find the tool result message
        let tool_result_msg = llm_messages.iter()
            .find(|m| m.role == "user" && matches!(&m.content, MessageContent::Blocks(_)))
            .expect("Should have tool result message");

        // Verify it's a USER message with content blocks
        assert_eq!(tool_result_msg.role, "user");
        if let MessageContent::Blocks(blocks) = &tool_result_msg.content {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                    assert_eq!(tool_use_id, "call_123");
                    assert_eq!(content, "File contents here");
                    assert!(is_error.is_none()); // Not an error
                }
                _ => panic!("Expected ToolResult block"),
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    /// Test that tool error results include is_error flag
    #[test]
    fn test_tool_error_result_format() {
        let mut session = ChatSession::new();

        session.add_user_message("Delete the file");
        let tool_calls = vec![
            cowork_core::orchestration::ToolCallInfo::new(
                "call_456",
                "delete_file",
                serde_json::json!({"path": "/protected.txt"})
            )
        ];
        session.add_assistant_message("I'll delete that file", tool_calls);

        // Add tool result with error
        session.add_tool_result_with_error("call_456", "Permission denied", true);

        let llm_messages = session.to_llm_messages();

        let tool_result_msg = llm_messages.last().expect("Should have messages");
        if let MessageContent::Blocks(blocks) = &tool_result_msg.content {
            match &blocks[0] {
                ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                    assert_eq!(tool_use_id, "call_456");
                    assert_eq!(content, "Permission denied");
                    assert_eq!(*is_error, Some(true)); // Error flag set
                }
                _ => panic!("Expected ToolResult block"),
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    /// Test that multiple tool results are batched into single message
    #[test]
    fn test_batched_tool_results_format() {
        let mut session = ChatSession::new();

        session.add_user_message("Read two files");
        let tool_calls = vec![
            cowork_core::orchestration::ToolCallInfo::new(
                "call_1",
                "Read",
                serde_json::json!({"file_path": "/file1.txt"})
            ),
            cowork_core::orchestration::ToolCallInfo::new(
                "call_2",
                "Read",
                serde_json::json!({"file_path": "/file2.txt"})
            ),
        ];
        session.add_assistant_message("I'll read both files", tool_calls);

        // Add batched tool results
        session.add_tool_results(vec![
            ("call_1".to_string(), "Contents of file 1".to_string(), false),
            ("call_2".to_string(), "Contents of file 2".to_string(), false),
        ]);

        let llm_messages = session.to_llm_messages();

        // Should be 3 messages: user, assistant (with tool calls), user (tool results)
        assert_eq!(llm_messages.len(), 3);

        let tool_result_msg = &llm_messages[2];
        assert_eq!(tool_result_msg.role, "user");

        if let MessageContent::Blocks(blocks) = &tool_result_msg.content {
            // Should have 2 tool results in a single message
            assert_eq!(blocks.len(), 2);

            // Verify first result
            match &blocks[0] {
                ContentBlock::ToolResult { tool_use_id, content, .. } => {
                    assert_eq!(tool_use_id, "call_1");
                    assert_eq!(content, "Contents of file 1");
                }
                _ => panic!("Expected ToolResult block"),
            }

            // Verify second result
            match &blocks[1] {
                ContentBlock::ToolResult { tool_use_id, content, .. } => {
                    assert_eq!(tool_use_id, "call_2");
                    assert_eq!(content, "Contents of file 2");
                }
                _ => panic!("Expected ToolResult block"),
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    /// Test that batched results can include both success and error
    #[test]
    fn test_batched_mixed_results() {
        let mut session = ChatSession::new();

        session.add_user_message("Try two operations");
        let tool_calls = vec![
            cowork_core::orchestration::ToolCallInfo::new(
                "op_1",
                "Read",
                serde_json::json!({"file_path": "/exists.txt"})
            ),
            cowork_core::orchestration::ToolCallInfo::new(
                "op_2",
                "Read",
                serde_json::json!({"file_path": "/missing.txt"})
            ),
        ];
        session.add_assistant_message("Trying both", tool_calls);

        // Add batched results with mixed success/failure
        session.add_tool_results(vec![
            ("op_1".to_string(), "File contents".to_string(), false), // Success
            ("op_2".to_string(), "File not found".to_string(), true), // Error
        ]);

        let llm_messages = session.to_llm_messages();
        let tool_result_msg = llm_messages.last().expect("Should have messages");

        if let MessageContent::Blocks(blocks) = &tool_result_msg.content {
            assert_eq!(blocks.len(), 2);

            // First should be success (no is_error)
            match &blocks[0] {
                ContentBlock::ToolResult { is_error, .. } => {
                    assert!(is_error.is_none() || *is_error == Some(false));
                }
                _ => panic!("Expected ToolResult"),
            }

            // Second should be error
            match &blocks[1] {
                ContentBlock::ToolResult { is_error, .. } => {
                    assert_eq!(*is_error, Some(true));
                }
                _ => panic!("Expected ToolResult"),
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    /// Test assistant message with tool calls creates proper content blocks
    #[test]
    fn test_assistant_tool_use_format() {
        let mut session = ChatSession::new();

        session.add_user_message("Search for foo");
        let tool_calls = vec![
            cowork_core::orchestration::ToolCallInfo::new(
                "grep_1",
                "Grep",
                serde_json::json!({"pattern": "foo", "path": "."})
            ),
        ];
        session.add_assistant_message("Let me search for that", tool_calls);

        let llm_messages = session.to_llm_messages();
        let assistant_msg = &llm_messages[1];

        assert_eq!(assistant_msg.role, "assistant");

        if let MessageContent::Blocks(blocks) = &assistant_msg.content {
            // Should have text + tool_use blocks
            assert!(blocks.len() >= 2);

            // First block should be text
            match &blocks[0] {
                ContentBlock::Text { text } => {
                    assert_eq!(text, "Let me search for that");
                }
                _ => panic!("Expected Text block first"),
            }

            // Second block should be tool_use
            match &blocks[1] {
                ContentBlock::ToolUse { id, name, input } => {
                    assert_eq!(id, "grep_1");
                    assert_eq!(name, "Grep");
                    assert_eq!(input["pattern"], "foo");
                }
                _ => panic!("Expected ToolUse block"),
            }
        } else {
            panic!("Expected Blocks content");
        }
    }
}

mod session_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_output_receiver_receives_injected_output() {
        let (manager, mut rx) = SessionManager::new(test_config());
        let sender = manager.output_sender();

        // Send a test output
        sender
            .send(("test".to_string(), SessionOutput::ready()))
            .await
            .unwrap();

        // Receive with timeout
        let result = timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok(), "Should receive within timeout");

        let (session_id, output) = result.unwrap().unwrap();
        assert_eq!(session_id, "test");
        assert!(matches!(output, SessionOutput::Ready));
    }

    #[tokio::test]
    async fn test_multiple_outputs_received_in_order() {
        let (manager, mut rx) = SessionManager::new(test_config());
        let sender = manager.output_sender();

        // Send multiple outputs
        let outputs = vec![
            ("s1".to_string(), SessionOutput::ready()),
            ("s1".to_string(), SessionOutput::idle()),
            ("s2".to_string(), SessionOutput::ready()),
        ];

        for output in &outputs {
            sender.send(output.clone()).await.unwrap();
        }

        // Receive all outputs
        for expected in &outputs {
            let result = timeout(Duration::from_millis(100), rx.recv()).await;
            assert!(result.is_ok());
            let (session_id, _) = result.unwrap().unwrap();
            assert_eq!(session_id, expected.0);
        }
    }

    #[tokio::test]
    async fn test_different_session_outputs() {
        let (manager, mut rx) = SessionManager::new(test_config());
        let sender = manager.output_sender();

        // Send outputs for different sessions
        sender
            .send(("session-a".to_string(), SessionOutput::user_message("m1", "Hello A")))
            .await
            .unwrap();

        sender
            .send(("session-b".to_string(), SessionOutput::user_message("m2", "Hello B")))
            .await
            .unwrap();

        // Receive first
        let (id1, out1) = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(id1, "session-a");
        if let SessionOutput::UserMessage { content, .. } = out1 {
            assert_eq!(content, "Hello A");
        } else {
            panic!("Expected UserMessage");
        }

        // Receive second
        let (id2, out2) = timeout(Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(id2, "session-b");
        if let SessionOutput::UserMessage { content, .. } = out2 {
            assert_eq!(content, "Hello B");
        } else {
            panic!("Expected UserMessage");
        }
    }
}
