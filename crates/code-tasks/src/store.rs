//! Task store: in-memory registry with optional JSON persistence.
//!
//! Ref: src/tasks/ (task state management)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::task::{TaskId, TaskRecord};

// ── TaskStore ─────────────────────────────────────────────────────────────────

/// Thread-safe in-memory task registry with optional disk persistence.
#[derive(Debug, Clone, Default)]
pub struct TaskStore {
    inner: Arc<Mutex<StoreInner>>,
}

#[derive(Debug, Default)]
struct StoreInner {
    tasks: HashMap<String, TaskRecord>,
    persist_dir: Option<PathBuf>,
}

impl TaskStore {
    /// Create a store that persists to `dir/<task_id>.json`.
    pub fn with_persistence(dir: PathBuf) -> Self {
        let store = Self::default();
        store.inner.lock().unwrap().persist_dir = Some(dir);
        store
    }

    /// Register a new task. Returns the task id.
    pub fn insert(&self, record: TaskRecord) -> TaskId {
        let id = record.id.clone();
        let mut inner = self.inner.lock().unwrap();
        inner.tasks.insert(id.to_string(), record);
        id
    }

    /// Update a task record in place.
    pub fn update<F>(&self, id: &TaskId, f: F) -> bool
    where
        F: FnOnce(&mut TaskRecord),
    {
        let mut inner = self.inner.lock().unwrap();
        if let Some(rec) = inner.tasks.get_mut(id.as_str()) {
            f(rec);
            true
        } else {
            false
        }
    }

    /// Get a snapshot of a task record.
    pub fn get(&self, id: &TaskId) -> Option<TaskRecord> {
        self.inner.lock().unwrap().tasks.get(id.as_str()).cloned()
    }

    /// All task records, sorted by creation time (newest last).
    pub fn all(&self) -> Vec<TaskRecord> {
        let inner = self.inner.lock().unwrap();
        let mut tasks: Vec<_> = inner.tasks.values().cloned().collect();
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }

    /// Active tasks only (Pending or Running).
    pub fn active(&self) -> Vec<TaskRecord> {
        self.all()
            .into_iter()
            .filter(|t| t.status.is_active())
            .collect()
    }

    /// Remove a task record.
    pub fn remove(&self, id: &TaskId) -> Option<TaskRecord> {
        self.inner.lock().unwrap().tasks.remove(id.as_str())
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    /// Persist a single task to disk (if a persistence dir is configured).
    pub async fn persist(&self, id: &TaskId) -> Result<()> {
        let (record, dir) = {
            let inner = self.inner.lock().unwrap();
            let record = inner.tasks.get(id.as_str()).cloned();
            let dir = inner.persist_dir.clone();
            (record, dir)
        };
        if let (Some(record), Some(dir)) = (record, dir) {
            tokio::fs::create_dir_all(&dir).await?;
            let path = task_path(&dir, id);
            let json = serde_json::to_string_pretty(&record)?;
            tokio::fs::write(path, json).await?;
        }
        Ok(())
    }

    /// Load all persisted tasks from disk into the store.
    pub async fn load_from_dir(dir: &Path) -> Result<Self> {
        let store = Self::with_persistence(dir.to_path_buf());
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return Ok(store),
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(json) = tokio::fs::read_to_string(&path).await {
                    if let Ok(record) = serde_json::from_str::<TaskRecord>(&json) {
                        store.insert(record);
                    }
                }
            }
        }
        Ok(store)
    }
}

fn task_path(dir: &Path, id: &TaskId) -> PathBuf {
    dir.join(format!("{id}.json"))
}

// ── Shared handle ─────────────────────────────────────────────────────────────

/// A cheaply cloneable shared task store.
pub type SharedTaskStore = TaskStore;
