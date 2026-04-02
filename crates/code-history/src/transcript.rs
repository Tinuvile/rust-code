//! JSONL-based message transcript persistence.
//!
//! Each line of `transcript.jsonl` is a JSON-serialized `Message`.
//! Append-only writes are used for crash safety; a full load reads all lines.
//!
//! Ref: src/utils/sessionStorage.ts (appendMessage, getMessages)

use std::path::{Path, PathBuf};

use anyhow::Context;
use code_types::message::Message;
use tokio::io::AsyncWriteExt;

/// Manages the JSONL transcript file for a session.
#[derive(Debug, Clone)]
pub struct Transcript {
    path: PathBuf,
}

impl Transcript {
    /// Create a transcript handle pointing at `{session_dir}/transcript.jsonl`.
    pub fn new(session_dir: &Path) -> Self {
        Self {
            path: session_dir.join("transcript.jsonl"),
        }
    }

    /// Create a transcript handle with an explicit path.
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns `true` if the transcript file exists on disk.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Return the path to the transcript file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a single message as a JSON line.
    ///
    /// Creates the file if it does not exist.
    pub async fn append(&self, msg: &Message) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut line = serde_json::to_string(msg)
            .with_context(|| "failed to serialize message")?;
        line.push('\n');

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .with_context(|| format!("cannot open transcript at {}", self.path.display()))?;
        file.write_all(line.as_bytes()).await?;
        Ok(())
    }

    /// Append multiple messages at once (single file open).
    pub async fn append_many(&self, msgs: &[Message]) -> anyhow::Result<()> {
        if msgs.is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut buf = String::new();
        for msg in msgs {
            let line = serde_json::to_string(msg)?;
            buf.push_str(&line);
            buf.push('\n');
        }

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(buf.as_bytes()).await?;
        Ok(())
    }

    /// Load all messages from the transcript.
    ///
    /// Lines that fail to parse are skipped with a warning.
    pub async fn load_all(&self) -> anyhow::Result<Vec<Message>> {
        let raw = match tokio::fs::read_to_string(&self.path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };

        let mut messages = Vec::new();
        for (i, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Message>(line) {
                Ok(msg) => messages.push(msg),
                Err(e) => {
                    tracing::warn!("transcript line {i} parse error: {e}");
                }
            }
        }
        Ok(messages)
    }

    /// Overwrite the transcript with a new set of messages (used after compaction).
    pub async fn rewrite(&self, msgs: &[Message]) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut buf = String::new();
        for msg in msgs {
            let line = serde_json::to_string(msg)?;
            buf.push_str(&line);
            buf.push('\n');
        }
        tokio::fs::write(&self.path, buf).await?;
        Ok(())
    }

    /// Delete the transcript file (used on session clear).
    pub async fn clear(&self) -> anyhow::Result<()> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Count the number of messages by counting non-empty lines.
    pub async fn line_count(&self) -> anyhow::Result<usize> {
        let raw = match tokio::fs::read_to_string(&self.path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        Ok(raw.lines().filter(|l| !l.trim().is_empty()).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_types::message::UserMessage;
    use uuid::Uuid;

    #[tokio::test]
    async fn append_and_load_roundtrip() {
        let dir = tempdir();
        let t = Transcript::new(&dir);

        let msg = Message::User(UserMessage {
            uuid: Uuid::new_v4(),
            content: vec![code_types::message::ContentBlock::text("hello")],
            is_api_error_message: false,
            agent_id: None,
        });
        t.append(&msg).await.unwrap();
        let loaded = t.load_all().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].uuid(), msg.uuid());
    }

    #[tokio::test]
    async fn load_nonexistent_returns_empty() {
        let dir = tempdir();
        let t = Transcript::new(&dir);
        let msgs = t.load_all().await.unwrap();
        assert!(msgs.is_empty());
    }

    fn tempdir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "code-history-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
