//! Application state and types for the TUI

use cowork_core::session::SessionOutput;
use cowork_core::QuestionInfo;
use std::collections::{HashMap, HashSet};
use tui_input::Input;

/// Message types for display in the output area
#[derive(Debug, Clone)]
pub enum MessageType {
    User,
    Assistant,
    System,
    Error,
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
            let max = q.options.len();
            let current = self.selected_options.get(self.current_question).copied().unwrap_or(0);
            if self.current_question < self.selected_options.len() {
                self.selected_options[self.current_question] = (current + 1) % (max + 1);
            }
        }
    }

    pub fn select_prev(&mut self) {
        if let Some(q) = self.current() {
            let max = q.options.len();
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

/// Modal overlay — when present, input is disabled and modal is shown
#[derive(Debug, Clone)]
pub enum Modal {
    Approval(PendingApproval),
    Question(PendingQuestion),
}

/// Main TUI application
pub struct App {
    /// Persistent messages (User, Assistant, System, Error)
    pub messages: Vec<Message>,
    /// Current ephemeral activity line (overwritten by each tool event)
    pub ephemeral: Option<String>,
    /// High-level status: "Processing", "Thinking", "" (empty = idle)
    pub status: String,
    /// Tick counter for spinner animation
    pub tick: usize,
    /// Scroll offset for message area
    pub scroll_offset: usize,
    /// Text input buffer
    pub input: Input,
    /// Modal overlay (None = no modal, Some = show modal + disable input)
    pub modal: Option<Modal>,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Provider info for status bar display
    pub provider_info: String,
    /// Version string for status bar
    pub version: String,
    /// Input history
    pub history: Vec<String>,
    /// Current position in history (None = not browsing)
    pub history_index: Option<usize>,
    /// Saved current input when browsing history
    pub history_draft: String,
    /// Session-approved tools
    pub session_approved_tools: HashSet<String>,
    /// Approve all tools for session
    pub approve_all_session: bool,
}

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl App {
    pub fn new(provider_info: String, version: String) -> Self {
        Self {
            messages: vec![
                Message::system("Welcome to Cowork. Type your message and press Enter. Ctrl+C to quit."),
            ],
            ephemeral: None,
            status: String::new(),
            tick: 0,
            scroll_offset: 0,
            input: Input::default(),
            modal: None,
            should_quit: false,
            provider_info,
            version,
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            session_approved_tools: HashSet::new(),
            approve_all_session: false,
        }
    }

    /// Advance spinner
    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Get current spinner char
    pub fn spinner(&self) -> char {
        SPINNER[self.tick % SPINNER.len()]
    }

    /// Add a message to the history
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.scroll_to_bottom();
    }

    /// Scroll to the bottom of messages
    pub fn scroll_to_bottom(&mut self) {
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

    /// Push input to history
    pub fn push_history(&mut self, input: String) {
        if !input.is_empty() {
            self.history.push(input);
        }
        self.history_index = None;
        self.history_draft.clear();
    }

    /// Navigate to previous history entry
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_index = match self.history_index {
            None => {
                self.history_draft = self.input.value().to_string();
                self.history.len() - 1
            }
            Some(0) => return,
            Some(i) => i - 1,
        };
        self.history_index = Some(new_index);
        self.input = Input::new(self.history[new_index].clone());
    }

    /// Navigate to next history entry
    pub fn history_next(&mut self) {
        let Some(idx) = self.history_index else { return };
        if idx + 1 >= self.history.len() {
            self.history_index = None;
            self.input = Input::new(self.history_draft.clone());
        } else {
            self.history_index = Some(idx + 1);
            self.input = Input::new(self.history[idx + 1].clone());
        }
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
                self.status.clear();
                self.ephemeral = None;
            }
            SessionOutput::UserMessage { .. } => {}
            SessionOutput::Thinking { content } => {
                if content.is_empty() {
                    self.status = "Processing...".to_string();
                } else {
                    self.status = "Thinking...".to_string();
                }
            }
            SessionOutput::AssistantMessage { content, .. } => {
                if !content.is_empty() {
                    self.add_message(Message::assistant(content));
                }
                self.status.clear();
                self.ephemeral = None;
            }
            SessionOutput::ToolStart { name, arguments, .. } => {
                self.status = "Processing...".to_string();
                let summary = format_tool_args(&name, &arguments);
                self.ephemeral = Some(format!("{}: {}", name, truncate_str(&summary, 80)));
            }
            SessionOutput::ToolPending { id, name, arguments, .. } => {
                self.modal = Some(Modal::Approval(PendingApproval::new(id, name, arguments)));
            }
            SessionOutput::ToolDone { name, success, output, .. } => {
                if success {
                    self.ephemeral = Some(format!("{}: done", name));
                } else {
                    let err = truncate_str(&output, 80);
                    self.ephemeral = Some(format!("{}: {}", name, err));
                }
            }
            SessionOutput::Question { request_id, questions } => {
                self.modal = Some(Modal::Question(PendingQuestion::new(request_id, questions)));
            }
            SessionOutput::Error { message } => {
                self.add_message(Message::error(message));
                self.status.clear();
                self.ephemeral = None;
            }
        }
    }
}

/// Format tool arguments into a concise summary
fn format_tool_args(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "Read" => args["file_path"].as_str().unwrap_or("?").to_string(),
        "Write" => args["file_path"].as_str().unwrap_or("?").to_string(),
        "Edit" => args["file_path"].as_str().unwrap_or("?").to_string(),
        "Glob" => args["pattern"].as_str().unwrap_or("?").to_string(),
        "Grep" => {
            let pattern = args["pattern"].as_str().unwrap_or("?");
            let path = args["path"].as_str().unwrap_or(".");
            format!("{} in {}", pattern, path)
        }
        "Bash" => {
            let cmd = args["command"].as_str().unwrap_or("?");
            truncate_str(cmd, 100)
        }
        "Task" => {
            let desc = args["description"].as_str().unwrap_or("?");
            let agent = args["subagent_type"].as_str().unwrap_or("?");
            format!("[{}] {}", agent, desc)
        }
        "WebFetch" => args["url"].as_str().unwrap_or("?").to_string(),
        "WebSearch" => args["query"].as_str().unwrap_or("?").to_string(),
        "LSP" => {
            let op = args["operation"].as_str().unwrap_or("?");
            let file = args["filePath"].as_str().unwrap_or("?");
            format!("{} {}", op, file)
        }
        _ => {
            serde_json::to_string(args).unwrap_or_default()
        }
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
