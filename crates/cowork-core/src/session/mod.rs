//! Session module - Unified agent loop architecture
//!
//! This module provides a multi-session architecture that can be used by both
//! CLI and UI frontends. Key components:
//!
//! - `SessionManager`: Manages multiple concurrent sessions
//! - `AgentLoop`: The core execution loop for each session
//! - `SessionInput`/`SessionOutput`: Message types for communication
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                 SessionManager                   │
//! │                                                  │
//! │  push_message(session_id, input) ────────────▶  │
//! │                                                  │
//! │  ┌──────────────────────────────────────────┐   │
//! │  │  HashMap<SessionId, Sender<SessionInput>>│   │
//! │  │                                          │   │
//! │  │  session_1 -> tx1 ──▶ [AgentLoop 1] ─┐  │   │
//! │  │  session_2 -> tx2 ──▶ [AgentLoop 2] ─┼──────▶ output_rx
//! │  │  session_3 -> tx3 ──▶ [AgentLoop 3] ─┘  │   │
//! │  └──────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! # Example Usage
//!
//! ```ignore
//! use cowork_core::session::{SessionManager, SessionConfig, SessionInput};
//!
//! // Create manager with config factory (returns manager and output receiver)
//! let (manager, mut output_rx) = SessionManager::new(|| SessionConfig::default());
//!
//! // Send a message (creates session if needed)
//! manager.push_message("my-session", SessionInput::user_message("Hello!")).await?;
//!
//! // Receive outputs
//! while let Some((session_id, output)) = output_rx.recv().await {
//!     match output {
//!         SessionOutput::AssistantMessage { content, .. } => println!("{}", content),
//!         SessionOutput::ToolPending { id, name, .. } => {
//!             // Ask user for approval, then:
//!             manager.push_message(&session_id, SessionInput::approve_tool(id)).await?;
//!         }
//!         _ => {}
//!     }
//! }
//! ```

mod agent_loop;
mod manager;
mod types;

pub use agent_loop::AgentLoop;
pub use manager::{ConfigFactory, OutputReceiver, SessionManager};
pub use types::{
    QuestionInfo, QuestionOption, SessionConfig, SessionId, SessionInput, SessionOutput,
};
