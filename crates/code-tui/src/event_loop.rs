//! crossterm event loop + ratatui Terminal setup/teardown.
//!
//! Ref: src/ink/ink.tsx (main render loop)

use std::io;
use std::time::Duration;

use crossterm::event::EventStream;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::repl::render_repl;

/// Run the TUI event loop until `app.should_exit` is true.
///
/// Sets up raw mode and the alternate screen, then drives a `tokio::select!`
/// loop combining:
/// - crossterm keyboard/resize events
/// - messages from the query engine broadcast channel
/// - a 50ms tick for spinner animation
///
/// Tears down the terminal cleanly on exit or error.
///
/// The entire loop runs inside a `LocalSet` so that `spawn_local` can be used
/// for `QueryEngine::query()` which is `!Send` due to internal `MutexGuard`s.
pub async fn run(app: App) -> anyhow::Result<()> {
    let local = tokio::task::LocalSet::new();
    local.run_until(run_inner(app)).await
}

async fn run_inner(mut app: App) -> anyhow::Result<()> {
    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = event_loop(&mut app, &mut terminal).await;

    // ── Terminal teardown (always runs) ─────────────────────────────────��─────
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn event_loop(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> anyhow::Result<()> {
    let mut event_stream = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        // Render current state.
        terminal.draw(|frame| render_repl(app, frame))?;

        tokio::select! {
            // Keyboard / resize event.
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        app.handle_event(event).await?;
                    }
                    Some(Err(e)) => {
                        tracing::warn!("crossterm event error: {e}");
                    }
                    None => break, // Stream closed.
                }
            }

            // Message from the query engine.
            maybe_msg = app.message_rx.recv() => {
                if let Some(msg) = maybe_msg {
                    app.push_message(msg);
                }
                // If recv returns None the engine was dropped — keep running.
            }

            // Animation tick (50ms).
            _ = tick.tick() => {
                app.spinner.tick();
            }
        }

        if app.should_exit {
            break;
        }
    }

    Ok(())
}
