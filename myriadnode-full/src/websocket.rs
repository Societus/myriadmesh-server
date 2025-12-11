//! WebSocket support for real-time updates
//!
//! This module provides WebSocket connectivity for the web UI to receive
//! real-time updates about node status, messages, adapter changes, etc.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// WebSocket event types that can be broadcast to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Node information updated
    NodeInfo {
        id: String,
        name: String,
        version: String,
        uptime_secs: u64,
    },
    /// Adapter status changed
    AdapterStatus {
        adapter_id: String,
        active: bool,
        health_status: String,
    },
    /// New message received
    MessageReceived {
        message_id: String,
        source: String,
        content: String,
        timestamp: u64,
    },
    /// Message sent confirmation
    MessageSent {
        message_id: String,
        destination: String,
        timestamp: u64,
    },
    /// Heartbeat statistics updated
    HeartbeatUpdate {
        total_nodes: usize,
        nodes_with_location: usize,
    },
    /// Failover event occurred
    FailoverEvent {
        from_adapter: String,
        to_adapter: String,
        reason: String,
        timestamp: u64,
    },
    /// DHT node discovered
    DhtNodeDiscovered {
        node_id: String,
        adapters: Vec<String>,
    },
    /// Generic status update
    StatusUpdate {
        message: String,
    },
}

/// WebSocket broadcaster
#[derive(Clone)]
pub struct WsBroadcaster {
    tx: broadcast::Sender<WsEvent>,
}

impl WsBroadcaster {
    /// Create a new WebSocket broadcaster
    pub fn new() -> Self {
        // Create a broadcast channel with capacity for 100 events
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    /// Broadcast an event to all connected clients
    pub fn broadcast(&self, event: WsEvent) {
        // It's okay if there are no receivers
        let _ = self.tx.send(event);
    }

    /// Get a receiver for subscribing to events
    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.tx.subscribe()
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for WsBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket handler state
#[derive(Clone)]
pub struct WsState {
    pub broadcaster: Arc<WsBroadcaster>,
}

/// Handle WebSocket upgrade request
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WsState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle an individual WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<WsState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to broadcast events
    let mut rx = state.broadcaster.subscribe();

    info!("WebSocket client connected (total: {})", state.broadcaster.subscriber_count());

    // Send initial connection confirmation
    let welcome = WsEvent::StatusUpdate {
        message: "Connected to MyriadNode WebSocket".to_string(),
    };

    if let Ok(json) = serde_json::to_string(&welcome) {
        if sender.send(Message::Text(json)).await.is_err() {
            warn!("Failed to send welcome message");
            return;
        }
    }

    // Spawn a task to handle incoming messages from the client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Serialize event to JSON and send
            match serde_json::to_string(&event) {
                Ok(json) => {
                    if sender.send(Message::Text(json)).await.is_err() {
                        debug!("Client disconnected");
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize event: {}", e);
                }
            }
        }
    });

    // Handle messages from the client (mostly ping/pong for keepalive)
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(_text)) => {
                    // Could handle client -> server messages here if needed
                    debug!("Received text message from client");
                }
                Ok(Message::Binary(_data)) => {
                    debug!("Received binary message from client");
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping, sending pong");
                    // Axum automatically handles pong responses
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Close(_)) => {
                    debug!("Client sent close frame");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    info!("WebSocket client disconnected (remaining: {})", state.broadcaster.subscriber_count() - 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broadcaster_creation() {
        let broadcaster = WsBroadcaster::new();
        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[test]
    fn test_broadcast_event() {
        let broadcaster = WsBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        let event = WsEvent::StatusUpdate {
            message: "Test".to_string(),
        };

        broadcaster.broadcast(event.clone());

        // Should receive the event
        let received = rx.try_recv();
        assert!(received.is_ok());
    }

    #[test]
    fn test_subscriber_count() {
        let broadcaster = WsBroadcaster::new();
        assert_eq!(broadcaster.subscriber_count(), 0);

        let _rx1 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 1);

        let _rx2 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 2);
    }
}
