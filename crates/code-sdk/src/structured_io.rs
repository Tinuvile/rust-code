//! Structured JSON streaming I/O for the SDK.
//!
//! The SDK communicates with external SDK consumers via newline-delimited JSON
//! (NDJSON) over stdin/stdout.  This module provides the reader and writer halves.
//!
//! Ref: src/cli/ (JSON streaming input/output)

use std::io::{self, BufRead, Write};

use anyhow::Result;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::message::SdkMessage;

// ── SdkInput ──────────────────────────────────────────────────────────────────

/// An inbound SDK message from an external process via stdin.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SdkInput {
    /// A user prompt to submit.
    UserMessage { content: String },
    /// Interrupt the current query.
    Interrupt,
    /// Terminate the session.
    Exit,
}

// ── Async writer ──────────────────────────────────────────────────────────────

/// Write SDK messages to stdout as NDJSON.
pub struct SdkWriter<W: AsyncWriteExt + Unpin> {
    inner: W,
}

impl<W: AsyncWriteExt + Unpin> SdkWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    /// Serialize `msg` as one JSON line and flush.
    pub async fn write(&mut self, msg: &SdkMessage) -> Result<()> {
        let line = serde_json::to_string(msg)?;
        self.inner.write_all(line.as_bytes()).await?;
        self.inner.write_all(b"\n").await?;
        self.inner.flush().await?;
        Ok(())
    }
}

// ── Async reader ──────────────────────────────────────────────────────────────

/// Read SDK input messages from stdin as NDJSON.
pub struct SdkReader<R: AsyncBufReadExt + Unpin> {
    inner: R,
    buf: String,
}

impl<R: AsyncBufReadExt + Unpin> SdkReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, buf: String::new() }
    }

    /// Read the next message. Returns `None` on EOF.
    pub async fn next(&mut self) -> Result<Option<SdkInput>> {
        self.buf.clear();
        let n = self.inner.read_line(&mut self.buf).await?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = self.buf.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let msg = serde_json::from_str::<SdkInput>(trimmed)?;
        Ok(Some(msg))
    }
}

// ── Synchronous helpers for non-async contexts ────────────────────────────────

/// Write a single SDK message to stdout synchronously (non-async contexts).
pub fn write_sdk_message_sync(msg: &SdkMessage) -> Result<()> {
    let line = serde_json::to_string(msg)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

/// Read one SDK input message from stdin synchronously.
pub fn read_sdk_input_sync() -> Result<Option<SdkInput>> {
    let stdin = io::stdin();
    let mut line = String::new();
    let n = stdin.lock().read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(trimmed)?))
}

// ── Convenience: drain an SDK message receiver to NDJSON stdout ───────────────

/// Print a single `SdkMessage` as a JSON line to stdout.
pub fn emit(msg: &SdkMessage) {
    if let Ok(line) = serde_json::to_string(msg) {
        println!("{line}");
    }
}
