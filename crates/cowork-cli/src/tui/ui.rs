//! UI rendering for the TUI

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::{App, AppState, Message, MessageType, PendingApproval, PendingQuestion};

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

    // Draw modal overlays if needed
    match app.state {
        AppState::ToolApproval => {
            if let Some(ref approval) = app.pending_approval {
                draw_approval_modal(frame, approval);
            }
        }
        AppState::Question => {
            if let Some(ref question) = app.pending_question {
                draw_question_modal(frame, question);
            }
        }
        _ => {}
    }

    // Draw thinking panel if present
    if let Some(ref thinking) = app.thinking_content {
        draw_thinking_panel(frame, thinking, chunks[0]);
    }
}

/// Draw the messages area
fn draw_messages(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Messages ");

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.is_empty() {
        return;
    }

    // Convert messages to list items
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .flat_map(|msg| message_to_lines(msg, inner_area.width as usize - 2))
        .collect();

    // Calculate scroll
    let total_lines = items.len();
    let visible_lines = inner_area.height as usize;

    // Handle auto-scroll (scroll_offset == usize::MAX means scroll to bottom)
    let scroll = if app.scroll_offset == usize::MAX {
        total_lines.saturating_sub(visible_lines)
    } else {
        app.scroll_offset.min(total_lines.saturating_sub(visible_lines))
    };

    // Update scroll offset for next render
    if app.scroll_offset == usize::MAX && total_lines > visible_lines {
        app.scroll_offset = total_lines - visible_lines;
    }

    // For simple scrolling, just render with offset
    let visible_items: Vec<ListItem> = app
        .messages
        .iter()
        .flat_map(|msg| message_to_lines(msg, inner_area.width as usize - 2))
        .skip(scroll)
        .take(visible_lines)
        .collect();

    let list = List::new(visible_items);
    frame.render_widget(list, inner_area);
}

/// Convert a message to styled lines
fn message_to_lines(msg: &Message, max_width: usize) -> Vec<ListItem<'static>> {
    let (prefix, style) = match &msg.message_type {
        MessageType::User => (
            "You: ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        MessageType::Assistant => (
            "Assistant: ",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        MessageType::System => (
            "System: ",
            Style::default().fg(Color::Yellow),
        ),
        MessageType::Error => (
            "Error: ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        MessageType::ToolStart { name } => (
            &format!("▶ {}: ", name) as &str,
            Style::default().fg(Color::Blue),
        ),
        MessageType::ToolDone { name, success } => {
            let symbol = if *success { "✓" } else { "✗" };
            let color = if *success { Color::Green } else { Color::Red };
            (
                &format!("{} {}: ", symbol, name) as &str,
                Style::default().fg(color),
            )
        }
        MessageType::Thinking => (
            "💭 ",
            Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC),
        ),
    };

    // Handle ToolStart and ToolDone with owned strings
    let prefix_owned: String;
    let actual_prefix = match &msg.message_type {
        MessageType::ToolStart { name } => {
            prefix_owned = format!("▶ {}: ", name);
            &prefix_owned
        }
        MessageType::ToolDone { name, success } => {
            let symbol = if *success { "✓" } else { "✗" };
            prefix_owned = format!("{} {}: ", symbol, name);
            &prefix_owned
        }
        _ => prefix,
    };

    // Wrap content to fit width
    let content_width = max_width.saturating_sub(actual_prefix.len());
    let wrapped_lines = wrap_text(&msg.content, content_width);

    wrapped_lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let line_content = if i == 0 {
                Line::from(vec![
                    Span::styled(actual_prefix.to_string(), style),
                    Span::raw(line),
                ])
            } else {
                Line::from(vec![
                    Span::raw(" ".repeat(actual_prefix.len())),
                    Span::raw(line),
                ])
            };
            ListItem::new(line_content)
        })
        .collect()
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
                    // Word is too long, split it
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
    let status_text = if !app.status.is_empty() {
        format!(" {} | {} ", app.status, app.provider_info)
    } else {
        format!(" {} ", app.provider_info)
    };

    let style = match app.state {
        AppState::Processing => Style::default().bg(Color::Blue).fg(Color::White),
        AppState::ToolApproval => Style::default().bg(Color::Yellow).fg(Color::Black),
        AppState::Question => Style::default().bg(Color::Cyan).fg(Color::Black),
        AppState::Normal => Style::default().bg(Color::DarkGray).fg(Color::White),
    };

    let paragraph = Paragraph::new(status_text).style(style);
    frame.render_widget(paragraph, area);
}

/// Draw the input area
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = match app.state {
        AppState::Normal => "You> ",
        AppState::Processing => "You> ",  // Keep same prompt, user can type
        AppState::ToolApproval => "[Awaiting approval] ",
        AppState::Question => "[Answering question] ",
    };

    // Input is active during Normal and Processing states
    let input_active = matches!(app.state, AppState::Normal | AppState::Processing);

    let input_style = if input_active {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let input_text = format!("{}{}", prompt, app.input.value());

    let title = match app.state {
        AppState::Processing => " Input (processing...) ",
        _ => " Input ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if input_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });

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

/// Draw the tool approval modal
fn draw_approval_modal(frame: &mut Frame, approval: &PendingApproval) {
    let area = centered_rect(60, 50, frame.area());

    // Clear the area behind the modal
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tool Approval Required ")
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Tool name
            Constraint::Min(5),    // Arguments
            Constraint::Length(6), // Options
        ])
        .split(inner);

    // Tool name
    let tool_text = Paragraph::new(format!("Tool: {}", approval.name))
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(tool_text, chunks[0]);

    // Arguments
    let args_str = serde_json::to_string_pretty(&approval.arguments)
        .unwrap_or_else(|_| approval.arguments.to_string());
    let args_text = Paragraph::new(args_str)
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::TOP).title(" Arguments "));
    frame.render_widget(args_text, chunks[1]);

    // Options
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
        .block(Block::default().borders(Borders::TOP).title(" Select action (↑/↓, Enter) "));
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
                Constraint::Length(3), // Custom input (if shown)
            ])
            .split(inner);

        // Question text
        let header = q.header.as_deref().unwrap_or("Question");
        let question_text = Paragraph::new(q.question.clone())
            .style(Style::default().add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: true })
            .block(Block::default().title(format!(" {} ", header)));
        frame.render_widget(question_text, chunks[0]);

        // Options
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

        // Add "Other" option
        let other_selected = selected == q.options.len();
        options.push(ListItem::new("  Other (custom answer)  ").style(
            if other_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));

        let list = List::new(options)
            .block(Block::default().borders(Borders::TOP).title(" Options (↑/↓, Enter) "));
        frame.render_widget(list, chunks[1]);

        // Custom input area
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

/// Draw the thinking panel (overlay on messages area)
fn draw_thinking_panel(frame: &mut Frame, thinking: &str, messages_area: Rect) {
    // Show thinking in a small overlay at the bottom of messages area
    let height = 5.min(thinking.lines().count() as u16 + 2);
    let area = Rect {
        x: messages_area.x + 1,
        y: messages_area.y + messages_area.height - height - 1,
        width: messages_area.width - 2,
        height,
    };

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 💭 Thinking ")
        .border_style(Style::default().fg(Color::Magenta));

    // Truncate thinking content
    let lines: Vec<&str> = thinking.lines().take(3).collect();
    let display_text = if thinking.lines().count() > 3 {
        format!("{}...", lines.join("\n"))
    } else {
        lines.join("\n")
    };

    let paragraph = Paragraph::new(display_text)
        .style(Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC))
        .wrap(Wrap { trim: true })
        .block(block);

    frame.render_widget(paragraph, area);
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
