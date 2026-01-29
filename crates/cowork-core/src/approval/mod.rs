//! Approval system for human-in-the-loop control
//!
//! The approval system allows users to control which operations
//! are automatically approved vs require explicit confirmation.

pub mod bash_safety;

use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::tools::interaction::ASK_QUESTION_TOOL_NAME;

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

        // Task/agent tools
        auto_approve.insert("TodoWrite".to_string());
        auto_approve.insert("TaskOutput".to_string());
        auto_approve.insert("Task".to_string());
        auto_approve.insert("KillShell".to_string());

        // LSP operations (read-only)
        auto_approve.insert("LSP".to_string());

        // Planning/interaction tools
        auto_approve.insert(ASK_QUESTION_TOOL_NAME.to_string());
        auto_approve.insert("ExitPlanMode".to_string());
        auto_approve.insert("Skill".to_string());

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

    /// Check if a tool should be auto-approved, considering its arguments.
    ///
    /// For Bash tools, this parses the command to determine if it's read-only (safe).
    /// For other tools, delegates to `should_auto_approve`.
    pub fn should_auto_approve_with_args(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        if tool_name == "Bash" {
            // Session-wide approval overrides everything
            if self.session_approve_all {
                return true;
            }
            if self.session_approved.contains(tool_name) {
                return true;
            }
            // Parse the "command" argument and check safety
            if let Some(command) = args.get("command").and_then(|v| v.as_str()) {
                return bash_safety::is_safe_command(command);
            }
            // No command argument — require approval
            return false;
        }
        self.should_auto_approve(tool_name)
    }

    /// Check if a tool should be auto-approved
    pub fn should_auto_approve(&self, tool_name: &str) -> bool {
        // Session-wide approval overrides everything
        if self.session_approve_all || self.session_approved.contains(tool_name) {
            return true;
        }

        // Critical: require approval for everything
        if self.level == ApprovalLevel::Critical {
            return false;
        }

        // Must be in auto_approve list
        if !self.auto_approve.contains(tool_name) {
            return false;
        }

        // Medium+: also check always_require_approval list
        if self.level >= ApprovalLevel::Medium && self.always_require_approval.contains(tool_name) {
            return false;
        }

        // High: additional checks for destructive-sounding tools
        if self.level >= ApprovalLevel::High {
            let name_lower = tool_name.to_lowercase();
            if name_lower.contains("write") || name_lower.contains("execute") || name_lower.contains("delete") {
                return false;
            }
        }

        true
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
