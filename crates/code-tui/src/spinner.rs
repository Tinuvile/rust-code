//! Animated spinner widget for tool execution progress.
//!
//! Ref: src/components/Spinner.tsx

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::theme::{Theme, SPINNER_FRAMES};

// ── SpinnerMode ───────────────────────────────────────────────────────────────

/// Current spinner state.
#[derive(Debug, Clone, Default)]
pub enum SpinnerMode {
    #[default]
    Idle,
    /// Model is reasoning before responding.
    Thinking,
    /// A named tool is executing.
    Running(String),
    /// Model is streaming a response.
    Streaming,
}

// ── Spinner ───────────────────────────────────────────────────────────────────

/// State for the animated spinner.
#[derive(Debug, Clone)]
pub struct Spinner {
    /// Current animation frame index.
    pub frame: usize,
    /// Current spinner state.
    pub mode: SpinnerMode,
    /// Milliseconds elapsed since the current mode started.
    pub elapsed_ms: u64,
}

impl Default for Spinner {
    fn default() -> Self {
        Self { frame: 0, mode: SpinnerMode::Idle, elapsed_ms: 0 }
    }
}

impl Spinner {
    /// Advance the animation by one tick (50ms).
    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
        self.elapsed_ms = self.elapsed_ms.saturating_add(50);
    }

    /// Reset the elapsed timer and set a new mode.
    pub fn set_mode(&mut self, mode: SpinnerMode) {
        self.mode = mode;
        self.elapsed_ms = 0;
        self.frame = 0;
    }

    /// Build the one-line label shown next to or below the message list.
    pub fn label(&self) -> String {
        let glyph = SPINNER_FRAMES[self.frame];
        let elapsed = format_elapsed(self.elapsed_ms);
        match &self.mode {
            SpinnerMode::Idle => String::new(),
            SpinnerMode::Thinking => format!("{glyph} Thinking…  ({elapsed})"),
            SpinnerMode::Streaming => format!("{glyph} Responding…  ({elapsed})"),
            SpinnerMode::Running(tool) => {
                let short = if tool.len() > 30 { &tool[..30] } else { tool };
                format!("{glyph} Running {short}…  ({elapsed})")
            }
        }
    }

    /// `true` if the spinner should be shown at all.
    pub fn is_active(&self) -> bool {
        !matches!(self.mode, SpinnerMode::Idle)
    }
}

fn format_elapsed(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

// ── Widget impl ───────────────────────────────────────────────────────────────

/// Renders the spinner label as a single styled line.
pub struct SpinnerWidget<'a> {
    pub spinner: &'a Spinner,
    pub theme: &'a Theme,
}

impl Widget for SpinnerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || !self.spinner.is_active() {
            return;
        }
        let label = self.spinner.label();
        let style = Style::default()
            .fg(self.theme.claude)
            .add_modifier(Modifier::BOLD);
        let line = Line::from(Span::styled(label, style));
        ratatui::widgets::Paragraph::new(line).render(area, buf);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_advances_frame() {
        let mut s = Spinner::default();
        s.set_mode(SpinnerMode::Thinking);
        s.tick();
        assert_eq!(s.frame, 1);
        assert_eq!(s.elapsed_ms, 50);
    }

    #[test]
    fn frame_wraps_around() {
        let mut s = Spinner::default();
        s.set_mode(SpinnerMode::Thinking);
        for _ in 0..SPINNER_FRAMES.len() {
            s.tick();
        }
        assert_eq!(s.frame, 0);
    }

    #[test]
    fn idle_label_is_empty() {
        let s = Spinner::default();
        assert_eq!(s.label(), "");
        assert!(!s.is_active());
    }

    #[test]
    fn running_label_contains_tool_name() {
        let mut s = Spinner::default();
        s.set_mode(SpinnerMode::Running("bash".to_owned()));
        assert!(s.label().contains("bash"));
    }
}
