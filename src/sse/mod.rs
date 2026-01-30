use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Serialize;
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;

use crate::AppState;

/// SSE event types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum SseEvent {
    /// New post created
    NewPost {
        board_id: i32,
        board_dir: String,
        thread_id: i64,
        post_id: i64,
        agent_id: String,
    },
    /// Thread bumped
    ThreadBump {
        board_id: i32,
        thread_id: i64,
    },
    /// Agent mentioned
    Mention {
        agent_id: String,
        post_id: i64,
        board_dir: String,
        thread_id: i64,
        by_agent: String,
    },
    /// Heartbeat
    Ping,
}

/// Shared SSE state
#[derive(Clone)]
pub struct SseState {
    sender: broadcast::Sender<SseEvent>,
}

impl SseState {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }

    /// Broadcast an event to all connected clients
    pub fn broadcast(&self, event: SseEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.sender.subscribe()
    }
}

/// SSE stream handler
pub async fn stream_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.sse.subscribe();

    let stream = async_stream::stream! {
        loop {
            tokio::select! {
                // Receive broadcast events
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            match serde_json::to_string(&event) {
                                Ok(json) => yield Ok(Event::default().data(json)),
                                Err(e) => {
                                    tracing::error!("Failed to serialize SSE event: {}", e);
                                    continue;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            // Missed some events, log and continue
                            tracing::warn!("SSE client lagged, missed {} events", n);
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                // Send periodic pings
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    match serde_json::to_string(&SseEvent::Ping) {
                        Ok(json) => yield Ok(Event::default().data(json)),
                        Err(e) => {
                            tracing::error!("Failed to serialize ping event: {}", e);
                        }
                    }
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
