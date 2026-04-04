//! Todo list management: JSON file I/O for the TodoWrite/TodoRead tool pair.
//!
//! Ref: src/tools/TodoWriteTool/

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── TodoItem ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TodoStatus {
    #[default]
    Pending,
    #[serde(rename = "in_progress")]
    InProgress,
    Completed,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub priority: TodoPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TodoPriority {
    High,
    #[default]
    Medium,
    Low,
}

impl TodoItem {
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Medium,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.status == TodoStatus::Completed
    }
}

// ── TodoList ──────────────────────────────────────────────────────────────────

/// The todo list for a session, persisted as `<session_dir>/todos.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TodoList {
    pub items: Vec<TodoItem>,
}

impl TodoList {
    /// Load from `path`, returning an empty list if the file does not exist.
    pub async fn load(path: &Path) -> Result<Self> {
        match tokio::fs::read_to_string(path).await {
            Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Save to `path` (creates parent directories as needed).
    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Replace the entire list and save.
    pub async fn write(&mut self, items: Vec<TodoItem>, path: &Path) -> Result<()> {
        self.items = items;
        self.save(path).await
    }

    pub fn pending(&self) -> Vec<&TodoItem> {
        self.items
            .iter()
            .filter(|i| i.status == TodoStatus::Pending)
            .collect()
    }

    pub fn in_progress(&self) -> Vec<&TodoItem> {
        self.items
            .iter()
            .filter(|i| i.status == TodoStatus::InProgress)
            .collect()
    }

    pub fn completed(&self) -> Vec<&TodoItem> {
        self.items
            .iter()
            .filter(|i| i.status == TodoStatus::Completed)
            .collect()
    }
}

// ── Default path ──────────────────────────────────────────────────────────────

/// Returns `<session_dir>/todos.json`.
pub fn todos_path(session_dir: &Path) -> PathBuf {
    session_dir.join("todos.json")
}
