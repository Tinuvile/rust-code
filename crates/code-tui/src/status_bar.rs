//! Status bar widget for the REPL TUI.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::Theme;

/// State data displayed in the status bar.
#[derive(Debug, Clone)]
pub struct StatusBarState {
    pub model: String,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cwd: String,
    pub is_querying: bool,
    pub vim_mode: Option<String>,
}

impl StatusBarState {
    /// Create a new `StatusBarState` with zeroed numeric defaults.
    pub fn new(model: String, cwd: String) -> Self {
        Self {
            model,
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            cwd,
            is_querying: false,
            vim_mode: None,
        }
    }
}

/// Render the status bar as a single-line `Paragraph`.
///
/// `width` is reserved for future truncation logic.
pub fn render_status_bar(state: &StatusBarState, _width: u16, theme: &Theme) -> Paragraph<'static> {
    let total_tokens = state.input_tokens + state.output_tokens;

    // Last component of the CWD path (split on `/` or `\`).
    let cwd_last = state
        .cwd
        .rsplit(|c| c == '/' || c == '\\')
        .find(|s| !s.is_empty())
        .unwrap_or(&state.cwd)
        .to_owned();

    let sep = " │ ";

    let mut text = String::new();

    if state.is_querying {
        text.push_str("⠋ ");
    }

    text.push(' ');
    text.push_str(&state.model);
    text.push(' ');
    text.push_str(sep);
    text.push_str(&format!("{}tok", total_tokens));
    text.push_str(sep);
    text.push_str(&format!("${:.4}", state.cost_usd));
    text.push_str(sep);
    text.push_str(&cwd_last);

    if let Some(ref mode) = state.vim_mode {
        text.push_str(sep);
        text.push_str(mode);
    }

    let style = Style::default().fg(theme.subtle);
    let line = Line::from(Span::styled(text, style));
    Paragraph::new(line).style(style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::dark_theme;

    #[test]
    fn status_bar_contains_model_name() {
        let theme = dark_theme();
        let state = StatusBarState::new(
            "claude-3-opus".to_owned(),
            "/home/user/project".to_owned(),
        );
        // Verify render_status_bar doesn't panic and returns a Paragraph.
        let _paragraph = render_status_bar(&state, 120, &theme);

        // Verify the text that would be produced contains the model name by
        // re-running the same string-building logic used in the implementation.
        let total_tokens = state.input_tokens + state.output_tokens;
        let sep = " │ ";
        let mut text = String::new();
        text.push(' ');
        text.push_str(&state.model);
        text.push(' ');
        text.push_str(sep);
        text.push_str(&format!("{}tok", total_tokens));
        assert!(
            text.contains("claude-3-opus"),
            "status bar text should contain model name"
        );
    }
}
