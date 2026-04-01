//! File-change watcher for config files.
//!
//! Monitors `~/.claude/config.json`, `settings.json`, `settings.local.json`,
//! and `code.md` for changes and sends a notification over a channel so the
//! rest of the application can reload.
//!
//! Ref: src/utils/settings/changeDetector.ts

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

// ── Event types ───────────────────────────────────────────────────────────────

/// Which config file changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigChangeKind {
    GlobalConfig,
    UserSettings,
    ProjectSettings,
    LocalSettings,
    CodeMd,
    Other(PathBuf),
}

/// Notification emitted when a watched file changes.
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub kind: ConfigChangeKind,
    pub path: PathBuf,
}

// ── Watcher ───────────────────────────────────────────────────────────────────

/// A running config-file watcher.
///
/// Dropping this value stops the watcher.
pub struct ConfigWatcher {
    /// The underlying `notify` watcher — kept alive by holding it here.
    _watcher: RecommendedWatcher,
}

/// Start watching config files and return a receiver for change events.
///
/// `paths` is a list of `(path, kind)` pairs. Each path is watched
/// non-recursively for modification events.
///
/// The watcher debounces events with a short delay to avoid double-fires.
pub fn start_config_watcher(
    paths: Vec<(PathBuf, ConfigChangeKind)>,
) -> anyhow::Result<(ConfigWatcher, mpsc::UnboundedReceiver<ConfigChangeEvent>)> {
    let (tx, rx) = mpsc::unbounded_channel::<ConfigChangeEvent>();

    // Clone path→kind mapping for the closure.
    let path_map: Vec<(PathBuf, ConfigChangeKind)> = paths.clone();
    let tx = Arc::new(tx);

    let tx_clone = Arc::clone(&tx);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if !matches!(
            event.kind,
            notify::EventKind::Modify(_) | notify::EventKind::Create(_)
        ) {
            return;
        }
        for changed_path in &event.paths {
            // Find the kind for this path.
            let kind = path_map
                .iter()
                .find(|(p, _)| p == changed_path)
                .map(|(_, k)| k.clone())
                .unwrap_or_else(|| ConfigChangeKind::Other(changed_path.clone()));

            let _ = tx_clone.send(ConfigChangeEvent {
                kind,
                path: changed_path.clone(),
            });
        }
    })?;

    // Register each path.
    for (path, _) in &paths {
        if path.exists() {
            // Watch the parent directory non-recursively so we catch atomic
            // replace-via-rename writes (which is how many editors save files).
            if let Some(parent) = path.parent() {
                let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
            }
        }
    }

    Ok((ConfigWatcher { _watcher: watcher }, rx))
}

/// Build the default set of watched paths for a session.
///
/// Ref: src/utils/settings/changeDetector.ts
pub fn default_watch_paths(project_root: Option<&std::path::Path>) -> Vec<(PathBuf, ConfigChangeKind)> {
    let mut paths = Vec::new();

    // Global config.json
    if let Some(home) = dirs_next::home_dir() {
        let claude_dir = home.join(".claude");
        paths.push((claude_dir.join("config.json"), ConfigChangeKind::GlobalConfig));
        paths.push((claude_dir.join("settings.json"), ConfigChangeKind::UserSettings));
        // Global code.md
        let code_md = if claude_dir.join("code.md").exists() {
            claude_dir.join("code.md")
        } else {
            claude_dir.join("claude.md")
        };
        paths.push((code_md, ConfigChangeKind::CodeMd));
    }

    // Project-level settings
    if let Some(root) = project_root {
        paths.push((
            root.join(".claude").join("settings.json"),
            ConfigChangeKind::ProjectSettings,
        ));
        paths.push((
            root.join(".claude").join("settings.local.json"),
            ConfigChangeKind::LocalSettings,
        ));
        // Project code.md
        let code_md = if root.join("code.md").exists() {
            root.join("code.md")
        } else {
            root.join("claude.md")
        };
        paths.push((code_md, ConfigChangeKind::CodeMd));
    }

    paths
}

/// Convenience: debounce a stream of change events.
///
/// Returns a new receiver that emits at most one event per `window` per path.
pub fn debounce_changes(
    mut rx: mpsc::UnboundedReceiver<ConfigChangeEvent>,
    window: Duration,
) -> mpsc::UnboundedReceiver<ConfigChangeEvent> {
    let (out_tx, out_rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        use std::collections::HashMap;
        let mut pending: HashMap<PathBuf, ConfigChangeEvent> = HashMap::new();
        let mut interval = tokio::time::interval(window);

        loop {
            tokio::select! {
                Some(ev) = rx.recv() => {
                    pending.insert(ev.path.clone(), ev);
                }
                _ = interval.tick() => {
                    for (_, ev) in pending.drain() {
                        let _ = out_tx.send(ev);
                    }
                }
            }
        }
    });

    out_rx
}
