//! Screen stack for TUI navigation (REPL, Help, Resume).

/// The named screens in the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    /// The main REPL/chat screen.
    Repl,
    /// In-app help screen.
    Help,
    /// Session resume / history browser screen.
    Resume,
}

/// A push-down stack of `Screen` values.
///
/// The bottom of the stack is always `Screen::Repl`; `pop` will never
/// remove the last screen.
#[derive(Debug, Clone)]
pub struct ScreenStack {
    stack: Vec<Screen>,
}

impl ScreenStack {
    /// Create a new `ScreenStack` initialised with `Screen::Repl`.
    pub fn new() -> Self {
        Self {
            stack: vec![Screen::Repl],
        }
    }

    /// Return a reference to the current (top) screen.
    pub fn current(&self) -> &Screen {
        // Stack is never empty; initialised with at least one entry and pop
        // prevents dropping the last one.
        self.stack.last().expect("ScreenStack must never be empty")
    }

    /// Push a new screen onto the stack.
    pub fn push(&mut self, screen: Screen) {
        self.stack.push(screen);
    }

    /// Pop the top screen, unless it is the only remaining screen.
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
}

impl Default for ScreenStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop_returns_to_repl() {
        let mut stack = ScreenStack::new();
        assert_eq!(stack.current(), &Screen::Repl);

        stack.push(Screen::Help);
        assert_eq!(stack.current(), &Screen::Help);

        stack.pop();
        assert_eq!(stack.current(), &Screen::Repl);

        // Popping the last screen should be a no-op.
        stack.pop();
        assert_eq!(stack.current(), &Screen::Repl);
    }
}
