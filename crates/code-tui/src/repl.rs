//! Main REPL screen renderer.
//!
//! Draws a 3-panel layout: scrollable message list (top), input box (middle),
//! status bar (bottom).
//!
//! Ref: src/screens/REPL.tsx (layout and message rendering)

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, StatefulWidget, Widget};
use ratatui::Frame;

use code_types::message::{ContentBlock, Message, SystemMessageLevel, ToolResultContent};

use crate::app::App;
use crate::input::render_input;
use crate::markdown::render_markdown;
use crate::spinner::SpinnerWidget;
use crate::status_bar::render_status_bar;

// ── Layout ────────────────────────────────────────────────────────────────────

/// Render the full REPL screen onto `frame`.
pub fn render_repl(app: &mut App, frame: &mut Frame) {
    let area = frame.area();

    let [msg_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(area);

    render_messages(app, frame.buffer_mut(), msg_area);
    render_input(&app.input, input_area, frame.buffer_mut(), &app.theme);
    render_status_bar(&app.status, status_area.width, &app.theme)
        .render(status_area, frame.buffer_mut());

    // Spinner overlay: rendered as the last item in the message area
    // (already included in render_messages via push_message).
    // If actively querying, show a spinner line above the input.
    if app.is_querying {
        let spinner_area = Rect {
            x: msg_area.x,
            y: msg_area.y + msg_area.height.saturating_sub(2),
            width: msg_area.width,
            height: 1,
        };
        SpinnerWidget { spinner: &app.spinner, theme: &app.theme }
            .render(spinner_area, frame.buffer_mut());
    }

    // Dialog overlay.
    if app.dialog.is_active() {
        app.dialog.render(msg_area, frame.buffer_mut(), &app.theme);
    }
}

// ── Message list ──────────────────────────────────────────────────────────────

fn render_messages(app: &mut App, buf: &mut Buffer, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Claude Code ",
            Style::default()
                .fg(app.theme.claude)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(app.theme.subtle));

    let inner = block.inner(area);
    block.render(area, buf);

    let width = inner.width;

    // Build list items from messages.
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .filter_map(|msg| message_to_item(msg, width, &app.theme))
        .collect();

    let list = List::new(items);
    StatefulWidget::render(list, inner, buf, &mut app.list_state);
}

fn message_to_item<'a>(
    msg: &'a Message,
    width: u16,
    theme: &'a crate::theme::Theme,
) -> Option<ListItem<'static>> {
    match msg {
        Message::User(u) => {
            let text: String = u
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    ContentBlock::ToolResult(tr) => {
                        let s = match &tr.content {
                            ToolResultContent::Text(t) => t.clone(),
                            ToolResultContent::Blocks(blocks) => blocks
                                .iter()
                                .filter_map(|b| {
                                    if let ContentBlock::Text(t) = b {
                                        Some(t.text.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n"),
                        };
                        Some(format!("  [tool result]: {s}"))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");

            if text.trim().is_empty() {
                return None;
            }

            let label = Span::styled("You: ", Style::default().fg(theme.subtle));
            let content = Span::raw(text);
            let line = Line::from(vec![label, content]);
            Some(ListItem::new(line))
        }

        Message::Assistant(a) => {
            let md_text: String = a
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    ContentBlock::ToolUse(tu) => {
                        Some(format!("  🔧 {}(…)", tu.name))
                    }
                    ContentBlock::Thinking(th) => {
                        Some(format!("  💭 {}", &th.thinking[..th.thinking.len().min(80)]))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");

            let label = Span::styled(
                "Claude: ",
                Style::default()
                    .fg(theme.claude)
                    .add_modifier(Modifier::BOLD),
            );

            let mut lines: Vec<Line<'static>> = Vec::new();
            let label_line = Line::from(label);
            lines.push(label_line);

            let rendered = render_markdown(&md_text, width.saturating_sub(2), theme);
            lines.extend(rendered);

            Some(ListItem::new(lines))
        }

        Message::Progress(p) => {
            let line = Line::from(Span::styled(
                format!("  ✻ {}…", p.tool_name),
                Style::default().fg(theme.claude),
            ));
            Some(ListItem::new(line))
        }

        Message::SystemInformational(s) => {
            let color = match s.level {
                SystemMessageLevel::Info => theme.subtle,
                SystemMessageLevel::Warning => theme.warning,
                SystemMessageLevel::Error => theme.error,
            };
            let line = Line::from(Span::styled(
                s.content.clone(),
                Style::default().fg(color).add_modifier(Modifier::ITALIC),
            ));
            Some(ListItem::new(line))
        }

        Message::SystemApiError(e) => {
            let line = Line::from(Span::styled(
                format!("⚠ API Error: {}", e.error),
                Style::default().fg(theme.error),
            ));
            Some(ListItem::new(line))
        }

        Message::SystemTurnDuration(d) => {
            let cost = d.cost_usd;
            let ms = d.duration_ms;
            let line = Line::from(Span::styled(
                format!("  ({ms}ms  ${cost:.4})"),
                Style::default()
                    .fg(theme.subtle)
                    .add_modifier(Modifier::DIM),
            ));
            Some(ListItem::new(line))
        }

        Message::SystemCompactBoundary(c) => {
            let line = Line::from(Span::styled(
                format!(
                    "── Context compacted ({} → {} tokens) ──",
                    c.tokens_before, c.tokens_after
                ),
                Style::default().fg(theme.permission),
            ));
            Some(ListItem::new(line))
        }

        Message::SystemMemorySaved(m) => {
            let line = Line::from(Span::styled(
                format!("✓ Memory saved: {}", m.path),
                Style::default().fg(theme.success),
            ));
            Some(ListItem::new(line))
        }

        // Skip purely internal messages.
        Message::Tombstone(_)
        | Message::Attachment(_)
        | Message::ToolUseSummary(_)
        | Message::SystemMicrocompactBoundary(_)
        | Message::SystemPermissionRetry(_) => None,
    }
}
