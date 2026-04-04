//! Configurable keymap: maps crossterm `KeyEvent`s to semantic `Action`s.
//!
//! Ref: src/keybindings/ (KeybindingContext, useKeybinding hooks)

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ── Action enum ───────────────────────────────────────────────────────────────

/// High-level actions triggered by key presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Submit the current input line.
    Submit,
    /// Send Ctrl+C interruption signal to running query.
    Interrupt,
    /// Exit the TUI.
    Exit,
    /// Scroll the message list up one line.
    ScrollUp,
    /// Scroll the message list down one line.
    ScrollDown,
    /// Scroll the message list up one page.
    PageUp,
    /// Scroll the message list down one page.
    PageDown,
    /// Navigate to the previous history entry.
    HistoryPrev,
    /// Navigate to the next history entry.
    HistoryNext,
    /// Move cursor one character left.
    CursorLeft,
    /// Move cursor one character right.
    CursorRight,
    /// Jump cursor to the beginning of the line.
    CursorHome,
    /// Jump cursor to the end of the line.
    CursorEnd,
    /// Move cursor one word to the left.
    WordLeft,
    /// Move cursor one word to the right.
    WordRight,
    /// Delete the character to the left of the cursor.
    DeleteBack,
    /// Delete the character under the cursor.
    DeleteForward,
    /// Delete from cursor to the beginning of the line.
    DeleteToStart,
    /// Clear the entire input buffer.
    ClearLine,
    /// Insert a literal newline (multi-line mode stub).
    Newline,
}

// ── KeybindingMap ─────────────────────────────────────────────────────────────

/// Maps `KeyEvent` → `Action`.  Cloneable and cheaply shared.
#[derive(Clone)]
pub struct KeybindingMap(HashMap<KeyEventKey, Action>);

/// Hashable wrapper for `KeyEvent` (only code + modifiers matter).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct KeyEventKey {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl From<&KeyEvent> for KeyEventKey {
    fn from(k: &KeyEvent) -> Self {
        Self { code: k.code, modifiers: k.modifiers }
    }
}

impl KeybindingMap {
    /// Build the default keymap.
    pub fn default_map() -> Self {
        let mut m = HashMap::new();

        macro_rules! bind {
            ($code:expr, $mods:expr, $action:expr) => {
                m.insert(
                    KeyEventKey { code: $code, modifiers: $mods },
                    $action,
                );
            };
        }

        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::CONTROL;
        let alt  = KeyModifiers::ALT;

        bind!(KeyCode::Enter,     none,  Action::Submit);
        bind!(KeyCode::Char('m'), ctrl,  Action::Submit);  // Ctrl+M = Enter
        bind!(KeyCode::Char('c'), ctrl,  Action::Interrupt);
        bind!(KeyCode::Char('d'), ctrl,  Action::Exit);
        bind!(KeyCode::Up,        none,  Action::HistoryPrev);
        bind!(KeyCode::Down,      none,  Action::HistoryNext);
        bind!(KeyCode::PageUp,    none,  Action::PageUp);
        bind!(KeyCode::PageDown,  none,  Action::PageDown);
        bind!(KeyCode::Left,      none,  Action::CursorLeft);
        bind!(KeyCode::Right,     none,  Action::CursorRight);
        bind!(KeyCode::Home,      none,  Action::CursorHome);
        bind!(KeyCode::End,       none,  Action::CursorEnd);
        bind!(KeyCode::Char('a'), ctrl,  Action::CursorHome);
        bind!(KeyCode::Char('e'), ctrl,  Action::CursorEnd);
        bind!(KeyCode::Char('b'), ctrl,  Action::CursorLeft);
        bind!(KeyCode::Char('f'), ctrl,  Action::CursorRight);
        bind!(KeyCode::Left,      alt,   Action::WordLeft);
        bind!(KeyCode::Right,     alt,   Action::WordRight);
        bind!(KeyCode::Char('b'), alt,   Action::WordLeft);
        bind!(KeyCode::Char('f'), alt,   Action::WordRight);
        bind!(KeyCode::Backspace, none,  Action::DeleteBack);
        bind!(KeyCode::Char('h'), ctrl,  Action::DeleteBack);
        bind!(KeyCode::Delete,    none,  Action::DeleteForward);
        bind!(KeyCode::Char('d'), alt,   Action::DeleteForward);
        bind!(KeyCode::Char('u'), ctrl,  Action::DeleteToStart);
        bind!(KeyCode::Char('k'), ctrl,  Action::ClearLine);

        Self(m)
    }

    /// Look up the action for a key event, if any.
    pub fn action_for(&self, key: &KeyEvent) -> Option<Action> {
        self.0.get(&KeyEventKey::from(key)).copied()
    }
}

impl std::fmt::Debug for KeybindingMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeybindingMap")
            .field("bindings", &self.0.len())
            .finish()
    }
}
