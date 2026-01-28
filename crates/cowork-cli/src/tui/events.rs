//! Event handling for the TUI

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;
use tokio::sync::mpsc;

use cowork_core::session::SessionOutput;

/// Events that can occur in the TUI
#[derive(Debug)]
pub enum Event {
    /// Terminal event (key press, resize, etc.)
    Terminal(CrosstermEvent),
    /// Session output from the agent loop
    Session(String, SessionOutput),
    /// Tick for UI refresh
    Tick,
}

/// Event handler that polls for terminal events and session outputs
pub struct EventHandler {
    /// Receiver for events
    rx: mpsc::UnboundedReceiver<Event>,
    /// Sender for events (kept to clone for session output forwarding)
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new(mut output_rx: cowork_core::session::OutputReceiver) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();

        // Spawn terminal event polling task
        let tx_terminal = tx.clone();
        std::thread::spawn(move || {
            loop {
                // Poll with a short timeout to allow checking for shutdown
                if event::poll(Duration::from_millis(50)).unwrap_or(false)
                    && let Ok(evt) = event::read()
                        && tx_terminal.send(Event::Terminal(evt)).is_err() {
                            break;
                        }
                // Send tick for UI refresh
                if tx_terminal.send(Event::Tick).is_err() {
                    break;
                }
            }
        });

        // Spawn session output forwarding task
        tokio::spawn(async move {
            while let Some((session_id, output)) = output_rx.recv().await {
                if tx_clone.send(Event::Session(session_id, output)).is_err() {
                    break;
                }
            }
        });

        Self { rx, _tx: tx }
    }

    /// Get the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}

/// Result of handling a key event
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// No action needed
    None,
    /// Submit the current input
    Submit(String),
    /// Quit the application
    Quit,
    /// Cancel the current turn
    Cancel,
    /// Approve the pending tool
    ApproveTool,
    /// Reject the pending tool
    RejectTool,
    /// Approve tool for session
    ApproveToolSession,
    /// Approve all tools for session
    ApproveAllSession,
    /// Answer question and move to next
    AnswerQuestion,
    /// Scroll up
    ScrollUp,
    /// Scroll down
    ScrollDown,
    /// Page up
    PageUp,
    /// Page down
    PageDown,
    /// History previous
    HistoryPrev,
    /// History next
    HistoryNext,
}

/// Handle a key event in normal mode
pub fn handle_key_normal(key: KeyEvent, input: &mut tui_input::Input) -> KeyAction {
    match key.code {
        KeyCode::Enter => {
            let value = input.value().to_string();
            if !value.trim().is_empty() {
                input.reset();
                KeyAction::Submit(value)
            } else {
                KeyAction::None
            }
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        KeyCode::Up if key.modifiers.contains(KeyModifiers::SHIFT) => KeyAction::ScrollUp,
        KeyCode::Down if key.modifiers.contains(KeyModifiers::SHIFT) => KeyAction::ScrollDown,
        KeyCode::Up => KeyAction::HistoryPrev,
        KeyCode::Down => KeyAction::HistoryNext,
        KeyCode::PageUp => KeyAction::PageUp,
        KeyCode::PageDown => KeyAction::PageDown,
        KeyCode::Char(c) => {
            input.handle(tui_input::InputRequest::InsertChar(c));
            KeyAction::None
        }
        KeyCode::Backspace => {
            input.handle(tui_input::InputRequest::DeletePrevChar);
            KeyAction::None
        }
        KeyCode::Delete => {
            input.handle(tui_input::InputRequest::DeleteNextChar);
            KeyAction::None
        }
        KeyCode::Left => {
            input.handle(tui_input::InputRequest::GoToPrevChar);
            KeyAction::None
        }
        KeyCode::Right => {
            input.handle(tui_input::InputRequest::GoToNextChar);
            KeyAction::None
        }
        KeyCode::Home => {
            input.handle(tui_input::InputRequest::GoToStart);
            KeyAction::None
        }
        KeyCode::End => {
            input.handle(tui_input::InputRequest::GoToEnd);
            KeyAction::None
        }
        _ => KeyAction::None,
    }
}

/// Handle a key event in tool approval mode
pub fn handle_key_approval(key: KeyEvent, approval: &mut super::PendingApproval) -> KeyAction {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            approval.select_prev();
            KeyAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            approval.select_next();
            KeyAction::None
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            match approval.selected_option {
                0 => KeyAction::ApproveTool,
                1 => KeyAction::RejectTool,
                2 => KeyAction::ApproveToolSession,
                3 => KeyAction::ApproveAllSession,
                _ => KeyAction::RejectTool,
            }
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => KeyAction::ApproveTool,
        KeyCode::Char('n') | KeyCode::Char('N') => KeyAction::RejectTool,
        KeyCode::Esc => KeyAction::RejectTool,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
        _ => KeyAction::None,
    }
}

/// Handle a key event in question mode
pub fn handle_key_question(key: KeyEvent, question: &mut super::PendingQuestion) -> KeyAction {
    if question.in_custom_input_mode {
        // Handle custom input mode
        match key.code {
            KeyCode::Enter => {
                question.in_custom_input_mode = false;
                KeyAction::AnswerQuestion
            }
            KeyCode::Esc => {
                question.in_custom_input_mode = false;
                question.custom_input = None;
                KeyAction::None
            }
            KeyCode::Char(c) => {
                let input = question.custom_input.get_or_insert_with(String::new);
                input.push(c);
                KeyAction::None
            }
            KeyCode::Backspace => {
                if let Some(ref mut input) = question.custom_input {
                    input.pop();
                }
                KeyAction::None
            }
            _ => KeyAction::None,
        }
    } else {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                question.select_prev();
                KeyAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                question.select_next();
                KeyAction::None
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if question.is_other_selected() {
                    question.in_custom_input_mode = true;
                    question.custom_input = Some(String::new());
                    KeyAction::None
                } else {
                    KeyAction::AnswerQuestion
                }
            }
            KeyCode::Esc => {
                // Cancel/skip question
                KeyAction::AnswerQuestion
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => KeyAction::Quit,
            _ => KeyAction::None,
        }
    }
}
