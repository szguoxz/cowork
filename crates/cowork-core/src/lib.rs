//! Cowork Core - Multi-agent orchestration for desktop automation
//!
//! This crate provides the core functionality for the Cowork application:
//! - Agent definitions and implementations
//! - Tool system for file, shell, browser, and document operations
//! - Task planning and execution
//! - Human-in-the-loop approval system
//! - Context management

pub mod agent;
pub mod approval;
pub mod context;
pub mod error;
pub mod provider;
pub mod task;
pub mod tools;

pub use agent::{Agent, AgentRegistry};
pub use approval::{ApprovalLevel, ApprovalPolicy, ApprovalRequest};
pub use context::{Context, Workspace};
pub use error::{Error, Result};
pub use task::{Task, TaskExecutor, TaskPlanner, TaskStatus, TaskStep};
pub use tools::{Tool, ToolOutput, ToolRegistry};
