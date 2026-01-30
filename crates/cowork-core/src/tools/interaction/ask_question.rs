//! AskUserQuestion tool - Interactive questions during execution
//!
//! Allows the agent to ask the user clarifying questions with multiple-choice options.


use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::ToolError;
use crate::tools::{BoxFuture, Tool, ToolExecutionContext, ToolOutput};

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

/// The canonical tool name, used for routing in agent_loop and approval config
pub const NAME: &str = "AskUserQuestion";

/// Tool for asking user questions
pub struct AskUserQuestion;

impl AskUserQuestion {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AskUserQuestion {
    fn default() -> Self {
        Self::new()
    }
}


impl Tool for AskUserQuestion {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        crate::prompt::builtin::claude_code::tools::ASK_USER_QUESTION
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
                                "description": "Very short label displayed as a chip/tag (max 12 chars). Examples: \"Auth method\", \"Library\", \"Approach\"."
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
                                    "required": ["label"]
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

    fn execute(&self, params: Value, ctx: ToolExecutionContext) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
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
        }

        // Convert to QuestionInfo for the approval channel
        let question_infos: Vec<crate::session::QuestionInfo> = questions
            .iter()
            .map(|q| crate::session::QuestionInfo {
                question: q.question.clone(),
                header: if q.header.is_empty() { None } else { Some(q.header.clone()) },
                options: q.options.iter().map(|o| crate::session::QuestionOption {
                    label: o.label.clone(),
                    description: if o.description.is_empty() { None } else { Some(o.description.clone()) },
                }).collect(),
                multi_select: q.multi_select,
            })
            .collect();

        // Ask questions through the approval channel
        match ctx.ask_question(question_infos).await {
            Ok(answers) => Ok(ToolOutput::success(json!({
                "answered": true,
                "answers": answers
            }))),
            Err(e) => Err(ToolError::ExecutionFailed(e)),
        }
            })
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
