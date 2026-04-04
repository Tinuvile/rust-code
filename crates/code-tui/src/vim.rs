//! Vim-mode state machine for the input widget.
//!
//! Feature-gated behind `vim_mode`. When enabled, the input box supports
//! a Normal/Insert mode toggle similar to the original TypeScript vim mode.
//!
//! Ref: src/utils/vimModeState.ts

#[cfg(feature = "vim_mode")]
pub use inner::*;

#[cfg(feature = "vim_mode")]
mod inner {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use crate::input::InputState;

    /// Vim editing mode.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum VimMode {
        /// Normal mode — movement and operator commands.
        Normal,
        /// Insert mode — direct character input.
        #[default]
        Insert,
    }

    impl VimMode {
        pub fn label(&self) -> &'static str {
            match self {
                VimMode::Normal => "NOR",
                VimMode::Insert => "INS",
            }
        }
    }

    /// Vim mode state machine.
    #[derive(Debug, Clone, Default)]
    pub struct VimState {
        pub mode: VimMode,
        /// Pending count prefix (e.g. `3` in `3w`).
        count: Option<u32>,
    }

    impl VimState {
        pub fn new() -> Self {
            Self::default()
        }

        /// Handle a key event.
        ///
        /// Returns `true` if the key was consumed (should not be passed to
        /// the default input handler).
        pub fn handle_key(&mut self, key: &KeyEvent, input: &mut InputState) -> bool {
            match self.mode {
                VimMode::Insert => self.handle_insert(key, input),
                VimMode::Normal => self.handle_normal(key, input),
            }
        }

        fn handle_insert(&mut self, key: &KeyEvent, _input: &mut InputState) -> bool {
            if key.code == KeyCode::Esc {
                self.mode = VimMode::Normal;
                return true;
            }
            // All other keys fall through to default input handling.
            false
        }

        fn handle_normal(&mut self, key: &KeyEvent, input: &mut InputState) -> bool {
            let none = KeyModifiers::NONE;
            match (key.code, key.modifiers) {
                // Enter insert mode.
                (KeyCode::Char('i'), m) if m == none => {
                    self.mode = VimMode::Insert;
                    true
                }
                // Append (insert after cursor).
                (KeyCode::Char('a'), m) if m == none => {
                    input.move_right();
                    self.mode = VimMode::Insert;
                    true
                }
                // Append at end of line.
                (KeyCode::Char('A'), m) if m == none => {
                    input.end();
                    self.mode = VimMode::Insert;
                    true
                }
                // Insert at beginning of line.
                (KeyCode::Char('I'), m) if m == none => {
                    input.home();
                    self.mode = VimMode::Insert;
                    true
                }
                // Movement.
                (KeyCode::Char('h'), m) | (KeyCode::Left, m) if m == none => {
                    input.move_left();
                    true
                }
                (KeyCode::Char('l'), m) | (KeyCode::Right, m) if m == none => {
                    input.move_right();
                    true
                }
                (KeyCode::Char('0'), m) | (KeyCode::Home, m) if m == none => {
                    input.home();
                    true
                }
                (KeyCode::Char('$'), m) | (KeyCode::End, m) if m == none => {
                    input.end();
                    true
                }
                (KeyCode::Char('w'), m) if m == none => {
                    input.word_right();
                    true
                }
                (KeyCode::Char('b'), m) if m == none => {
                    input.word_left();
                    true
                }
                // Delete character under cursor.
                (KeyCode::Char('x'), m) if m == none => {
                    input.delete_forward();
                    true
                }
                // Delete to end of line.
                (KeyCode::Char('D'), m) if m == none => {
                    input.clear();
                    true
                }
                // Digit — accumulate count.
                (KeyCode::Char(c), m) if m == none && c.is_ascii_digit() => {
                    let d = c.to_digit(10).unwrap_or(0);
                    self.count = Some(self.count.unwrap_or(0) * 10 + d);
                    true
                }
                _ => false,
            }
        }
    }
}

// When the feature is not enabled, provide a dummy zero-size type so code
// that references VimState compiles without gating every call site.
#[cfg(not(feature = "vim_mode"))]
#[derive(Debug, Clone, Default)]
pub struct VimState;

#[cfg(not(feature = "vim_mode"))]
impl VimState {
    pub fn label(&self) -> &'static str {
        "INS"
    }
}
