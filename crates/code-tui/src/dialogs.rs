//! Modal dialog widgets: permission requests and cost warnings.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::theme::Theme;

/// The active dialog variant.
#[derive(Debug, Clone)]
pub enum Dialog {
    /// No dialog is shown.
    None,
    /// A tool is requesting permission to execute.
    PermissionRequest {
        tool_name: String,
        description: String,
        input_preview: String,
        /// `true` = Allow button focused, `false` = Deny button focused.
        focused_allow: bool,
    },
    /// Warn the user that costs have exceeded a threshold.
    CostWarning { cost_usd: f64 },
}

/// The result of interacting with a dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    Allow,
    Deny,
    Dismiss,
}

impl Dialog {
    /// Returns `true` when a dialog is currently active.
    pub fn is_active(&self) -> bool {
        !matches!(self, Dialog::None)
    }

    /// Handle a key event and optionally produce a `DialogResult`.
    pub fn handle_key(&mut self, key: &KeyEvent) -> Option<DialogResult> {
        match self {
            Dialog::None => None,
            Dialog::PermissionRequest {
                focused_allow, ..
            } => match key.code {
                KeyCode::Char('a') | KeyCode::Char('y') => Some(DialogResult::Allow),
                KeyCode::Char('d') | KeyCode::Char('n') | KeyCode::Esc => {
                    Some(DialogResult::Deny)
                }
                KeyCode::Tab => {
                    *focused_allow = !*focused_allow;
                    None
                }
                KeyCode::Enter => {
                    if *focused_allow {
                        Some(DialogResult::Allow)
                    } else {
                        Some(DialogResult::Deny)
                    }
                }
                _ => None,
            },
            Dialog::CostWarning { .. } => Some(DialogResult::Dismiss),
        }
    }

    /// Render the dialog into `buf` within `area`.
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        match self {
            Dialog::None => {}
            Dialog::PermissionRequest {
                tool_name,
                description,
                input_preview,
                focused_allow,
            } => {
                let dialog_area = centered_rect(area);
                Clear.render(dialog_area, buf);

                let block = Block::bordered()
                    .border_style(Style::default().fg(theme.permission))
                    .title(Span::styled(
                        " Permission Request ",
                        Style::default()
                            .fg(theme.permission)
                            .add_modifier(Modifier::BOLD),
                    ));

                let inner = block.inner(dialog_area);
                block.render(dialog_area, buf);

                let allow_style = if *focused_allow {
                    Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(theme.success)
                };
                let deny_style = if !*focused_allow {
                    Style::default()
                        .fg(theme.error)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default().fg(theme.error)
                };

                let lines: Vec<Line<'static>> = vec![
                    Line::from(Span::styled(
                        format!("Tool: {}", tool_name),
                        Style::default().add_modifier(Modifier::BOLD),
                    )),
                    Line::default(),
                    Line::from(Span::raw(description.clone())),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("Input: ".to_owned(), Style::default().fg(theme.subtle)),
                        Span::raw(input_preview.clone()),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("[Allow]".to_owned(), allow_style),
                        Span::raw("  ".to_owned()),
                        Span::styled("[Deny]".to_owned(), deny_style),
                    ]),
                ];

                Paragraph::new(lines).render(inner, buf);
            }
            Dialog::CostWarning { cost_usd } => {
                let dialog_area = centered_rect(area);
                Clear.render(dialog_area, buf);

                let block = Block::bordered()
                    .border_style(Style::default().fg(theme.warning))
                    .title(Span::styled(
                        " Cost Warning ",
                        Style::default()
                            .fg(theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ));

                let inner = block.inner(dialog_area);
                block.render(dialog_area, buf);

                let lines: Vec<Line<'static>> = vec![
                    Line::from(Span::styled(
                        format!("Current cost: ${:.4}", cost_usd),
                        Style::default().fg(theme.warning),
                    )),
                    Line::default(),
                    Line::from(Span::raw("Press any key to continue".to_owned())),
                ];

                Paragraph::new(lines).render(inner, buf);
            }
        }
    }
}

/// Compute a centered dialog rect: half the width, one-third the height.
fn centered_rect(area: Rect) -> Rect {
    Rect {
        x: area.x + area.width / 4,
        y: area.y + area.height / 3,
        width: area.width / 2,
        height: area.height / 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_dialog_is_not_active() {
        let d = Dialog::None;
        assert!(!d.is_active());
    }
}
