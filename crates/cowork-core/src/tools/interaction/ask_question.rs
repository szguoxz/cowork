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
pub struct Question {
    /// The full question text
    pub question: String,
    /// Short header/tag for the question (max 12 chars)
    pub header: String,
    /// Available options (2-4 choices)
    pub options: Vec<QuestionOption>,
    /// Allow multiple selections
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
        "ask_user_question"
    }

    fn description(&self) -> &str {
        "Ask the user questions during execution to gather preferences, clarify requirements, \
         or get decisions on implementation choices. Users can select from predefined options \
         or provide custom text input."
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
        if let Some(answers) = params.get("answers") {
            if answers.is_object() && !answers.as_object().unwrap().is_empty() {
                return Ok(ToolOutput::success(json!({
                    "answered": true,
                    "answers": answers
                })));
            }
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
            .map(|m| serde_json::from_value::<QuestionMetadata>(m.clone()).ok())
            .flatten();

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
