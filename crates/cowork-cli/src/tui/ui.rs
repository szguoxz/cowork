//! UI rendering for the TUI

use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use cowork_core::formatting::format_approval_args;
use cowork_core::DiffLine;

use super::{App, Message, MessageType, Modal, PendingApproval, PendingQuestion};

/// Draw the entire UI
pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Messages area
            Constraint::Length(1), // Status bar
            Constraint::Length(3), // Input area
        ])
        .split(frame.area());

    draw_messages(frame, app, chunks[0]);
    draw_status_bar(frame, app, chunks[1]);
    draw_input(frame, app, chunks[2]);

    // Draw modal overlay if present
    if let Some(ref modal) = app.modal {
        draw_modal(frame, modal);
    }
}

/// Draw the messages area with persistent messages + ephemeral line at bottom
fn draw_messages(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Messages ");

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.is_empty() && app.ephemeral.is_none() {
        return;
    }

    let max_width = inner_area.width as usize - 2;

    // Convert persistent messages to list items
    let mut items: Vec<ListItem> = app
        .messages
        .iter()
        .flat_map(|msg| message_to_lines(msg, max_width))
        .collect();

    // Append ephemeral activity lines (dim) if present - up to 3 lines
    if let Some(ref ephemeral) = app.ephemeral {
        for (i, line_text) in ephemeral.lines().take(3).enumerate() {
            let prefix = if i == 0 { " \u{2591} " } else { "   " };
            let line = Line::from(Span::styled(
                format!("{}{}", prefix, line_text),
                Style::default().fg(Color::DarkGray),
            ));
            items.push(ListItem::new(line));
        }
    }

    let total_lines = items.len();
    let visible_lines = inner_area.height as usize;

    // Handle auto-scroll
    let scroll = if app.scroll_offset == usize::MAX {
        total_lines.saturating_sub(visible_lines)
    } else {
        app.scroll_offset.min(total_lines.saturating_sub(visible_lines))
    };

    if app.scroll_offset == usize::MAX && total_lines > visible_lines {
        app.scroll_offset = total_lines - visible_lines;
    }

    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(scroll)
        .take(visible_lines)
        .collect();

    let list = List::new(visible_items);
    frame.render_widget(list, inner_area);
}

/// Convert a message to styled lines
fn message_to_lines(msg: &Message, max_width: usize) -> Vec<ListItem<'static>> {
    match &msg.message_type {
        MessageType::Assistant => {
            // Assistant messages get ● prefix for each paragraph
            assistant_to_lines(&msg.content, max_width)
        }
        MessageType::ToolCall { formatted, .. } => {
            // Tool calls: ● ToolName(args...) in cyan
            tool_call_to_lines(formatted, max_width)
        }
        MessageType::ToolResult { summary, success, diff, expanded, .. } => {
            // Tool results: ⎿ summary, with optional diff (red for errors)
            tool_result_to_lines(summary, *success, diff.as_ref(), *expanded, max_width)
        }
        _ => {
            let (prefix, style) = match &msg.message_type {
                MessageType::User => (
                    "You: ",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                MessageType::System => (
                    "",
                    Style::default().fg(Color::DarkGray),
                ),
                MessageType::Error => (
                    "Error: ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                _ => unreachable!(),
            };

            let content_width = max_width.saturating_sub(prefix.len());
            let wrapped_lines = wrap_text(&msg.content, content_width);

            wrapped_lines
                .into_iter()
                .enumerate()
                .map(|(i, line)| {
                    let line_content = if i == 0 {
                        Line::from(vec![
                            Span::styled(prefix.to_string(), style),
                            Span::styled(line, style),
                        ])
                    } else {
                        Line::from(vec![
                            Span::raw(" ".repeat(prefix.len())),
                            Span::styled(line, style),
                        ])
                    };
                    ListItem::new(line_content)
                })
                .collect()
        }
    }
}

/// Render assistant message with ● prefix for each paragraph
fn assistant_to_lines(content: &str, max_width: usize) -> Vec<ListItem<'static>> {
    let prefix = "● ";
    let continuation = "  ";
    let content_width = max_width.saturating_sub(2);
    let mut items: Vec<ListItem> = Vec::new();
    let mut in_code_block = false;
    let code_style = Style::default().fg(Color::Green);
    let code_fence_style = Style::default().fg(Color::DarkGray);
    let prefix_style = Style::default().fg(Color::White);

    for (para_idx, raw_line) in content.split('\n').enumerate() {
        // Detect fenced code block boundaries
        if raw_line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            let line = Line::from(vec![
                Span::styled(if para_idx == 0 { prefix } else { continuation }.to_string(), prefix_style),
                Span::styled(raw_line.to_string(), code_fence_style),
            ]);
            items.push(ListItem::new(line));
            continue;
        }

        if in_code_block {
            let wrapped = wrap_text(raw_line, content_width);
            for w in wrapped {
                let line = Line::from(vec![
                    Span::styled(continuation.to_string(), prefix_style),
                    Span::styled(w, code_style),
                ]);
                items.push(ListItem::new(line));
            }
            continue;
        }

        // Headers
        if let Some(header) = parse_header(raw_line) {
            let wrapped = wrap_text(&header.text, content_width);
            let header_style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);
            for (i, w) in wrapped.into_iter().enumerate() {
                let p = if para_idx == 0 && i == 0 { prefix } else { continuation };
                let line = Line::from(vec![
                    Span::styled(p.to_string(), prefix_style),
                    Span::styled(w, header_style),
                ]);
                items.push(ListItem::new(line));
            }
            continue;
        }

        // Empty line - still show prefix for first paragraph
        if raw_line.is_empty() {
            items.push(ListItem::new(Line::from("")));
            continue;
        }

        // Normal text with inline formatting, wrapped
        let wrapped = wrap_text(raw_line, content_width);
        for (i, w) in wrapped.into_iter().enumerate() {
            let p = if para_idx == 0 && i == 0 { prefix } else { continuation };
            let spans = parse_inline_markdown(&w);
            let mut line_spans = vec![Span::styled(p.to_string(), prefix_style)];
            line_spans.extend(spans);
            items.push(ListItem::new(Line::from(line_spans)));
        }
    }

    items
}

/// Render tool call: ● ToolName(args...) in cyan
fn tool_call_to_lines(formatted: &str, max_width: usize) -> Vec<ListItem<'static>> {
    let prefix = "● ";
    let continuation = "  ";
    let content_width = max_width.saturating_sub(2);
    let prefix_style = Style::default().fg(Color::White);
    let tool_style = Style::default().fg(Color::Cyan);

    let wrapped = wrap_text(formatted, content_width);
    wrapped
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let p = if i == 0 { prefix } else { continuation };
            ListItem::new(Line::from(vec![
                Span::styled(p.to_string(), prefix_style),
                Span::styled(line, tool_style),
            ]))
        })
        .collect()
}

/// Render tool result: ⎿ summary, with optional diff
fn tool_result_to_lines(
    summary: &str,
    success: bool,
    diff: Option<&Vec<DiffLine>>,
    _expanded: bool,
    max_width: usize,
) -> Vec<ListItem<'static>> {
    let prefix = "  ⎿  ";
    let continuation = "     ";
    let content_width = max_width.saturating_sub(5);
    // Use red for errors, gray for success
    let summary_style = if success {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Red)
    };
    let added_style = Style::default().fg(Color::Green);
    let removed_style = Style::default().fg(Color::Red);
    let context_style = Style::default().fg(Color::DarkGray);

    let mut items = Vec::new();

    // Summary line
    let wrapped = wrap_text(summary, content_width);
    for (i, line) in wrapped.into_iter().enumerate() {
        let p = if i == 0 { prefix } else { continuation };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(p.to_string(), summary_style),
            Span::styled(line, summary_style),
        ])));
    }

    // Diff lines (if present)
    if let Some(diff_lines) = diff {
        for diff_line in diff_lines.iter().take(10) {
            let (marker, style) = match diff_line.line_type.as_str() {
                "added" => ("+", added_style),
                "removed" => ("-", removed_style),
                _ => (" ", context_style),
            };

            // Format: "     513 +   content"
            let line_num = diff_line
                .line_number
                .map(|n| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());

            let content = wrap_text(&diff_line.content, content_width.saturating_sub(7))
                .into_iter()
                .next()
                .unwrap_or_default();

            items.push(ListItem::new(Line::from(vec![
                Span::styled(continuation.to_string(), context_style),
                Span::styled(line_num, context_style),
                Span::styled(format!("{} ", marker), style),
                Span::styled(content, style),
            ])));
        }
    }

    items
}

/// Parsed header info
struct HeaderInfo {
    text: String,
}

/// Parse a markdown header line (# ... to ######)
fn parse_header(line: &str) -> Option<HeaderInfo> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    // Header must be followed by space or be empty
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim_start().to_string();
    Some(HeaderInfo { text })
}

/// Parse inline markdown: `code`, **bold**, *italic*, and plain text
fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut plain_start = 0;

    while let Some(&(i, ch)) = chars.peek() {
        match ch {
            '`' => {
                // Inline code
                if i > plain_start {
                    spans.push(Span::styled(
                        text[plain_start..i].to_string(),
                        Style::default().fg(Color::White),
                    ));
                }
                chars.next();
                let code_start = i + 1;
                let mut code_end = None;
                while let Some(&(j, c)) = chars.peek() {
                    chars.next();
                    if c == '`' {
                        code_end = Some(j);
                        break;
                    }
                }
                if let Some(end) = code_end {
                    spans.push(Span::styled(
                        text[code_start..end].to_string(),
                        Style::default().fg(Color::Green),
                    ));
                    plain_start = end + 1;
                } else {
                    // No closing backtick — treat as plain
                    spans.push(Span::styled(
                        text[i..].to_string(),
                        Style::default().fg(Color::White),
                    ));
                    plain_start = text.len();
                    break;
                }
            }
            '*' => {
                // Check for ** (bold) or * (italic)
                let next = text.get(i + 1..i + 2);
                if next == Some("*") {
                    // Bold: **...**
                    if i > plain_start {
                        spans.push(Span::styled(
                            text[plain_start..i].to_string(),
                            Style::default().fg(Color::White),
                        ));
                    }
                    chars.next(); // consume first *
                    chars.next(); // consume second *
                    let bold_start = i + 2;
                    let mut bold_end = None;
                    while let Some(&(j, c)) = chars.peek() {
                        if c == '*' && text.get(j + 1..j + 2) == Some("*") {
                            bold_end = Some(j);
                            chars.next(); // consume first *
                            chars.next(); // consume second *
                            break;
                        }
                        chars.next();
                    }
                    if let Some(end) = bold_end {
                        spans.push(Span::styled(
                            text[bold_start..end].to_string(),
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                        ));
                        plain_start = end + 2;
                    } else {
                        spans.push(Span::styled(
                            text[i..].to_string(),
                            Style::default().fg(Color::White),
                        ));
                        plain_start = text.len();
                        break;
                    }
                } else {
                    // Italic: *...*
                    if i > plain_start {
                        spans.push(Span::styled(
                            text[plain_start..i].to_string(),
                            Style::default().fg(Color::White),
                        ));
                    }
                    chars.next(); // consume *
                    let italic_start = i + 1;
                    let mut italic_end = None;
                    while let Some(&(j, c)) = chars.peek() {
                        if c == '*' {
                            italic_end = Some(j);
                            chars.next(); // consume closing *
                            break;
                        }
                        chars.next();
                    }
                    if let Some(end) = italic_end {
                        spans.push(Span::styled(
                            text[italic_start..end].to_string(),
                            Style::default().fg(Color::White).add_modifier(Modifier::ITALIC),
                        ));
                        plain_start = end + 1;
                    } else {
                        spans.push(Span::styled(
                            text[i..].to_string(),
                            Style::default().fg(Color::White),
                        ));
                        plain_start = text.len();
                        break;
                    }
                }
            }
            _ => {
                chars.next();
            }
        }
    }

    // Remaining plain text
    if plain_start < text.len() {
        spans.push(Span::styled(
            text[plain_start..].to_string(),
            Style::default().fg(Color::White),
        ));
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), Style::default()));
    }

    spans
}

/// Wrap text to fit within a given width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        let mut current_line = String::new();

        for word in words {
            if current_line.is_empty() {
                if word.len() > max_width {
                    let mut remaining = word;
                    while remaining.len() > max_width {
                        lines.push(remaining[..max_width].to_string());
                        remaining = &remaining[max_width..];
                    }
                    current_line = remaining.to_string();
                } else {
                    current_line = word.to_string();
                }
            } else if current_line.len() + 1 + word.len() <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Draw the status bar
fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let time = Local::now().format("%H:%M").to_string();
    let right_info = format!("cowork {} | {} | {}", app.version, app.provider_info, time);

    let (left_text, bg_color) = if !app.status.is_empty() {
        (format!("{} {}", app.spinner(), app.status), Color::Blue)
    } else {
        (String::new(), Color::DarkGray)
    };

    let style = Style::default().bg(bg_color).fg(Color::White);

    // Build the full status bar: left-aligned status, right-aligned info
    let width = area.width as usize;
    let left_len = left_text.len();
    let right_len = right_info.len();
    let padding = width.saturating_sub(left_len + right_len + 2); // +2 for spaces

    let bar_text = format!(" {}{}{} ", left_text, " ".repeat(padding), right_info);

    let paragraph = Paragraph::new(bar_text).style(style);
    frame.render_widget(paragraph, area);
}

/// Draw the input area
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = "You> ";
    let input_active = app.modal.is_none();

    let (input_text, input_style, border_style) = if input_active {
        (
            format!("{}{}", prompt, app.input.value()),
            Style::default(),
            Style::default().fg(Color::Cyan),
        )
    } else {
        (
            format!("{}(waiting...)", prompt),
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::DarkGray),
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Input ")
        .border_style(border_style);

    let paragraph = Paragraph::new(input_text)
        .style(input_style)
        .block(block);

    frame.render_widget(paragraph, area);

    // Set cursor position when input is active
    if input_active {
        let cursor_x = area.x + 1 + prompt.len() as u16 + app.input.visual_cursor() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x.min(area.x + area.width - 2), cursor_y));
    }
}

/// Draw modal overlay (dispatches to approval or question)
fn draw_modal(frame: &mut Frame, modal: &Modal) {
    match modal {
        Modal::Approval(approval) => draw_approval_modal(frame, approval),
        Modal::Question(question) => draw_question_modal(frame, question),
    }
}

/// Draw the tool approval modal
fn draw_approval_modal(frame: &mut Frame, approval: &PendingApproval) {
    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tool Approval Required ")
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Tool name
            Constraint::Min(8),    // Arguments (more space)
            Constraint::Length(6), // Options
        ])
        .split(inner);

    let tool_text = Paragraph::new(format!("Tool: {}", approval.name))
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(tool_text, chunks[0]);

    // Format arguments nicely instead of raw JSON dump
    let args_lines = format_approval_args(&approval.name, &approval.arguments);
    let args_text = Paragraph::new(args_lines.join("\n"))
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::TOP).title(" Details "));
    frame.render_widget(args_text, chunks[1]);

    let options: Vec<ListItem> = approval
        .options()
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == approval.selected_option {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("  {}  ", opt)).style(style)
        })
        .collect();

    let list = List::new(options)
        .block(Block::default().borders(Borders::TOP).title(" Select action (\u{2191}/\u{2193}, Enter) "));
    frame.render_widget(list, chunks[2]);
}

/// Draw the question modal
fn draw_question_modal(frame: &mut Frame, question: &PendingQuestion) {
    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Question ")
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(q) = question.current() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Question text
                Constraint::Min(5),    // Options
                Constraint::Length(3), // Custom input
            ])
            .split(inner);

        let header = q.header.as_deref().unwrap_or("Question");
        let question_text = Paragraph::new(q.question.clone())
            .style(Style::default().add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: true })
            .block(Block::default().title(format!(" {} ", header)));
        frame.render_widget(question_text, chunks[0]);

        let selected = question.selected_options.get(question.current_question).copied().unwrap_or(0);
        let mut options: Vec<ListItem> = q
            .options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let style = if i == selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let text = if let Some(ref desc) = opt.description {
                    format!("  {} - {}  ", opt.label, desc)
                } else {
                    format!("  {}  ", opt.label)
                };
                ListItem::new(text).style(style)
            })
            .collect();

        options.push(ListItem::new("  Other (custom answer)  ").style(
            if selected == q.options.len() {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));

        let list = List::new(options)
            .block(Block::default().borders(Borders::TOP).title(" Options (\u{2191}/\u{2193}, Enter) "));
        frame.render_widget(list, chunks[1]);

        if question.in_custom_input_mode {
            let input_text = question.custom_input.as_deref().unwrap_or("");
            let input = Paragraph::new(format!("> {}", input_text))
                .style(Style::default().fg(Color::Yellow))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Custom Answer ")
                        .border_style(Style::default().fg(Color::Yellow)),
                );
            frame.render_widget(input, chunks[2]);
        }
    }
}

/// Create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
