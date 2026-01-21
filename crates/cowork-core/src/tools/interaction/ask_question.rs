//! AskUserQuestion tool - Interactive questions during execution
//!
//! Allows the agent to ask the user clarifying questions with multiple-choice options.


use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::approval::ApprovalLevel;
use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolOutput};

/// A single question option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// Display text for the option
    pub label: String,
    /// Description of what this option means
    pub description: String,
}

/// A question to ask the user
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// The full question text
    pub question: String,
    /// Short header/tag for the question (max 12 chars)
    #[serde(default)]
    pub header: String,
    /// Available options (2-4 choices)
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    /// Allow multiple selections
    #[serde(default)]
    pub multi_select: bool,
}

/// Request to ask the user a question
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    pub id: String,
    pub questions: Vec<Question>,
    pub metadata: Option<QuestionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionMetadata {
    pub source: Option<String>,
}

/// Response from user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionResponse {
    pub id: String,
    /// Map of question index to selected option(s)
    pub answers: std::collections::HashMap<String, Vec<String>>,
}

/// Handler for user questions
pub struct QuestionHandler {
    pending_questions: Arc<RwLock<std::collections::HashMap<String, oneshot::Sender<QuestionResponse>>>>,
    question_tx: mpsc::Sender<QuestionRequest>,
}

impl QuestionHandler {
    pub fn new(question_tx: mpsc::Sender<QuestionRequest>) -> Self {
        Self {
            pending_questions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            question_tx,
        }
    }

    pub async fn ask(&self, request: QuestionRequest) -> Result<QuestionResponse, String> {
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_questions.write().await;
            pending.insert(request.id.clone(), tx);
        }

        self.question_tx
            .send(request.clone())
            .await
            .map_err(|e| format!("Failed to send question: {}", e))?;

        rx.await.map_err(|e| format!("Failed to receive answer: {}", e))
    }

    pub async fn answer(&self, response: QuestionResponse) -> Result<(), String> {
        let mut pending = self.pending_questions.write().await;
        if let Some(tx) = pending.remove(&response.id) {
            tx.send(response)
                .map_err(|_| "Failed to send response".to_string())
        } else {
            Err(format!("No pending question with id {}", response.id))
        }
    }
}

/// Tool for asking user questions
pub struct AskUserQuestion {
    handler: Option<Arc<QuestionHandler>>,
}

impl AskUserQuestion {
    pub fn new() -> Self {
        Self { handler: None }
    }

    pub fn with_handler(handler: Arc<QuestionHandler>) -> Self {
        Self {
            handler: Some(handler),
        }
    }
}

impl Default for AskUserQuestion {
    fn default() -> Self {
        Self::new()
    }
}


impl Tool for AskUserQuestion {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        "Use this tool when you need to ask the user questions during execution. This allows you to:\n\
         1. Gather user preferences or requirements\n\
         2. Clarify ambiguous instructions\n\
         3. Get decisions on implementation choices as you work\n\
         4. Offer choices to the user about what direction to take\n\n\
         Usage notes:\n\
         - Users will always be able to select \"Other\" to provide custom text input\n\
         - Use multiSelect: true to allow multiple answers to be selected\n\
         - If you recommend a specific option, make that the first option and add \"(Recommended)\" at the end"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Questions to ask the user (1-4 questions)",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The complete question to ask"
                            },
                            "header": {
                                "type": "string",
                                "description": "Short label (max 12 chars) displayed as a chip/tag",
                                "maxLength": 12
                            },
                            "options": {
                                "type": "array",
                                "description": "Available choices (2-4 options)",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "Display text for this option (1-5 words)"
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Explanation of what this option means"
                                        }
                                    },
                                    "required": ["label", "description"]
                                }
                            },
                            "multiSelect": {
                                "type": "boolean",
                                "description": "Allow multiple options to be selected",
                                "default": false
                            }
                        },
                        "required": ["question", "header", "options", "multiSelect"]
                    }
                },
                "answers": {
                    "type": "object",
                    "description": "User answers collected by the UI",
                    "additionalProperties": {
                        "type": "string"
                    }
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata for tracking",
                    "properties": {
                        "source": {
                            "type": "string",
                            "description": "Source identifier for this question"
                        }
                    }
                }
            },
            "required": ["questions"]
        })
    }

    fn execute(&self, params: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
        // Check if answers are already provided (from UI callback)
        if let Some(answers) = params.get("answers")
            && answers.is_object() && !answers.as_object().unwrap().is_empty() {
                return Ok(ToolOutput::success(json!({
                    "answered": true,
                    "answers": answers
                })));
            }

        // Parse questions
        let questions_value = params
            .get("questions")
            .ok_or_else(|| ToolError::InvalidParams("questions is required".into()))?;

        let questions: Vec<Question> = serde_json::from_value(questions_value.clone())
            .map_err(|e| ToolError::InvalidParams(format!("Invalid questions format: {}", e)))?;

        // Validate questions
        if questions.is_empty() || questions.len() > 4 {
            return Err(ToolError::InvalidParams(
                "Must have 1-4 questions".into(),
            ));
        }

        for (i, q) in questions.iter().enumerate() {
            if q.options.len() < 2 || q.options.len() > 4 {
                return Err(ToolError::InvalidParams(format!(
                    "Question {} must have 2-4 options",
                    i + 1
                )));
            }
            if q.header.len() > 12 {
                return Err(ToolError::InvalidParams(format!(
                    "Question {} header must be max 12 chars",
                    i + 1
                )));
            }
        }

        // Create question request
        let request_id = uuid::Uuid::new_v4().to_string();
        let metadata = params
            .get("metadata")
            .and_then(|m| serde_json::from_value::<QuestionMetadata>(m.clone()).ok());

        let request = QuestionRequest {
            id: request_id.clone(),
            questions: questions.clone(),
            metadata,
        };

        // If we have a handler, send the question and wait for response
        if let Some(handler) = &self.handler {
            match handler.ask(request).await {
                Ok(response) => Ok(ToolOutput::success(json!({
                    "answered": true,
                    "request_id": request_id,
                    "answers": response.answers
                }))),
                Err(e) => Err(ToolError::ExecutionFailed(e)),
            }
        } else {
            // No handler - return pending status for UI to handle
            Ok(ToolOutput::success(json!({
                "pending": true,
                "request_id": request_id,
                "questions": questions,
                "message": "Waiting for user response"
            })))
        }
            })
    }

    fn approval_level(&self) -> ApprovalLevel {
        ApprovalLevel::None
    }
}

// ============================================================================
// Shared Question Parsing Utilities
// ============================================================================
// These functions provide standardized parsing and formatting for questions,
// eliminating duplicate code across CLI and UI modules.

/// Parse questions from a JSON Value (tool arguments)
///
/// This parses the "questions" array from the tool call arguments and returns
/// validated Question structs that both CLI and UI can use.
///
/// # Errors
/// Returns an error if:
/// - The "questions" field is missing
/// - The questions array is empty or has more than 4 questions
/// - Any question has fewer than 2 or more than 4 options
/// - Any question header exceeds 12 characters
pub fn parse_questions(args: &Value) -> Result<Vec<Question>, String> {
    let questions_value = args
        .get("questions")
        .ok_or_else(|| "Missing questions field".to_string())?;

    // Try to parse as strongly-typed Vec<Question>
    let questions: Vec<Question> = serde_json::from_value(questions_value.clone())
        .map_err(|e| format!("Invalid questions format: {}", e))?;

    // Validate
    validate_questions(&questions)?;

    Ok(questions)
}

/// Parse questions from a JSON Value using a lenient approach
///
/// This is more forgiving about malformed data and will extract what it can.
/// Useful for cases where the LLM might not follow the schema exactly.
pub fn parse_questions_lenient(args: &Value) -> Result<Vec<Question>, String> {
    let questions_value = args
        .get("questions")
        .and_then(|q| q.as_array())
        .ok_or_else(|| "Missing or invalid questions array".to_string())?;

    let mut questions = Vec::new();

    for q in questions_value {
        let question = q
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("Question")
            .to_string();

        let header = q
            .get("header")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let multi_select = q
            .get("multiSelect")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let options = q
            .get("options")
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|opt| QuestionOption {
                        label: opt
                            .get("label")
                            .and_then(|l| l.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: opt
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        questions.push(Question {
            question,
            header,
            options,
            multi_select,
        });
    }

    if questions.is_empty() {
        return Err("No valid questions found".to_string());
    }

    Ok(questions)
}

/// Validate a list of questions
pub fn validate_questions(questions: &[Question]) -> Result<(), String> {
    if questions.is_empty() {
        return Err("Must have at least 1 question".to_string());
    }
    if questions.len() > 4 {
        return Err("Must have at most 4 questions".to_string());
    }

    for (i, q) in questions.iter().enumerate() {
        if q.options.len() < 2 {
            return Err(format!("Question {} must have at least 2 options", i + 1));
        }
        if q.options.len() > 4 {
            return Err(format!("Question {} must have at most 4 options", i + 1));
        }
        if q.header.len() > 12 {
            return Err(format!(
                "Question {} header must be at most 12 characters",
                i + 1
            ));
        }
    }

    Ok(())
}

/// Format user answers into a JSON response
///
/// Takes a map of question index to answer string and formats it for the tool response.
pub fn format_answer_response(
    answers: std::collections::HashMap<String, String>,
) -> Value {
    json!({
        "answered": true,
        "answers": answers
    })
}

/// Format user answers with request ID
pub fn format_answer_response_with_id(
    request_id: &str,
    answers: std::collections::HashMap<String, String>,
) -> Value {
    json!({
        "answered": true,
        "request_id": request_id,
        "answers": answers
    })
}
