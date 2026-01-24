//! TUI (Terminal User Interface) module for Cowork CLI
//!
//! This module provides a non-blocking terminal UI similar to Claude Code,
//! where the input area is always visible at the bottom of the screen
//! and output appears above it.

mod app;
pub mod events;
mod ui;

pub use app::{App, Message, MessageType, Modal, PendingApproval, PendingQuestion};
pub use events::{Event, EventHandler, KeyAction, handle_key_approval, handle_key_normal, handle_key_question};
pub use ui::draw;
