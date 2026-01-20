//! Approval system for human-in-the-loop control
//!
//! The approval system allows users to control which operations
//! are automatically approved vs require explicit confirmation.

use serde::{Deserialize, Serialize};

/// Level of approval required for an operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum ApprovalLevel {
    /// No approval needed (read-only operations)
    #[default]
    None,
    /// Low risk (creating files, minor changes)
    Low,
    /// Medium risk (shell commands, external requests)
    Medium,
    /// High risk (deleting files, system changes)
    High,
    /// Critical operations (always require approval)
    Critical,
}

/// A request for user approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub description: String,
    pub level: ApprovalLevel,
    pub details: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ApprovalRequest {
    pub fn new(description: impl Into<String>, level: ApprovalLevel) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            level,
            details: None,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Response to an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalResponse {
    Approved,
    Denied { reason: Option<String> },
    ApprovedOnce,
    ApprovedForSession,
}

/// Policy for handling approval requests
pub trait ApprovalPolicy: Send + Sync {
    /// Check if an operation at this level requires approval
    fn requires_approval(&self, level: &ApprovalLevel) -> bool;

    /// Get the minimum level that requires approval
    fn approval_threshold(&self) -> ApprovalLevel;
}

/// Default approval policy - approve Low and below
#[derive(Debug, Clone, Default)]
pub struct DefaultApprovalPolicy {
    threshold: ApprovalLevel,
}

impl DefaultApprovalPolicy {
    pub fn new() -> Self {
        Self {
            threshold: ApprovalLevel::Medium,
        }
    }

    pub fn with_threshold(threshold: ApprovalLevel) -> Self {
        Self { threshold }
    }

    /// Trust all operations (dangerous - use for testing only)
    pub fn trust_all() -> Self {
        Self {
            threshold: ApprovalLevel::Critical,
        }
    }

    /// Require approval for everything except reads
    pub fn paranoid() -> Self {
        Self {
            threshold: ApprovalLevel::Low,
        }
    }
}

impl ApprovalPolicy for DefaultApprovalPolicy {
    fn requires_approval(&self, level: &ApprovalLevel) -> bool {
        level >= &self.threshold
    }

    fn approval_threshold(&self) -> ApprovalLevel {
        self.threshold
    }
}

/// Approval handler that prompts the user
pub struct ApprovalHandler {
    #[allow(dead_code)]
    policy: Box<dyn ApprovalPolicy>,
    session_approvals: std::collections::HashSet<String>,
}

impl ApprovalHandler {
    pub fn new(policy: impl ApprovalPolicy + 'static) -> Self {
        Self {
            policy: Box::new(policy),
            session_approvals: std::collections::HashSet::new(),
        }
    }

    /// Check if an operation is pre-approved
    pub fn is_pre_approved(&self, operation_id: &str) -> bool {
        self.session_approvals.contains(operation_id)
    }

    /// Record a session-wide approval
    pub fn approve_for_session(&mut self, operation_id: impl Into<String>) {
        self.session_approvals.insert(operation_id.into());
    }

    /// Clear all session approvals
    pub fn clear_session_approvals(&mut self) {
        self.session_approvals.clear();
    }
}
