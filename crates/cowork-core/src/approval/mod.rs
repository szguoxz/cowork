//! Approval system for human-in-the-loop control
//!
//! The approval system allows users to control which operations
//! are automatically approved vs require explicit confirmation.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

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

impl std::fmt::Display for ApprovalLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalLevel::None => write!(f, "none"),
            ApprovalLevel::Low => write!(f, "low"),
            ApprovalLevel::Medium => write!(f, "medium"),
            ApprovalLevel::High => write!(f, "high"),
            ApprovalLevel::Critical => write!(f, "critical"),
        }
    }
}

impl FromStr for ApprovalLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(ApprovalLevel::None),
            "low" => Ok(ApprovalLevel::Low),
            "medium" => Ok(ApprovalLevel::Medium),
            "high" => Ok(ApprovalLevel::High),
            "critical" => Ok(ApprovalLevel::Critical),
            _ => Err(format!("Unknown approval level: {}. Valid values: none, low, medium, high, critical", s)),
        }
    }
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

// ============================================================================
// Tool Approval Configuration
// ============================================================================

/// Configuration for auto-approval of tools
///
/// This is the canonical source for determining which tools need user approval.
/// Both CLI and UI should use this configuration.
#[derive(Debug, Clone)]
pub struct ToolApprovalConfig {
    /// Tools that are automatically approved (read-only, safe)
    auto_approve: std::collections::HashSet<String>,
    /// Tools that always require approval (destructive)
    always_require_approval: std::collections::HashSet<String>,
    /// Current approval level threshold
    level: ApprovalLevel,
    /// Session-approved tools (approved for the current session)
    session_approved: std::collections::HashSet<String>,
    /// If true, auto-approve everything for the session
    session_approve_all: bool,
}

impl Default for ToolApprovalConfig {
    fn default() -> Self {
        Self::new(ApprovalLevel::Low)
    }
}

impl ToolApprovalConfig {
    /// Create a new tool approval config with the given threshold level
    pub fn new(level: ApprovalLevel) -> Self {
        let mut auto_approve = std::collections::HashSet::new();

        // Read-only file operations (PascalCase tool names)
        auto_approve.insert("Read".to_string());
        auto_approve.insert("Glob".to_string());
        auto_approve.insert("Grep".to_string());

        // Web operations (read-only)
        auto_approve.insert("WebFetch".to_string());
        auto_approve.insert("WebSearch".to_string());

        // Task/planning tools
        auto_approve.insert("TodoWrite".to_string());
        auto_approve.insert("TaskOutput".to_string());

        // LSP operations (read-only)
        auto_approve.insert("LSP".to_string());

        // User interaction tools
        auto_approve.insert("AskUserQuestion".to_string());

        // Destructive tools that always require approval
        let mut always_require = std::collections::HashSet::new();
        always_require.insert("Write".to_string());
        always_require.insert("Edit".to_string());
        always_require.insert("Bash".to_string());

        Self {
            auto_approve,
            always_require_approval: always_require,
            level,
            session_approved: std::collections::HashSet::new(),
            session_approve_all: false,
        }
    }

    /// Create with no auto-approval (require approval for everything)
    pub fn strict() -> Self {
        Self {
            auto_approve: std::collections::HashSet::new(),
            always_require_approval: std::collections::HashSet::new(),
            level: ApprovalLevel::None,
            session_approved: std::collections::HashSet::new(),
            session_approve_all: false,
        }
    }

    /// Create with auto-approval for everything (dangerous - for testing)
    pub fn trust_all() -> Self {
        let mut config = Self::new(ApprovalLevel::Critical);
        config.session_approve_all = true;
        config
    }

    /// Check if a tool should be auto-approved
    pub fn should_auto_approve(&self, tool_name: &str) -> bool {
        // Session-wide approval overrides everything
        if self.session_approve_all {
            return true;
        }

        // Check session-approved tools
        if self.session_approved.contains(tool_name) {
            return true;
        }

        // Check based on approval level
        // Higher levels are MORE restrictive (require approval for more tools)
        match self.level {
            ApprovalLevel::None => {
                // None: auto-approve everything in auto_approve list
                self.auto_approve.contains(tool_name)
            }
            ApprovalLevel::Low => {
                // Low: auto-approve read-only tools in auto_approve list
                self.auto_approve.contains(tool_name)
            }
            ApprovalLevel::Medium => {
                // Medium: only auto-approve if in auto_approve AND not in always_require
                self.auto_approve.contains(tool_name)
                    && !self.always_require_approval.contains(tool_name)
            }
            ApprovalLevel::High => {
                // High: only auto-approve read-only tools (those in auto_approve but not destructive)
                self.auto_approve.contains(tool_name)
                    && !self.always_require_approval.contains(tool_name)
                    && !tool_name.contains("write")
                    && !tool_name.contains("execute")
                    && !tool_name.contains("delete")
            }
            ApprovalLevel::Critical => {
                // Critical: require approval for everything
                false
            }
        }
    }

    /// Check if a tool needs user approval
    pub fn needs_approval(&self, tool_name: &str) -> bool {
        !self.should_auto_approve(tool_name)
    }

    /// Approve a tool for the current session
    pub fn approve_for_session(&mut self, tool_name: impl Into<String>) {
        self.session_approved.insert(tool_name.into());
    }

    /// Approve all tools for the current session
    pub fn approve_all_for_session(&mut self) {
        self.session_approve_all = true;
    }

    /// Clear session approvals
    pub fn clear_session(&mut self) {
        self.session_approved.clear();
        self.session_approve_all = false;
    }

    /// Categorize tool calls into auto-approve and needs-approval lists
    pub fn categorize<'a>(&self, tool_names: impl Iterator<Item = &'a str>) -> (Vec<&'a str>, Vec<&'a str>) {
        let mut auto_approved = Vec::new();
        let mut needs_approval = Vec::new();

        for name in tool_names {
            if self.should_auto_approve(name) {
                auto_approved.push(name);
            } else {
                needs_approval.push(name);
            }
        }

        (auto_approved, needs_approval)
    }

    /// Get the current approval level
    pub fn level(&self) -> ApprovalLevel {
        self.level
    }

    /// Set the approval level
    pub fn set_level(&mut self, level: ApprovalLevel) {
        self.level = level;
    }
}
