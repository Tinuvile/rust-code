//! Terminal UI: ratatui-based REPL, streaming display, permission dialogs.
//!
//! Ref: src/ink/ (custom renderer replaced by ratatui),
//!      src/components/ (144 React components → ratatui widgets)

pub mod app;
pub mod event_loop;
pub mod screen;
pub mod repl;
pub mod input;
pub mod markdown;
pub mod diff_view;
pub mod status_bar;
pub mod dialogs;
pub mod keybindings;
pub mod theme;
pub mod spinner;

#[cfg(feature = "vim_mode")]
pub mod vim;
