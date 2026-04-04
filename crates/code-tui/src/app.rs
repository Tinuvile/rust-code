//! Top-level application state.
//!
//! `App` owns all runtime state: message list, input, spinner, dialogs,
//! query engine handle, command registry, and theme.
//!
//! Ref: src/screens/REPL.tsx (main state management)

use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use code_commands::{CommandContext, CommandOutput, CommandRegistry};
use code_config::global::GlobalConfig;
use code_config::settings::SettingsJson;
use code_query::engine::QueryEngine;
use code_query::message_queue::MessageReceiver;
use code_types::ids::SessionId;
use code_types::message::{
    ContentBlock, Message, SystemInformationalMessage, SystemMessageLevel, TextBlock, UserMessage,
};
use ratatui::widgets::ListState;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::dialogs::{Dialog, DialogResult};
use crate::input::InputState;
use crate::keybindings::{Action, KeybindingMap};
use crate::screen::ScreenStack;
use crate::spinner::{Spinner, SpinnerMode};
use crate::status_bar::StatusBarState;
use crate::theme::Theme;
use crate::vim::VimState;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Split `/command args` into `("command", "args")`.
fn parse_slash(raw: &str) -> (&str, &str) {
    let stripped = raw.trim_start_matches('/');
    if let Some(space) = stripped.find(' ') {
        (&stripped[..space], stripped[space + 1..].trim_start())
    } else {
        (stripped, "")
    }
}

// ── App ───────────────────────────────────────────────────────────────────────

/// All runtime state for the TUI session.
pub struct App {
    /// All messages visible in the message list (UI + API messages).
    pub messages: Vec<Message>,
    /// API-eligible conversation messages (sent to QueryEngine).
    pub conversation: Vec<Message>,
    /// Text input widget state.
    pub input: InputState,
    /// Spinner state.
    pub spinner: Spinner,
    /// Screen navigation stack.
    pub screens: ScreenStack,
    /// Active dialog (permission request, cost warning, etc.).
    pub dialog: Dialog,
    /// The query engine (shared with spawned tasks).
    pub engine: Arc<QueryEngine>,
    /// Receiver for messages published by the engine.
    pub message_rx: MessageReceiver,
    /// Slash command registry.
    pub registry: CommandRegistry,
    /// User-level global config.
    pub config: Arc<GlobalConfig>,
    /// Merged settings.
    pub settings: Arc<SettingsJson>,
    /// Current color theme.
    pub theme: Theme,
    /// Keymap.
    pub keymap: KeybindingMap,
    /// Whether the event loop should terminate.
    pub should_exit: bool,
    /// Whether a query is currently running.
    pub is_querying: bool,
    /// Scroll state for the message list.
    pub list_state: ListState,
    /// Status bar state.
    pub status: StatusBarState,
    /// Session identifier.
    pub session_id: SessionId,
    /// Vim mode (always present; no-op when `vim_mode` feature is off).
    pub vim: VimState,
}

impl App {
    /// Create a new `App`.
    pub fn new(
        engine: Arc<QueryEngine>,
        message_rx: MessageReceiver,
        registry: CommandRegistry,
        config: Arc<GlobalConfig>,
        settings: Arc<SettingsJson>,
        theme: Theme,
        session_id: SessionId,
    ) -> Self {
        let model = settings
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-6".to_owned());
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let status = StatusBarState::new(model, cwd);

        Self {
            messages: Vec::new(),
            conversation: Vec::new(),
            input: InputState::new(),
            spinner: Spinner::default(),
            screens: ScreenStack::new(),
            dialog: Dialog::None,
            engine,
            message_rx,
            registry,
            config,
            settings,
            theme,
            keymap: KeybindingMap::default_map(),
            should_exit: false,
            is_querying: false,
            list_state: ListState::default(),
            status,
            session_id,
            vim: VimState::default(),
        }
    }

    // ── Message list ──────────────────────────────────────────────────────────

    /// Push a message received from the engine into the display list.
    pub fn push_message(&mut self, msg: Message) {
        // Update status bar token counts from AssistantMessage usage.
        if let Message::Assistant(ref a) = msg {
            self.status.input_tokens += a.usage.input_tokens as u64;
            self.status.output_tokens += a.usage.output_tokens as u64;
            self.status.cost_usd = self.engine.total_cost_usd();
        }
        // When a turn finishes, turn off the spinner.
        if let Message::SystemTurnDuration(_) = msg {
            self.is_querying = false;
            self.spinner.set_mode(SpinnerMode::Idle);
            self.status.is_querying = false;
        }
        self.messages.push(msg);
        self.scroll_to_bottom();
    }

    /// Append an informational system message.
    pub fn push_info(&mut self, text: impl Into<String>, level: SystemMessageLevel) {
        self.messages.push(Message::SystemInformational(SystemInformationalMessage {
            uuid: Uuid::new_v4(),
            content: text.into(),
            level,
        }));
        self.scroll_to_bottom();
    }

    /// Auto-scroll the list to the last item.
    pub fn scroll_to_bottom(&mut self) {
        if !self.messages.is_empty() {
            self.list_state.select(Some(self.messages.len() - 1));
        }
    }

    // ── Input submission ──────────────────────────────────────────────────────

    /// Submit the current input buffer.
    ///
    /// Slash commands are routed to the command registry; everything else is
    /// sent to the query engine.
    pub async fn submit_input(&mut self) -> anyhow::Result<()> {
        let text = self.input.submit();
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }

        if text.starts_with('/') {
            self.dispatch_command(&text).await?;
        } else {
            self.send_to_engine(text).await?;
        }
        Ok(())
    }

    async fn dispatch_command(&mut self, raw: &str) -> anyhow::Result<()> {
        let (name, args) = parse_slash(raw);

        // Look up via `all()` to get an owned `Arc<dyn Command>` without
        // keeping a borrow of `self.registry` across the await below.
        let cmd_arc = self
            .registry
            .all()
            .iter()
            .find(|c| c.name() == name || c.aliases().contains(&name))
            .cloned();

        let Some(cmd) = cmd_arc else {
            self.push_info(
                format!("Unknown command: /{name}. Type /help for a list."),
                SystemMessageLevel::Warning,
            );
            return Ok(());
        };

        let conversation_lock = Arc::new(RwLock::new(std::mem::take(&mut self.conversation)));

        let mut ctx = CommandContext::new(
            self.session_id.clone(),
            std::env::current_dir().unwrap_or_default(),
            Arc::clone(&self.config),
            Arc::clone(&self.settings),
            vec![],
            Arc::clone(&conversation_lock),
            true,
        );

        let output = cmd.execute(args, &mut ctx).await;

        // Put the conversation back.
        self.conversation = match Arc::try_unwrap(conversation_lock) {
            Ok(lock) => lock.into_inner(),
            Err(arc) => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { arc.read().await.clone() })
            }),
        };

        match output {
            Ok(CommandOutput::Text(t)) => {
                self.push_info(t, SystemMessageLevel::Info);
            }
            Ok(CommandOutput::Markdown(md)) => {
                self.push_info(md, SystemMessageLevel::Info);
            }
            Ok(CommandOutput::Exit) => {
                self.should_exit = true;
            }
            Ok(CommandOutput::Query(q)) => {
                self.send_to_engine(q).await?;
            }
            Ok(CommandOutput::Compact { custom_instruction: _ }) => {
                self.push_info(
                    "Context compaction is not yet wired to the TUI.".to_owned(),
                    SystemMessageLevel::Warning,
                );
            }
            Ok(CommandOutput::None) => {}
            Err(e) => {
                self.push_info(format!("Command error: {e}"), SystemMessageLevel::Error);
            }
        }

        Ok(())
    }

    async fn send_to_engine(&mut self, text: String) -> anyhow::Result<()> {
        let user_msg = UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![ContentBlock::Text(TextBlock { text: text.clone(), cache_control: None })],
            is_api_error_message: false,
            agent_id: None,
        };

        // Show user message immediately in the list.
        self.messages.push(Message::User(user_msg.clone()));
        self.scroll_to_bottom();

        // Start spinner.
        self.is_querying = true;
        self.spinner.set_mode(SpinnerMode::Thinking);
        self.status.is_querying = true;

        // Clone engine handle; spawn query in background task.
        let engine = Arc::clone(&self.engine);
        let mut conversation = self.conversation.clone();

        // Run the query on the LocalSet so the !Send future is allowed.
        // Messages are delivered via the broadcast channel.
        tokio::task::spawn_local(async move {
            let _ = engine.query(user_msg, &mut conversation).await;
        });

        Ok(())
    }

    // ── Event handling ────────────────────────────────────────────────────────

    /// Handle a crossterm `Event`.
    pub async fn handle_event(&mut self, event: Event) -> anyhow::Result<()> {
        match event {
            Event::Key(key) => self.handle_key(key).await?,
            Event::Resize(_, _) => {} // ratatui handles this via terminal.draw
            _ => {}
        }
        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        // Dialog intercepts all keys first.
        if self.dialog.is_active() {
            if let Some(result) = self.dialog.handle_key(&key) {
                match result {
                    DialogResult::Allow | DialogResult::Deny | DialogResult::Dismiss => {
                        self.dialog = Dialog::None;
                    }
                }
            }
            return Ok(());
        }

        // Vim mode intercepts keys in Normal mode.
        #[cfg(feature = "vim_mode")]
        if self.vim.handle_key(&key, &mut self.input) {
            return Ok(());
        }

        // Look up action in keymap.
        if let Some(action) = self.keymap.action_for(&key) {
            match action {
                Action::Submit => {
                    self.submit_input().await?;
                }
                Action::Interrupt => {
                    if self.is_querying {
                        self.engine.interruption_signal().set();
                    } else {
                        // Ctrl+C on idle clears the input line.
                        self.input.clear();
                    }
                }
                Action::Exit => {
                    self.should_exit = true;
                }
                Action::ScrollUp => {
                    self.scroll_up(1);
                }
                Action::ScrollDown => {
                    self.scroll_down(1);
                }
                Action::PageUp => {
                    self.scroll_up(10);
                }
                Action::PageDown => {
                    self.scroll_down(10);
                }
                Action::HistoryPrev => self.input.history_prev(),
                Action::HistoryNext => self.input.history_next(),
                Action::CursorLeft => self.input.move_left(),
                Action::CursorRight => self.input.move_right(),
                Action::CursorHome => self.input.home(),
                Action::CursorEnd => self.input.end(),
                Action::WordLeft => self.input.word_left(),
                Action::WordRight => self.input.word_right(),
                Action::DeleteBack => self.input.delete_back(),
                Action::DeleteForward => self.input.delete_forward(),
                Action::DeleteToStart | Action::ClearLine => self.input.clear(),
                Action::Newline => self.input.insert('\n'),
            }
            return Ok(());
        }

        // Default: printable character → insert into buffer.
        if let KeyCode::Char(c) = key.code {
            if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT {
                self.input.insert(c);
            }
        }

        Ok(())
    }

    fn scroll_up(&mut self, n: usize) {
        let selected = self.list_state.selected().unwrap_or(0);
        let new = selected.saturating_sub(n);
        self.list_state.select(Some(new));
    }

    fn scroll_down(&mut self, n: usize) {
        if self.messages.is_empty() {
            return;
        }
        let max = self.messages.len() - 1;
        let selected = self.list_state.selected().unwrap_or(max);
        let new = (selected + n).min(max);
        self.list_state.select(Some(new));
    }
}
