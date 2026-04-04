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
#[cfg(not(feature = "vim_mode"))]
pub mod vim;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use app::App;
pub use event_loop::run;
pub use theme::{dark_ansi_theme, dark_theme, light_theme, Theme};

// ── Entry point ───────────────────────────────────────────────────────────────

use std::sync::Arc;

use code_commands::CommandRegistry;
use code_config::global::{GlobalConfig, ThemeSetting};
use code_config::settings::SettingsJson;
use code_query::engine::QueryEngine;
use code_types::ids::SessionId;

/// Start the interactive TUI session.
///
/// Called from `code-cli` after the `QueryEngine` has been initialised.
pub async fn start(
    engine: Arc<QueryEngine>,
    config: Arc<GlobalConfig>,
    settings: Arc<SettingsJson>,
    registry: CommandRegistry,
    session_id: SessionId,
) -> anyhow::Result<()> {
    let theme = match config.theme {
        ThemeSetting::Light => light_theme(),
        _ => dark_theme(),
    };
    let message_rx = engine.subscribe();
    let app = App::new(engine, message_rx, registry, config, settings, theme, session_id);
    event_loop::run(app).await
}
