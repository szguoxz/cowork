//! Application state and types for the TUI

use cowork_core::context::ContextUsage;
use cowork_core::formatting::{format_ephemeral, truncate_str};
pub use cowork_core::DiffLine;
use std::time::Instant;
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
    /// Tool call message (Claude Code style: ● ToolName(args...))
    ToolCall {
        formatted: String,
        elapsed_secs: f32,
    },
    /// Tool result message (Claude Code style: ⎿ summary)
    ToolResult {
        summary: String,
        success: bool,
        elapsed_secs: f32,
        diff: Option<Vec<DiffLine>>,
        expanded: bool,
    },
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

    pub fn tool_call(formatted: impl Into<String>, elapsed_secs: f32) -> Self {
        Self {
            message_type: MessageType::ToolCall {
                formatted: formatted.into(),
                elapsed_secs,
            },
            content: String::new(),
        }
    }

    pub fn tool_result(
        summary: impl Into<String>,
        success: bool,
        elapsed_secs: f32,
        diff: Option<Vec<DiffLine>>,
    ) -> Self {
        Self {
            message_type: MessageType::ToolResult {
                summary: summary.into(),
                success,
                elapsed_secs,
                diff,
                expanded: false,
            },
            content: String::new(),
        }
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
    /// When the current turn started (user submitted message)
    pub turn_start: Option<Instant>,
    /// Whether plan mode is active
    pub plan_mode: bool,
    /// Current context usage (tokens used/total)
    pub context_usage: Option<ContextUsage>,
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
            turn_start: None,
            plan_mode: false,
            context_usage: None,
        }
    }

    /// Start a new turn (when user submits a message)
    pub fn start_turn(&mut self) {
        self.turn_start = Some(Instant::now());
    }

    /// Get elapsed seconds since turn started
    pub fn elapsed_secs(&self) -> f32 {
        self.turn_start
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0)
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
            SessionOutput::TextDelta { delta, .. } => {
                // Streaming text - append to ephemeral display
                if let Some(ref mut ephemeral) = self.ephemeral {
                    ephemeral.push_str(&delta);
                } else {
                    self.ephemeral = Some(delta);
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
                self.ephemeral = Some(format_ephemeral(&name, &arguments));
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
            SessionOutput::ToolCall { formatted, .. } => {
                // Add tool call as a persistent message with elapsed time
                let elapsed = self.elapsed_secs();
                self.add_message(Message::tool_call(&formatted, elapsed));
            }
            SessionOutput::ToolResult { summary, success, diff_preview, .. } => {
                // Add tool result as a persistent message with elapsed time
                let elapsed = self.elapsed_secs();
                self.add_message(Message::tool_result(&summary, success, elapsed, diff_preview));
                // Clear ephemeral since we have the result
                self.ephemeral = None;
            }
            SessionOutput::Question { request_id, questions, .. } => {
                self.modal = Some(Modal::Question(PendingQuestion::new(request_id, questions)));
            }
            SessionOutput::Error { message } => {
                self.add_message(Message::error(message));
                self.status.clear();
                self.ephemeral = None;
            }
            SessionOutput::Cancelled => {
                self.add_message(Message::system("Cancelled".to_string()));
                self.status.clear();
                self.ephemeral = None;
                self.modal = None;
            }
            SessionOutput::PlanModeChanged { active, plan_file } => {
                self.plan_mode = active;
                if active {
                    if let Some(ref path) = plan_file {
                        self.add_message(Message::system(format!(
                            "Plan mode enabled. Write your plan to: {}", path
                        )));
                    } else {
                        self.add_message(Message::system("Plan mode enabled."));
                    }
                } else {
                    self.add_message(Message::system("Plan mode disabled."));
                }
            }
            SessionOutput::ContextUpdate { usage } => {
                self.context_usage = Some(usage);
            }
        }
    }
}

// Formatting functions now imported from cowork_core::formatting
