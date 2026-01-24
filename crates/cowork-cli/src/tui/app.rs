//! Application state and types for the TUI

use cowork_core::session::SessionOutput;
use cowork_core::QuestionInfo;
use std::collections::{HashMap, VecDeque};
use tui_input::Input;

/// Message types for display in the output area
#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Assistant,
    System,
    Error,
    ToolStart { name: String },
    ToolDone { name: String, success: bool },
}

/// A message displayed in the output area
#[derive(Debug, Clone)]
pub struct Message {
    pub message_type: MessageType,
    pub content: String,
}

impl Message {
    pub fn new(message_type: MessageType, content: impl Into<String>) -> Self {
        Self {
            message_type,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageType::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageType::Assistant, content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageType::System, content)
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self::new(MessageType::Error, content)
    }

    pub fn tool_start(name: impl Into<String>, args: impl Into<String>) -> Self {
        let name = name.into();
        Self::new(
            MessageType::ToolStart { name: name.clone() },
            format!("Executing {} with: {}", name, args.into()),
        )
    }

    pub fn tool_done(name: impl Into<String>, success: bool, result: impl Into<String>) -> Self {
        let name = name.into();
        let content = result.into();
        Self::new(
            MessageType::ToolDone {
                name: name.clone(),
                success,
            },
            if content.is_empty() {
                format!("{} {}", name, if success { "completed" } else { "failed" })
            } else {
                content
            },
        )
    }

}

/// Pending tool approval request
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub selected_option: usize,
}

impl PendingApproval {
    pub fn new(id: String, name: String, arguments: serde_json::Value) -> Self {
        Self {
            id,
            name,
            arguments,
            selected_option: 0,
        }
    }

    pub fn options(&self) -> &[&str] {
        &[
            "Yes - approve this call",
            "No - reject this call",
            "Always - auto-approve for session",
            "Approve all - auto-approve everything",
        ]
    }

    pub fn select_next(&mut self) {
        self.selected_option = (self.selected_option + 1) % 4;
    }

    pub fn select_prev(&mut self) {
        self.selected_option = if self.selected_option == 0 {
            3
        } else {
            self.selected_option - 1
        };
    }
}

/// Pending question from ask_user_question tool
#[derive(Debug, Clone)]
pub struct PendingQuestion {
    pub request_id: String,
    pub questions: Vec<QuestionInfo>,
    pub current_question: usize,
    pub selected_options: Vec<usize>,
    pub answers: HashMap<String, String>,
    pub custom_input: Option<String>,
    pub in_custom_input_mode: bool,
}

impl PendingQuestion {
    pub fn new(request_id: String, questions: Vec<QuestionInfo>) -> Self {
        let num_questions = questions.len();
        Self {
            request_id,
            questions,
            current_question: 0,
            selected_options: vec![0; num_questions],
            answers: HashMap::new(),
            custom_input: None,
            in_custom_input_mode: false,
        }
    }

    pub fn current(&self) -> Option<&QuestionInfo> {
        self.questions.get(self.current_question)
    }

    pub fn select_next(&mut self) {
        if let Some(q) = self.current() {
            let max = q.options.len(); // +1 for "Other" option
            let current = self.selected_options.get(self.current_question).copied().unwrap_or(0);
            if self.current_question < self.selected_options.len() {
                self.selected_options[self.current_question] = (current + 1) % (max + 1);
            }
        }
    }

    pub fn select_prev(&mut self) {
        if let Some(q) = self.current() {
            let max = q.options.len(); // +1 for "Other" option
            let current = self.selected_options.get(self.current_question).copied().unwrap_or(0);
            if self.current_question < self.selected_options.len() {
                self.selected_options[self.current_question] = if current == 0 { max } else { current - 1 };
            }
        }
    }

    pub fn is_other_selected(&self) -> bool {
        if let Some(q) = self.current() {
            let selected = self.selected_options.get(self.current_question).copied().unwrap_or(0);
            selected == q.options.len()
        } else {
            false
        }
    }
}

/// Pending user interaction (approval or question)
#[derive(Debug, Clone)]
pub enum Interaction {
    ToolApproval(PendingApproval),
    Question(PendingQuestion),
}

/// Application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Normal mode - user can type messages
    Normal,
    /// Waiting for user interaction (tool approval or question)
    Interaction,
    /// Processing (AI is working)
    Processing,
}

/// Main TUI application
pub struct App {
    /// Current application state
    pub state: AppState,
    /// Text input buffer
    pub input: Input,
    /// Message history
    pub messages: Vec<Message>,
    /// Scroll offset for message area
    pub scroll_offset: usize,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Pending interactions queue
    pub interactions: VecDeque<Interaction>,
    /// Session-approved tools
    pub session_approved_tools: std::collections::HashSet<String>,
    /// Approve all tools for session
    pub approve_all_session: bool,
    /// Current thinking content (for display)
    pub thinking_content: Option<String>,
    /// Status message (shown in footer)
    pub status: String,
    /// Provider info for display
    pub provider_info: String,
}

impl App {
    pub fn new(provider_info: String) -> Self {
        Self {
            state: AppState::Normal,
            input: Input::default(),
            messages: vec![
                Message::system("Welcome to Cowork - AI Coding Assistant"),
                Message::system("Type your message and press Enter. Press Ctrl+C or type /exit to quit."),
            ],
            scroll_offset: 0,
            should_quit: false,
            interactions: VecDeque::new(),
            session_approved_tools: std::collections::HashSet::new(),
            approve_all_session: false,
            thinking_content: None,
            status: String::new(),
            provider_info,
        }
    }

    /// Add a message to the history
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        // Auto-scroll to bottom when new message arrives
        self.scroll_to_bottom();
    }

    /// Scroll to the bottom of messages
    pub fn scroll_to_bottom(&mut self) {
        // Scroll offset will be calculated during render based on viewport
        self.scroll_offset = usize::MAX;
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Check if a tool should be auto-approved
    pub fn should_auto_approve(&self, tool_name: &str) -> bool {
        self.approve_all_session || self.session_approved_tools.contains(tool_name)
    }

    /// Process a session output event
    pub fn handle_session_output(&mut self, output: SessionOutput) {
        match output {
            SessionOutput::Ready => {
                self.status = "Ready".to_string();
            }
            SessionOutput::Idle => {
                self.state = AppState::Normal;
                self.thinking_content = None;
                self.status.clear();
            }
            SessionOutput::UserMessage { content, .. } => {
                // Already shown when user types, skip echo
                let _ = content;
            }
            SessionOutput::Thinking { content } => {
                self.thinking_content = if content.is_empty() {
                    None
                } else {
                    Some(content)
                };
                self.status = "Thinking...".to_string();
            }
            SessionOutput::AssistantMessage { content, .. } => {
                if !content.is_empty() {
                    self.add_message(Message::assistant(content));
                }
                self.state = AppState::Normal;
            }
            SessionOutput::ToolStart { name, arguments, .. } => {
                let args_str = serde_json::to_string_pretty(&arguments)
                    .unwrap_or_else(|_| arguments.to_string());
                self.add_message(Message::tool_start(&name, args_str));
                self.status = format!("Executing {}...", name);
            }
            SessionOutput::ToolPending { id, name, arguments, .. } => {
                self.interactions.push_back(Interaction::ToolApproval(PendingApproval::new(id, name, arguments)));
                self.state = AppState::Interaction;
            }
            SessionOutput::ToolDone { name, success, output, .. } => {
                self.add_message(Message::tool_done(&name, success, &output));
                self.status.clear();
            }
            SessionOutput::Question { request_id, questions } => {
                self.interactions.push_back(Interaction::Question(PendingQuestion::new(request_id, questions)));
                self.state = AppState::Interaction;
            }
            SessionOutput::Error { message } => {
                self.add_message(Message::error(message));
                self.state = AppState::Normal;
            }
        }
    }
}
