//! Input state management and rendering for the REPL prompt.

use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::theme::Theme;

/// Editable single-line input with cursor, history, and slash-command detection.
#[derive(Debug, Clone)]
pub struct InputState {
    pub buffer: String,
    /// Byte offset into `buffer` where the cursor sits.
    pub cursor: usize,
    pub history: Vec<String>,
    /// Index into `history` when navigating (None = current live buffer).
    pub history_idx: Option<usize>,
    /// Saved live buffer while navigating history.
    saved_buffer: String,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_idx: None,
            saved_buffer: String::new(),
        }
    }

    /// Insert a character at the current cursor position and advance the cursor.
    pub fn insert(&mut self, ch: char) {
        self.buffer.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.history_idx = None;
    }

    /// Delete the character immediately before the cursor (handles multi-byte chars).
    pub fn delete_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // find start of the previous char
        let mut prev = self.cursor - 1;
        while !self.buffer.is_char_boundary(prev) {
            prev -= 1;
        }
        self.buffer.drain(prev..self.cursor);
        self.cursor = prev;
        self.history_idx = None;
    }

    /// Delete the character at the cursor position.
    pub fn delete_forward(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }
        let mut next = self.cursor + 1;
        while !self.buffer.is_char_boundary(next) {
            next += 1;
        }
        self.buffer.drain(self.cursor..next);
        self.history_idx = None;
    }

    /// Move the cursor one character to the left.
    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut pos = self.cursor - 1;
        while !self.buffer.is_char_boundary(pos) {
            pos -= 1;
        }
        self.cursor = pos;
    }

    /// Move the cursor one character to the right.
    pub fn move_right(&mut self) {
        if self.cursor >= self.buffer.len() {
            return;
        }
        let mut pos = self.cursor + 1;
        while !self.buffer.is_char_boundary(pos) {
            pos += 1;
        }
        self.cursor = pos;
    }

    /// Move the cursor one word to the left (skip whitespace then word chars).
    pub fn word_left(&mut self) {
        let bytes = self.buffer.as_bytes();
        let mut pos = self.cursor;
        // skip whitespace before cursor
        while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
            pos -= 1;
        }
        // skip word chars
        while pos > 0 && !bytes[pos - 1].is_ascii_whitespace() {
            pos -= 1;
        }
        self.cursor = pos;
    }

    /// Move the cursor one word to the right (skip word chars then whitespace).
    pub fn word_right(&mut self) {
        let bytes = self.buffer.as_bytes();
        let len = bytes.len();
        let mut pos = self.cursor;
        // skip word chars
        while pos < len && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // skip whitespace
        while pos < len && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        self.cursor = pos;
    }

    /// Move cursor to the beginning of the buffer.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the buffer.
    pub fn end(&mut self) {
        self.cursor = self.buffer.len();
    }

    /// Navigate to the previous (older) history entry.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_idx {
            None => {
                // save current live buffer
                self.saved_buffer = self.buffer.clone();
                let idx = self.history.len() - 1;
                self.history_idx = Some(idx);
                self.buffer = self.history[idx].clone();
            }
            Some(0) => {
                // already at oldest — stay
            }
            Some(idx) => {
                let new_idx = idx - 1;
                self.history_idx = Some(new_idx);
                self.buffer = self.history[new_idx].clone();
            }
        }
        self.cursor = self.buffer.len();
    }

    /// Navigate to the next (newer) history entry, or restore live buffer.
    pub fn history_next(&mut self) {
        match self.history_idx {
            None => {
                // already at live buffer; nothing to do
            }
            Some(idx) => {
                if idx + 1 >= self.history.len() {
                    // past end — restore live buffer
                    self.buffer = self.saved_buffer.clone();
                    self.history_idx = None;
                } else {
                    let new_idx = idx + 1;
                    self.history_idx = Some(new_idx);
                    self.buffer = self.history[new_idx].clone();
                }
            }
        }
        self.cursor = self.buffer.len();
    }

    /// Submit the current buffer: push to history if non-empty, reset state, return text.
    pub fn submit(&mut self) -> String {
        let text = std::mem::take(&mut self.buffer);
        if !text.is_empty() {
            self.history.push(text.clone());
        }
        self.cursor = 0;
        self.history_idx = None;
        self.saved_buffer.clear();
        text
    }

    /// Returns `true` when the buffer starts with `/`.
    pub fn is_slash_command(&self) -> bool {
        self.buffer.starts_with('/')
    }

    /// Clear the buffer and reset the cursor.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.history_idx = None;
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Render the input box into `buf` at the given `area`.
///
/// Displays the buffer with a reverse-video cursor indicator.
pub fn render_input(input: &InputState, area: Rect, buf: &mut Buffer, theme: &Theme) {
    let block = Block::bordered()
        .border_style(Style::default().fg(theme.prompt_border))
        .title(" > ");

    let inner = block.inner(area);
    block.render(area, buf);

    let before: String = input.buffer[..input.cursor].to_owned();
    let cursor_char: String = if input.cursor < input.buffer.len() {
        // Extract the char at cursor
        let ch = input.buffer[input.cursor..]
            .chars()
            .next()
            .unwrap_or(' ');
        ch.to_string()
    } else {
        " ".to_owned()
    };
    let after: String = if input.cursor < input.buffer.len() {
        let char_len = input.buffer[input.cursor..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        input.buffer[input.cursor + char_len..].to_owned()
    } else {
        String::new()
    };

    let line = Line::from(vec![
        Span::raw(before),
        Span::styled(cursor_char, Style::default().add_modifier(Modifier::REVERSED)),
        Span::raw(after),
    ]);

    Paragraph::new(line).render(inner, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_advances_cursor() {
        let mut s = InputState::new();
        s.insert('h');
        s.insert('i');
        assert_eq!(s.buffer, "hi");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn delete_back_removes_previous_char() {
        let mut s = InputState::new();
        s.insert('a');
        s.insert('b');
        s.insert('c');
        s.delete_back();
        assert_eq!(s.buffer, "ab");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn submit_clears_buffer_and_pushes_history() {
        let mut s = InputState::new();
        s.insert('x');
        s.insert('y');
        let result = s.submit();
        assert_eq!(result, "xy");
        assert_eq!(s.buffer, "");
        assert_eq!(s.cursor, 0);
        assert_eq!(s.history, vec!["xy"]);
    }

    #[test]
    fn is_slash_command_detects_slash_prefix() {
        let mut s = InputState::new();
        assert!(!s.is_slash_command());
        s.insert('/');
        s.insert('h');
        s.insert('e');
        s.insert('l');
        s.insert('p');
        assert!(s.is_slash_command());
    }
}
