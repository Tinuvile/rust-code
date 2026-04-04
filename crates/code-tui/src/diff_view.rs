//! Colored unified diff renderer.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme::Theme;

/// Render a unified diff patch string into colored ratatui `Line`s.
///
/// - Lines starting with `+++` / `---` (file headers) → bold
/// - Lines starting with `+` (but not `+++`) → `diff_added` fg
/// - Lines starting with `-` (but not `---`) → `diff_removed` fg
/// - Lines starting with `@@` → Cyan + DIM
/// - All others → default style
///
/// Returns one `Line<'static>` per non-trailing-empty input line.
pub fn render_diff(patch: &str, theme: &Theme) -> Vec<Line<'static>> {
    patch
        .split('\n')
        // skip a single trailing empty line
        .enumerate()
        .filter(|(idx, line)| {
            let total = patch.split('\n').count();
            !(*idx == total - 1 && line.is_empty())
        })
        .map(|(_, raw)| {
            let owned: String = raw.to_owned();
            let style = if owned.starts_with("+++") || owned.starts_with("---") {
                Style::default().add_modifier(Modifier::BOLD)
            } else if owned.starts_with('+') {
                Style::default().fg(theme.diff_added)
            } else if owned.starts_with('-') {
                Style::default().fg(theme.diff_removed)
            } else if owned.starts_with("@@") {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };
            Line::from(Span::styled(owned, style))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::dark_theme;

    #[test]
    fn added_line_uses_diff_added_color() {
        let theme = dark_theme();
        let patch = "+added line";
        let lines = render_diff(patch, &theme);
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(
            span.style.fg,
            Some(theme.diff_added),
            "expected diff_added color for '+' line"
        );
    }

    #[test]
    fn removed_line_uses_diff_removed_color() {
        let theme = dark_theme();
        let patch = "-removed line";
        let lines = render_diff(patch, &theme);
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(
            span.style.fg,
            Some(theme.diff_removed),
            "expected diff_removed color for '-' line"
        );
    }
}
