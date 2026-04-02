//! Message broadcast queue.
//!
//! Provides a fanout channel for `Message` values.  Multiple consumers
//! (TUI renderer, transcript writer, hooks) can subscribe independently.
//!
//! Uses `tokio::sync::broadcast` under the hood.
//!
//! Ref: src/utils/messageQueue.ts

use std::sync::Arc;

use tokio::sync::broadcast;

use code_types::message::Message;

/// Broadcast channel capacity.  Old messages are dropped if subscribers fall behind.
const CHANNEL_CAPACITY: usize = 256;

/// Sender side of the message queue.
#[derive(Clone)]
pub struct MessageSender(Arc<broadcast::Sender<Message>>);

impl MessageSender {
    /// Publish a message to all subscribers.
    ///
    /// Returns the number of active subscribers that received the message.
    pub fn publish(&self, msg: Message) -> usize {
        self.0.send(msg).unwrap_or(0)
    }

    /// Create a new subscriber.
    pub fn subscribe(&self) -> MessageReceiver {
        MessageReceiver(self.0.subscribe())
    }
}

/// Receiver side — one per subscriber.
pub struct MessageReceiver(broadcast::Receiver<Message>);

impl MessageReceiver {
    /// Receive the next message, waiting asynchronously.
    ///
    /// Returns `None` if the sender has been dropped and no more messages will arrive.
    pub async fn recv(&mut self) -> Option<Message> {
        loop {
            match self.0.recv().await {
                Ok(msg) => return Some(msg),
                // Lagged — skip missed messages and try again.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                // Channel closed.
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Try to receive a message without blocking.
    pub fn try_recv(&mut self) -> Option<Message> {
        match self.0.try_recv() {
            Ok(msg) => Some(msg),
            Err(_) => None,
        }
    }
}

// ── MessageQueue ──────────────────────────────────────────────────────────────

/// The shared message queue.
///
/// Cheap to clone — all clones share the same underlying channel.
#[derive(Clone)]
pub struct MessageQueue {
    sender: MessageSender,
}

impl MessageQueue {
    /// Create a new queue.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender: MessageSender(Arc::new(tx)),
        }
    }

    /// Publish a message to all current subscribers.
    pub fn publish(&self, msg: Message) {
        self.sender.publish(msg);
    }

    /// Create a new subscriber that will receive all messages published after
    /// this call.
    pub fn subscribe(&self) -> MessageReceiver {
        self.sender.subscribe()
    }

    /// Return a clone of the sender for passing to async tasks.
    pub fn sender(&self) -> MessageSender {
        self.sender.clone()
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}
