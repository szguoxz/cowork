//! Approval channel for tool execution
//!
//! This module provides a channel-based mechanism for tools to request user approval
//! during execution. The approval channel serializes all approval requests, ensuring
//! only one approval modal is shown at a time.
//!
//! ## Design
//!
//! - Tools send approval requests through a shared channel
//! - A mutex ensures only one request is in-flight at a time
//! - The agent loop handles the single active request
//! - Auto-approve logic is centralized in the handler
//! - Subagents share the same approval channel as their parent
//!
//! ## Serialization
//!
//! The `ApprovalGate` mutex ensures that even with concurrent tool execution,
//! only one tool can be waiting for approval at a time. Other tools block at
//! the gate until the current approval is resolved.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

use super::types::QuestionInfo;

/// Request sent through the approval channel
#[derive(Debug)]
pub enum ApprovalRequest {
    /// Request approval for a tool execution
    ToolApproval {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        description: Option<String>,
        response_tx: oneshot::Sender<ApprovalResponse>,
    },
    /// Request answer to a question
    Question {
        request_id: String,
        questions: Vec<QuestionInfo>,
        response_tx: oneshot::Sender<QuestionResponse>,
    },
}

/// Response to a tool approval request
#[derive(Debug, Clone)]
pub enum ApprovalResponse {
    Approved,
    Rejected { reason: Option<String> },
}

/// Response to a question
#[derive(Debug, Clone)]
pub struct QuestionResponse {
    pub answers: HashMap<String, String>,
}

/// Sender half of the approval channel
pub type ApprovalSender = mpsc::UnboundedSender<ApprovalRequest>;

/// Receiver half of the approval channel
pub type ApprovalReceiver = mpsc::UnboundedReceiver<ApprovalRequest>;

/// Gate to serialize approval requests
///
/// Only one tool can hold this gate at a time. This ensures that even with
/// concurrent tool execution, approval requests are serialized and the frontend
/// only sees one request at a time.
pub type ApprovalGate = Arc<Mutex<()>>;

/// Create a new approval channel and gate
pub fn approval_channel() -> (ApprovalSender, ApprovalReceiver, ApprovalGate) {
    let (tx, rx) = mpsc::unbounded_channel();
    let gate = Arc::new(Mutex::new(()));
    (tx, rx, gate)
}

/// Context passed to tools during execution
///
/// This provides tools with the ability to request approval and access
/// to their execution context (like the tool call ID).
#[derive(Clone)]
pub struct ToolExecutionContext {
    /// Channel to request approval
    approval_tx: ApprovalSender,
    /// Gate to serialize approval requests
    approval_gate: ApprovalGate,
    /// Tool call ID (for this execution)
    pub tool_call_id: String,
    /// Tool name
    pub tool_name: String,
}

impl ToolExecutionContext {
    /// Create a new tool execution context
    pub fn new(
        approval_tx: ApprovalSender,
        approval_gate: ApprovalGate,
        tool_call_id: String,
        tool_name: String,
    ) -> Self {
        Self {
            approval_tx,
            approval_gate,
            tool_call_id,
            tool_name,
        }
    }

    /// Create a standalone context for tools that don't need approval routing
    ///
    /// This creates a context with a dummy channel. Any approval requests will
    /// fail with "Session cancelled". Use this for standalone tool execution
    /// outside of an agent loop (e.g., CLI commands).
    pub fn standalone(tool_call_id: impl Into<String>, tool_name: impl Into<String>) -> Self {
        let (tx, _rx, gate) = approval_channel();
        Self {
            approval_tx: tx,
            approval_gate: gate,
            tool_call_id: tool_call_id.into(),
            tool_name: tool_name.into(),
        }
    }

    /// Create a test context that auto-approves all requests
    ///
    /// This spawns a background task to automatically approve any approval
    /// or question requests. Useful for testing tools that require approval.
    pub fn test_auto_approve(tool_call_id: impl Into<String>, tool_name: impl Into<String>) -> Self {
        let (tx, mut rx, gate) = approval_channel();

        // Spawn a task to auto-approve all requests
        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                match request {
                    ApprovalRequest::ToolApproval { response_tx, .. } => {
                        let _ = response_tx.send(ApprovalResponse::Approved);
                    }
                    ApprovalRequest::Question { response_tx, .. } => {
                        let _ = response_tx.send(QuestionResponse {
                            answers: std::collections::HashMap::new(),
                        });
                    }
                }
            }
        });

        Self {
            approval_tx: tx,
            approval_gate: gate,
            tool_call_id: tool_call_id.into(),
            tool_name: tool_name.into(),
        }
    }

    /// Request approval for a tool execution
    ///
    /// Returns Ok(()) if approved, Err with reason if rejected.
    ///
    /// This method acquires the approval gate to ensure only one approval request
    /// is in-flight at a time. Other tools calling this method will block until
    /// the current approval is resolved.
    pub async fn request_approval(
        &self,
        arguments: serde_json::Value,
        description: Option<String>,
    ) -> Result<(), String> {
        // Acquire the gate - only one approval request at a time
        let _guard = self.approval_gate.lock().await;

        let (response_tx, response_rx) = oneshot::channel();

        let request = ApprovalRequest::ToolApproval {
            tool_call_id: self.tool_call_id.clone(),
            tool_name: self.tool_name.clone(),
            arguments,
            description,
            response_tx,
        };

        // Send request (fails if channel is closed, e.g., session cancelled)
        self.approval_tx
            .send(request)
            .map_err(|_| "Session cancelled".to_string())?;

        // Wait for response (gate is held until we get a response)
        match response_rx.await {
            Ok(ApprovalResponse::Approved) => Ok(()),
            Ok(ApprovalResponse::Rejected { reason }) => {
                Err(reason.unwrap_or_else(|| "Rejected by user".to_string()))
            }
            Err(_) => Err("Session cancelled".to_string()),
        }
        // Gate is released here when _guard is dropped
    }

    /// Ask the user a question
    ///
    /// Returns the user's answers as a map of question header/id to answer.
    ///
    /// This method acquires the approval gate to ensure only one question
    /// is in-flight at a time.
    pub async fn ask_question(
        &self,
        questions: Vec<QuestionInfo>,
    ) -> Result<HashMap<String, String>, String> {
        // Acquire the gate - only one question at a time
        let _guard = self.approval_gate.lock().await;

        let (response_tx, response_rx) = oneshot::channel();

        let request = ApprovalRequest::Question {
            request_id: self.tool_call_id.clone(),
            questions,
            response_tx,
        };

        self.approval_tx
            .send(request)
            .map_err(|_| "Session cancelled".to_string())?;

        match response_rx.await {
            Ok(response) => Ok(response.answers),
            Err(_) => Err("Session cancelled".to_string()),
        }
        // Gate is released here when _guard is dropped
    }

    /// Get the approval sender and gate for passing to subagents
    pub fn approval_channel(&self) -> (ApprovalSender, ApprovalGate) {
        (self.approval_tx.clone(), self.approval_gate.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_approval_channel_creation() {
        let (tx, mut rx, _gate) = approval_channel();

        // Send a test request
        let (response_tx, _response_rx) = oneshot::channel();
        tx.send(ApprovalRequest::ToolApproval {
            tool_call_id: "test-123".to_string(),
            tool_name: "Bash".to_string(),
            arguments: serde_json::json!({"command": "ls"}),
            description: None,
            response_tx,
        }).unwrap();

        // Receive it
        let request = rx.recv().await.unwrap();
        match request {
            ApprovalRequest::ToolApproval { tool_call_id, tool_name, .. } => {
                assert_eq!(tool_call_id, "test-123");
                assert_eq!(tool_name, "Bash");
            }
            _ => panic!("Expected ToolApproval"),
        }
    }

    #[tokio::test]
    async fn test_context_request_approval() {
        let (tx, mut rx, gate) = approval_channel();
        let ctx = ToolExecutionContext::new(tx, gate, "call-456".to_string(), "Write".to_string());

        // Spawn a task to approve the request
        tokio::spawn(async move {
            if let Some(ApprovalRequest::ToolApproval { response_tx, .. }) = rx.recv().await {
                response_tx.send(ApprovalResponse::Approved).unwrap();
            }
        });

        // Request approval
        let result = ctx.request_approval(
            serde_json::json!({"file_path": "/test.txt"}),
            Some("Write to test.txt".to_string()),
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_context_request_rejected() {
        let (tx, mut rx, gate) = approval_channel();
        let ctx = ToolExecutionContext::new(tx, gate, "call-789".to_string(), "Bash".to_string());

        // Spawn a task to reject the request
        tokio::spawn(async move {
            if let Some(ApprovalRequest::ToolApproval { response_tx, .. }) = rx.recv().await {
                response_tx.send(ApprovalResponse::Rejected {
                    reason: Some("Too dangerous".to_string()),
                }).unwrap();
            }
        });

        // Request approval
        let result = ctx.request_approval(
            serde_json::json!({"command": "rm -rf /"}),
            None,
        ).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Too dangerous");
    }

    #[tokio::test]
    async fn test_channel_closed_on_cancel() {
        let (tx, rx, gate) = approval_channel();
        let ctx = ToolExecutionContext::new(tx, gate, "call-999".to_string(), "Test".to_string());

        // Drop the receiver to simulate session cancellation
        drop(rx);

        // Request should fail
        let result = ctx.request_approval(serde_json::json!({}), None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Session cancelled");
    }
}
