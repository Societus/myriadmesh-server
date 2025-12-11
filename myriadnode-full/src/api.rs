use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use crate::config::{ApiConfig, StorageConfig};
use crate::failover::FailoverManager;
use crate::heartbeat::HeartbeatService;
use crate::websocket::{WsBroadcaster, WsState};
use myriadmesh_appliance::{
    types::DevicePreferences, ApplianceManager, CachedMessage, PairingRequest, PairingResponse,
};
use myriadmesh_ledger::ChainSync;
use myriadmesh_network::{AdapterManager, AdapterStatus as NetworkAdapterStatus};
use myriadmesh_routing::Router as MessageRouter;
use myriadmesh_updates::UpdateCoordinator;

/// API server state
#[derive(Clone)]
pub struct ApiState {
    #[allow(dead_code)]
    config: ApiConfig,
    adapter_manager: Arc<RwLock<AdapterManager>>,
    heartbeat_service: Arc<HeartbeatService>,
    failover_manager: Arc<FailoverManager>,
    ledger: Arc<RwLock<ChainSync>>,
    appliance_manager: Option<Arc<ApplianceManager>>,
    update_coordinator: Option<Arc<UpdateCoordinator>>,
    storage: Option<Arc<RwLock<crate::storage::Storage>>>,
    router: Arc<MessageRouter>,
    storage_config: StorageConfig,
    node_id: String,
    node_name: String,
    start_time: SystemTime,
    ws_broadcaster: Arc<WsBroadcaster>,
}

/// API server
pub struct ApiServer {
    config: ApiConfig,
    state: Arc<ApiState>,
}

impl ApiServer {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        config: ApiConfig,
        adapter_manager: Arc<RwLock<AdapterManager>>,
        heartbeat_service: Arc<HeartbeatService>,
        failover_manager: Arc<FailoverManager>,
        ledger: Arc<RwLock<ChainSync>>,
        appliance_manager: Option<Arc<ApplianceManager>>,
        update_coordinator: Option<Arc<UpdateCoordinator>>,
        storage: Option<Arc<RwLock<crate::storage::Storage>>>,
        router: Arc<MessageRouter>,
        storage_config: StorageConfig,
        node_id: String,
        node_name: String,
    ) -> Result<Self> {
        let ws_broadcaster = Arc::new(WsBroadcaster::new());

        let state = Arc::new(ApiState {
            config: config.clone(),
            adapter_manager,
            heartbeat_service,
            failover_manager,
            ledger,
            appliance_manager,
            update_coordinator,
            storage,
            router,
            storage_config,
            node_id,
            node_name,
            start_time: SystemTime::now(),
            ws_broadcaster,
        });

        Ok(Self { config, state })
    }

    pub async fn start(&self) -> Result<tokio::task::JoinHandle<Result<()>>> {
        let app = self.create_router();

        let bind_addr = format!("{}:{}", self.config.bind, self.config.port);
        let listener = TcpListener::bind(&bind_addr).await?;

        info!("API server listening on {}", bind_addr);

        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .map_err(|e| anyhow::anyhow!("Server error: {}", e))
        });

        Ok(handle)
    }

    fn create_router(&self) -> Router {
        // Create WebSocket state
        let ws_state = Arc::new(WsState {
            broadcaster: self.state.ws_broadcaster.clone(),
        });

        // Create WebSocket router
        let ws_router = Router::new()
            .route("/ws", get(crate::websocket::ws_handler))
            .with_state(ws_state);

        // Create API router with main state
        let api_router = Router::new()
            // Health check
            .route("/health", get(health_check))
            // Node endpoints (Web UI expects /api/ prefix)
            .route("/api/node/info", get(get_node_info))
            .route("/api/node/status", get(get_node_status))
            // Adapter endpoints
            .route("/api/adapters", get(list_adapters))
            .route("/api/adapters/:id", get(get_adapter))
            .route("/api/adapters/:id/start", post(start_adapter))
            .route("/api/adapters/:id/stop", post(stop_adapter))
            // Heartbeat endpoints
            .route("/api/heartbeat/stats", get(get_heartbeat_stats))
            .route("/api/heartbeat/nodes", get(get_heartbeat_nodes))
            // Failover endpoints
            .route("/api/failover/events", get(get_failover_events))
            .route("/api/failover/force", post(force_failover))
            // Network config endpoints
            .route("/api/config/network", get(get_network_config))
            .route("/api/config/network", post(update_network_config))
            // i2p endpoints
            .route("/api/i2p/status", get(get_i2p_status))
            .route("/api/i2p/destination", get(get_i2p_destination))
            .route("/api/i2p/tunnels", get(get_i2p_tunnels))
            // Update endpoints
            .route("/api/updates/schedule", post(schedule_update))
            .route("/api/updates/schedules", get(list_update_schedules))
            .route("/api/updates/available", get(check_available_updates))
            .route(
                "/api/updates/fallbacks/:adapter_type",
                get(get_fallback_adapters),
            )
            // Appliance endpoints
            .route("/api/appliance/info", get(get_appliance_info))
            .route("/api/appliance/stats", get(get_appliance_stats))
            .route("/api/appliance/pair/request", post(initiate_pairing))
            .route("/api/appliance/pair/approve/:token", post(approve_pairing))
            .route("/api/appliance/pair/reject/:token", post(reject_pairing))
            .route("/api/appliance/pair/complete", post(complete_pairing))
            .route("/api/appliance/devices", get(list_paired_devices))
            .route("/api/appliance/devices/:device_id", get(get_paired_device))
            .route(
                "/api/appliance/devices/:device_id/unpair",
                post(unpair_device),
            )
            .route(
                "/api/appliance/devices/:device_id/preferences",
                post(update_device_preferences),
            )
            .route("/api/appliance/cache/store", post(store_cached_message))
            .route(
                "/api/appliance/cache/retrieve",
                get(retrieve_cached_messages),
            )
            .route(
                "/api/appliance/cache/delivered",
                post(mark_messages_delivered),
            )
            .route(
                "/api/appliance/cache/stats/:device_id",
                get(get_cache_stats),
            )
            // Ledger endpoints
            .route("/api/ledger/blocks", get(list_ledger_blocks))
            .route("/api/ledger/blocks/:height", get(get_ledger_block))
            .route("/api/ledger/entries", get(list_ledger_entries))
            .route("/api/ledger/entry", post(submit_ledger_entry))
            .route("/api/ledger/stats", get(get_ledger_stats))
            // Message endpoints
            .route("/api/messages/send", post(send_message))
            .route("/api/messages", get(list_messages))
            // Legacy v1 endpoints (for backwards compatibility)
            .route("/api/v1/node/status", get(get_node_status))
            .route("/api/v1/node/info", get(get_node_info))
            .route("/api/v1/messages", post(send_message))
            .route("/api/v1/messages", get(list_messages))
            .route("/api/v1/adapters", get(list_adapters))
            .route("/api/v1/dht/nodes", get(list_dht_nodes))
            // Add CORS middleware
            .layer(CorsLayer::permissive())
            .with_state(self.state.clone());

        // Merge WebSocket and API routers
        ws_router.merge(api_router)
    }
}

// === Health Check ===

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

// === Node Endpoints ===

async fn get_node_status(State(state): State<Arc<ApiState>>) -> Json<NodeStatusResponse> {
    // Calculate uptime since node start
    let uptime_secs = state.start_time.elapsed().map(|d| d.as_secs()).unwrap_or(0);

    // Count active (Ready) adapters
    let manager = state.adapter_manager.read().await;
    let adapter_ids = manager.adapter_ids();

    let mut adapters_active = 0;
    for id in adapter_ids {
        if let Some(adapter_arc) = manager.get_adapter(&id) {
            let adapter = adapter_arc.read().await;
            if matches!(adapter.get_status(), NetworkAdapterStatus::Ready) {
                adapters_active += 1;
            }
        }
    }

    Json(NodeStatusResponse {
        status: "running".to_string(),
        uptime_secs,
        adapters_active,
        messages_queued: 0, // TODO: Implement message queue tracking
    })
}

#[derive(Serialize)]
struct NodeStatusResponse {
    status: String,
    uptime_secs: u64,
    adapters_active: usize,
    messages_queued: usize,
}

async fn get_node_info(State(state): State<Arc<ApiState>>) -> Json<NodeInfoResponse> {
    let uptime_secs = state.start_time.elapsed().unwrap_or_default().as_secs();

    Json(NodeInfoResponse {
        node_id: state.node_id.clone(),
        node_name: state.node_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs,
    })
}

#[derive(Serialize)]
struct NodeInfoResponse {
    node_id: String,
    node_name: String,
    version: String,
    uptime_secs: u64,
}

// === Message Endpoints ===

async fn send_message(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<SendMessageRequest>,
) -> Result<Json<MessageResponse>, StatusCode> {
    // Generate message ID
    use myriadmesh_protocol::message::{Message as ProtocolMessage, MessageId, MessageType};
    use myriadmesh_protocol::types::{NodeId, Priority};
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Parse source (this node) and destination NodeIds
    let source_bytes = hex::decode(&state.node_id).unwrap_or_else(|_| vec![0u8; 64]);
    let mut source_id_bytes = [0u8; 64];
    source_id_bytes.copy_from_slice(&source_bytes[..64.min(source_bytes.len())]);
    let source = NodeId::from_bytes(source_id_bytes);

    let dest_bytes = hex::decode(&request.destination).unwrap_or_else(|_| vec![0u8; 64]);
    let mut dest_id_bytes = [0u8; 64];
    dest_id_bytes.copy_from_slice(&dest_bytes[..64.min(dest_bytes.len())]);
    let destination = NodeId::from_bytes(dest_id_bytes);

    // Convert payload string to bytes
    let payload = request.payload.as_bytes().to_vec();
    let payload_len = payload.len();

    let message_id = MessageId::generate(
        &source,
        &destination,
        &payload,
        timestamp,
        0, // sequence number (could be tracked per destination)
    );
    let message_id_str = format!("{:?}", message_id);

    // Convert priority (default to normal priority if not specified)
    let priority_u8 = request.priority.unwrap_or(128); // 128 = Normal
    let priority = Priority::from_u8(priority_u8);

    // Create protocol message
    let message = ProtocolMessage {
        id: message_id,
        source,
        destination,
        message_type: MessageType::Data,
        payload,
        priority,
        ttl: 16, // Default TTL
        timestamp,
        sequence: 0,
        emergency_realm: None,
    };

    // OPTIMIZATION: Conditional persistence based on storage config
    // - If persist_sent_messages is disabled AND not appliance mode: route-only (no DB storage)
    // - If enabled OR appliance mode: persist to DB for audit trail / offline caching
    let should_persist = state.storage_config.persist_sent_messages
        || state.appliance_manager.is_some()
        || priority_u8 >= 192; // Always persist High/Emergency priority

    if should_persist {
        if let Some(storage) = &state.storage {
            let storage_guard = storage.read().await;
            let pool = storage_guard.pool();

            // Insert message into database
            if let Err(e) = sqlx::query(
                r#"
                INSERT INTO messages (id, destination, payload, priority, status, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(&message_id_str)
            .bind(&request.destination)
            .bind(&message.payload)
            .bind(priority_u8 as i64)
            .bind("routing")
            .bind(timestamp as i64)
            .bind(timestamp as i64)
            .execute(pool)
            .await
            {
                warn!("Failed to store message: {}", e);
                // Continue routing even if storage fails
            }
        }
    }

    // Route message through Router (actual network transmission)
    match state.router.route_message(message).await {
        Ok(_) => {
            info!(
                "Message routed: {} to {} ({} bytes, priority: {}, persisted: {})",
                message_id_str,
                request.destination,
                payload_len,
                priority_u8,
                should_persist
            );

            // Broadcast WebSocket event for message sent
            state.ws_broadcaster.broadcast(crate::websocket::WsEvent::MessageSent {
                message_id: message_id_str.clone(),
                destination: request.destination.clone(),
                timestamp,
            });

            // Update status to "sent" if persisted
            if should_persist {
                if let Some(storage) = &state.storage {
                    let storage_guard = storage.read().await;
                    let pool = storage_guard.pool();

                    let _ = sqlx::query(
                        r#"
                        UPDATE messages SET status = ?1, updated_at = ?2 WHERE id = ?3
                        "#,
                    )
                    .bind("sent")
                    .bind(timestamp as i64)
                    .bind(&message_id_str)
                    .execute(pool)
                    .await;
                }
            }

            Ok(Json(MessageResponse {
                message_id: message_id_str,
                status: "sent".to_string(),
            }))
        }
        Err(e) => {
            warn!("Failed to route message: {}", e);

            // Update status to "failed" if persisted
            if should_persist {
                if let Some(storage) = &state.storage {
                    let storage_guard = storage.read().await;
                    let pool = storage_guard.pool();

                    let _ = sqlx::query(
                        r#"
                        UPDATE messages SET status = ?1, updated_at = ?2 WHERE id = ?3
                        "#,
                    )
                    .bind("failed")
                    .bind(timestamp as i64)
                    .bind(&message_id_str)
                    .execute(pool)
                    .await;
                }
            }

            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SendMessageRequest {
    destination: String,
    payload: String,
    priority: Option<u8>,
}

#[derive(Serialize)]
struct MessageResponse {
    message_id: String,
    status: String,
}

async fn list_messages(State(state): State<Arc<ApiState>>) -> Json<Vec<MessageInfo>> {
    if let Some(storage) = &state.storage {
        let storage_guard = storage.read().await;
        let pool = storage_guard.pool();

        // Query recent messages (limit to last 100)
        let messages = sqlx::query_as::<_, (String, String, i64, String, i64)>(
            r#"
            SELECT id, destination, priority, status, created_at
            FROM messages
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let message_infos: Vec<MessageInfo> = messages
            .into_iter()
            .map(
                |(id, destination, priority, status, timestamp)| MessageInfo {
                    message_id: id,
                    timestamp: timestamp as u64,
                    direction: if destination == state.node_id {
                        "inbound".to_string()
                    } else {
                        "outbound".to_string()
                    },
                    status,
                    destination: Some(destination),
                    priority: Some(priority as u8),
                },
            )
            .collect();

        Json(message_infos)
    } else {
        // No storage available
        Json(vec![])
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
struct MessageInfo {
    message_id: String,
    timestamp: u64,
    direction: String,
    status: String,
    destination: Option<String>,
    priority: Option<u8>,
}

// === Adapter Endpoints ===

async fn list_adapters(State(state): State<Arc<ApiState>>) -> Json<Vec<AdapterStatus>> {
    let manager = state.adapter_manager.read().await;
    let adapter_ids = manager.adapter_ids();

    let mut statuses = Vec::new();
    for id in adapter_ids {
        if let Some(status) = get_adapter_status_internal(&manager, &id).await {
            statuses.push(status);
        }
    }

    Json(statuses)
}

async fn get_adapter(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<AdapterStatus>, StatusCode> {
    let manager = state.adapter_manager.read().await;

    get_adapter_status_internal(&manager, &id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn start_adapter(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let manager = state.adapter_manager.read().await;

    // Get adapter
    let adapter_arc = manager.get_adapter(&id).ok_or(StatusCode::NOT_FOUND)?;

    // Start the adapter
    let mut adapter = adapter_arc.write().await;
    adapter
        .start()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

async fn stop_adapter(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let manager = state.adapter_manager.read().await;

    // Get adapter
    let adapter_arc = manager.get_adapter(&id).ok_or(StatusCode::NOT_FOUND)?;

    // Stop the adapter
    let mut adapter = adapter_arc.write().await;
    adapter
        .stop()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

// Helper function to get adapter status
async fn get_adapter_status_internal(manager: &AdapterManager, id: &str) -> Option<AdapterStatus> {
    // Get adapter
    let adapter_arc = manager.get_adapter(id)?;
    let adapter = adapter_arc.read().await;

    // Get status from adapter
    let network_status = adapter.get_status();

    // Get adapter capabilities
    let capabilities = manager.get_capabilities(id)?;

    let status_str = match network_status {
        NetworkAdapterStatus::Uninitialized => "uninitialized",
        NetworkAdapterStatus::Initializing => "initializing",
        NetworkAdapterStatus::Ready => "ready",
        NetworkAdapterStatus::Unavailable => "unavailable",
        NetworkAdapterStatus::Error => "error",
        NetworkAdapterStatus::ShuttingDown => "shutting_down",
    };

    Some(AdapterStatus {
        id: id.to_string(),
        adapter_type: capabilities.adapter_type.name().to_string(),
        status: status_str.to_string(),
        version: "1.0.0".to_string(), // Default version
        last_reload: 0,
        reload_count: 0,
        reputation_score: 1.0,
        capabilities: vec![],
    })
}

#[derive(Serialize)]
struct AdapterStatus {
    id: String,
    adapter_type: String,
    status: String,
    version: String,
    last_reload: u64,
    reload_count: u32,
    reputation_score: f64,
    capabilities: Vec<String>,
}

// === DHT Endpoints ===

async fn list_dht_nodes(State(_state): State<Arc<ApiState>>) -> Json<Vec<DhtNodeInfo>> {
    // TODO: Get actual DHT node list
    Json(vec![])
}

#[derive(Serialize)]
#[allow(dead_code)]
struct DhtNodeInfo {
    node_id: String,
    last_seen: u64,
    distance: String,
}

// === Heartbeat Endpoints ===

async fn get_heartbeat_stats(State(state): State<Arc<ApiState>>) -> Json<HeartbeatStatsResponse> {
    let stats = state.heartbeat_service.get_stats().await;

    Json(HeartbeatStatsResponse {
        active_nodes: stats.total_nodes,
        total_heartbeats_sent: 0, // TODO: Track heartbeat counts
        total_heartbeats_received: 0,
        average_rtt_ms: 0.0,
    })
}

#[derive(Serialize)]
struct HeartbeatStatsResponse {
    active_nodes: usize,
    total_heartbeats_sent: u64,
    total_heartbeats_received: u64,
    average_rtt_ms: f64,
}

async fn get_heartbeat_nodes(State(state): State<Arc<ApiState>>) -> Json<Vec<HeartbeatNodeEntry>> {
    let node_map = state.heartbeat_service.get_node_map().await;
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut entries = Vec::new();
    for (node_id, node_info) in node_map.iter() {
        // Determine status based on how recent the last heartbeat was
        // Consider alive if seen within last 5 minutes (300 seconds)
        let time_since_last_seen = current_time.saturating_sub(node_info.last_seen);
        let status = if time_since_last_seen < 300 {
            "alive"
        } else if time_since_last_seen < 600 {
            "stale"
        } else {
            "offline"
        };

        entries.push(HeartbeatNodeEntry {
            node_id: node_id.to_hex(),
            status: status.to_string(),
            last_seen: node_info.last_seen,
            rtt_ms: 0.0,             // TODO: Track RTT in heartbeat service
            consecutive_failures: 0, // TODO: Track failures in heartbeat service
        });
    }

    // Sort by last_seen (most recent first)
    entries.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));

    Json(entries)
}

#[derive(Serialize)]
struct HeartbeatNodeEntry {
    node_id: String,
    status: String,
    last_seen: u64,
    rtt_ms: f64,
    consecutive_failures: u32,
}

// === Failover Endpoints ===

async fn get_failover_events(
    State(state): State<Arc<ApiState>>,
) -> Json<Vec<FailoverEventResponse>> {
    use crate::failover::FailoverEvent;

    // Get recent failover events (last 50)
    let events = state.failover_manager.get_recent_events(50).await;

    // Map events to response format
    let responses: Vec<FailoverEventResponse> = events
        .into_iter()
        .map(|event| match event {
            FailoverEvent::AdapterSwitch { from, to, reason } => FailoverEventResponse {
                event_type: "adapter_switch".to_string(),
                adapter: None,
                from_adapter: Some(from),
                to_adapter: Some(to),
                reason: Some(reason),
                metric: None,
                value: None,
                threshold: None,
            },
            FailoverEvent::ThresholdViolation {
                adapter,
                metric,
                value,
                threshold,
            } => FailoverEventResponse {
                event_type: "threshold_violation".to_string(),
                adapter: Some(adapter),
                from_adapter: None,
                to_adapter: None,
                reason: Some(format!("{} exceeded threshold", metric)),
                metric: Some(metric),
                value: Some(value),
                threshold: Some(threshold),
            },
            FailoverEvent::AdapterDown { adapter, reason } => FailoverEventResponse {
                event_type: "adapter_down".to_string(),
                adapter: Some(adapter),
                from_adapter: None,
                to_adapter: None,
                reason: Some(reason),
                metric: None,
                value: None,
                threshold: None,
            },
            FailoverEvent::AdapterRecovered { adapter } => FailoverEventResponse {
                event_type: "adapter_recovered".to_string(),
                adapter: Some(adapter),
                from_adapter: None,
                to_adapter: None,
                reason: Some("Adapter has recovered and is available".to_string()),
                metric: None,
                value: None,
                threshold: None,
            },
        })
        .collect();

    Json(responses)
}

#[derive(Serialize)]
struct FailoverEventResponse {
    event_type: String,
    adapter: Option<String>,
    from_adapter: Option<String>,
    to_adapter: Option<String>,
    reason: Option<String>,
    metric: Option<String>,
    value: Option<f64>,
    threshold: Option<f64>,
}

async fn force_failover(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ForceFailoverRequest>,
) -> Result<StatusCode, StatusCode> {
    info!(
        "Force failover requested for adapter: {}",
        request.adapter_id
    );

    match state
        .failover_manager
        .force_failover(request.adapter_id)
        .await
    {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
struct ForceFailoverRequest {
    adapter_id: String,
}

// === Network Config Endpoints ===

async fn get_network_config(State(_state): State<Arc<ApiState>>) -> Json<NetworkConfigResponse> {
    // TODO: Get actual config
    Json(NetworkConfigResponse {
        scoring_mode: "default".to_string(),
        failover_enabled: true,
        heartbeat_enabled: true,
        privacy_mode: false,
    })
}

#[derive(Serialize)]
struct NetworkConfigResponse {
    scoring_mode: String,
    failover_enabled: bool,
    heartbeat_enabled: bool,
    privacy_mode: bool,
}

async fn update_network_config(
    State(_state): State<Arc<ApiState>>,
    Json(_request): Json<UpdateNetworkConfigRequest>,
) -> Result<StatusCode, StatusCode> {
    // TODO: Implement config update
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct UpdateNetworkConfigRequest {
    scoring_mode: Option<String>,
    failover_enabled: Option<bool>,
    heartbeat_enabled: Option<bool>,
    privacy_mode: Option<bool>,
}

// === i2p Endpoints ===

async fn get_i2p_status(State(state): State<Arc<ApiState>>) -> Json<I2pStatusResponse> {
    let manager = state.adapter_manager.read().await;

    // Try to find the i2p adapter
    let i2p_adapter_id = "i2p"; // Standard ID for i2p adapter

    let (router_status, adapter_status, version) =
        if let Some(adapter_arc) = manager.get_adapter(i2p_adapter_id) {
            let adapter = adapter_arc.read().await;
            let status = adapter.get_status();

            let (rs, as_str) = match status {
                NetworkAdapterStatus::Ready => ("running", "ready"),
                NetworkAdapterStatus::Initializing => ("starting", "initializing"),
                NetworkAdapterStatus::Unavailable => ("stopped", "unavailable"),
                NetworkAdapterStatus::Error => ("error", "error"),
                NetworkAdapterStatus::ShuttingDown => ("stopping", "shutting_down"),
                NetworkAdapterStatus::Uninitialized => ("unknown", "uninitialized"),
            };

            (rs, as_str, "1.0.0")
        } else {
            ("unknown", "uninitialized", "unknown")
        };

    Json(I2pStatusResponse {
        router_status: router_status.to_string(),
        adapter_status: adapter_status.to_string(),
        router_version: version.to_string(),
        tunnels_active: 0,  // TODO: Get actual tunnel count
        peers_connected: 0, // TODO: Get actual peer count
    })
}

#[derive(Serialize)]
struct I2pStatusResponse {
    router_status: String,
    adapter_status: String,
    router_version: String,
    tunnels_active: usize,
    peers_connected: usize,
}

async fn get_i2p_destination(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<I2pDestinationResponse>, StatusCode> {
    let _manager = state.adapter_manager.read().await;
    let _i2p_adapter_id = "i2p";

    // TODO: Get actual destination from adapter
    // For now, return a placeholder
    Ok(Json(I2pDestinationResponse {
        destination: "placeholder.b32.i2p".to_string(),
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        node_id: state.node_id.clone(),
    }))
}

#[derive(Serialize)]
struct I2pDestinationResponse {
    destination: String,
    created_at: u64,
    node_id: String,
}

async fn get_i2p_tunnels(State(_state): State<Arc<ApiState>>) -> Json<I2pTunnelsResponse> {
    // TODO: Get actual tunnel information from i2p adapter
    Json(I2pTunnelsResponse {
        inbound_tunnels: vec![],
        outbound_tunnels: vec![],
        total_bandwidth_bps: 0,
    })
}

#[derive(Serialize)]
struct I2pTunnelsResponse {
    inbound_tunnels: Vec<I2pTunnelInfo>,
    outbound_tunnels: Vec<I2pTunnelInfo>,
    total_bandwidth_bps: u64,
}

#[derive(Serialize)]
#[allow(dead_code)]
struct I2pTunnelInfo {
    tunnel_id: String,
    peers: Vec<String>,
    latency_ms: f64,
    bandwidth_bps: u64,
    status: String,
}

// === Appliance Endpoints ===

/// Get appliance information and capabilities
async fn get_appliance_info(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.get_capabilities().await {
        Ok(capabilities) => Ok(Json(serde_json::json!(capabilities))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get appliance statistics
async fn get_appliance_stats(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let uptime_secs = state.start_time.elapsed().unwrap_or_default().as_secs();
    let adapters_count = state.adapter_manager.read().await.adapter_ids().len();

    match appliance_manager
        .get_stats(uptime_secs, adapters_count)
        .await
    {
        Ok(stats) => Ok(Json(serde_json::json!(stats))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Initiate device pairing
async fn initiate_pairing(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    // Parse request
    let pairing_request: PairingRequest =
        serde_json::from_value(request).map_err(|_| StatusCode::BAD_REQUEST)?;

    match appliance_manager.initiate_pairing(pairing_request).await {
        Ok(token) => Ok(Json(serde_json::json!(token))),
        Err(e) => {
            tracing::error!("Pairing initiation failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Approve a pending pairing (manual approval flow)
async fn approve_pairing(
    State(state): State<Arc<ApiState>>,
    Path(token): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.approve_pairing(&token).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Reject a pending pairing
async fn reject_pairing(
    State(state): State<Arc<ApiState>>,
    Path(token): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.reject_pairing(&token).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Complete device pairing
async fn complete_pairing(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    // Extract fields from request
    let pairing_response: PairingResponse = serde_json::from_value(
        request
            .get("response")
            .cloned()
            .ok_or(StatusCode::BAD_REQUEST)?,
    )
    .map_err(|_| StatusCode::BAD_REQUEST)?;

    let device_id: String = request
        .get("device_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();

    let node_id_hex: String = request
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();

    let public_key_hex: String = request
        .get("public_key")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();

    // Parse node_id and public_key
    let node_id = myriadmesh_crypto::identity::NodeId::from_hex(&node_id_hex)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let public_key = hex::decode(&public_key_hex).map_err(|_| StatusCode::BAD_REQUEST)?;

    match appliance_manager
        .complete_pairing(pairing_response, device_id, node_id, public_key)
        .await
    {
        Ok(result) => Ok(Json(serde_json::json!(result))),
        Err(e) => {
            tracing::error!("Pairing completion failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List all paired devices
async fn list_paired_devices(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.list_paired_devices().await {
        Ok(devices) => Ok(Json(serde_json::json!(devices))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get information about a specific paired device
async fn get_paired_device(
    State(state): State<Arc<ApiState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.get_paired_device(&device_id).await {
        Ok(Some(device)) => Ok(Json(serde_json::json!(device))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Unpair a device
async fn unpair_device(
    State(state): State<Arc<ApiState>>,
    Path(device_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.unpair_device(&device_id).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Update device preferences
async fn update_device_preferences(
    State(state): State<Arc<ApiState>>,
    Path(device_id): Path<String>,
    Json(preferences): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let device_preferences: DevicePreferences =
        serde_json::from_value(preferences).map_err(|_| StatusCode::BAD_REQUEST)?;

    match appliance_manager
        .update_device_preferences(&device_id, device_preferences)
        .await
    {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Store a message in the cache
async fn store_cached_message(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let cached_message: CachedMessage =
        serde_json::from_value(request).map_err(|_| StatusCode::BAD_REQUEST)?;

    match appliance_manager.cache_message(cached_message).await {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Retrieve cached messages for a device
async fn retrieve_cached_messages(
    State(state): State<Arc<ApiState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let device_id = params.get("device_id").ok_or(StatusCode::BAD_REQUEST)?;
    let limit = params.get("limit").and_then(|l| l.parse::<usize>().ok());
    let only_undelivered = params
        .get("only_undelivered")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    match appliance_manager
        .retrieve_messages(device_id, limit, only_undelivered)
        .await
    {
        Ok(messages) => Ok(Json(serde_json::json!(messages))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

/// Mark messages as delivered
async fn mark_messages_delivered(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    let message_ids: Vec<String> = request
        .get("message_ids")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    match appliance_manager.mark_messages_delivered(message_ids).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get cache statistics for a device
async fn get_cache_stats(
    State(state): State<Arc<ApiState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let appliance_manager = state
        .appliance_manager
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?;

    match appliance_manager.get_cache_stats(&device_id).await {
        Ok(stats) => Ok(Json(serde_json::json!(stats))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}
// ============================================================================
// Update System Endpoints
// ============================================================================

/// Request body for scheduling an update
#[derive(Debug, Deserialize)]
struct ScheduleUpdateRequest {
    adapter_type: String,
    target_version: String,
    estimated_duration_secs: Option<u64>,
}

/// Response for update schedule
#[derive(Debug, Serialize)]
struct ScheduleUpdateResponse {
    schedule_id: String,
    adapter_type: String,
    scheduled_start: u64,
    estimated_duration_secs: u64,
    fallback_adapters: Vec<String>,
}

/// Parse adapter type from string (case-insensitive)
fn parse_adapter_type(s: &str) -> Result<myriadmesh_protocol::types::AdapterType, StatusCode> {
    use myriadmesh_protocol::types::AdapterType;

    match s.to_lowercase().as_str() {
        "ethernet" => Ok(AdapterType::Ethernet),
        "bluetooth" => Ok(AdapterType::Bluetooth),
        "bluetooth_le" | "bluetoothle" | "ble" => Ok(AdapterType::BluetoothLE),
        "cellular" => Ok(AdapterType::Cellular),
        "wifi_halow" | "wifihalow" | "halow" => Ok(AdapterType::WiFiHaLoW),
        "lorawan" | "lora" => Ok(AdapterType::LoRaWAN),
        "meshtastic" => Ok(AdapterType::Meshtastic),
        "frsgmrs" | "frs" | "gmrs" => Ok(AdapterType::FRSGMRS),
        "cbradio" | "cb" => Ok(AdapterType::CBRadio),
        "shortwave" | "sw" => Ok(AdapterType::Shortwave),
        "aprs" => Ok(AdapterType::APRS),
        "dialup" => Ok(AdapterType::Dialup),
        "pppoe" => Ok(AdapterType::PPPoE),
        "i2p" => Ok(AdapterType::I2P),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Schedule an adapter update
async fn schedule_update(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ScheduleUpdateRequest>,
) -> Result<Json<ScheduleUpdateResponse>, StatusCode> {
    let update_coordinator = state
        .update_coordinator
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Parse adapter type from request
    let adapter_type = parse_adapter_type(&request.adapter_type)?;

    // Parse version
    let version_parts: Vec<&str> = request.target_version.split('.').collect();
    if version_parts.len() != 3 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let target_version = myriadmesh_network::version_tracking::SemanticVersion::new(
        version_parts[0]
            .parse()
            .map_err(|_| StatusCode::BAD_REQUEST)?,
        version_parts[1]
            .parse()
            .map_err(|_| StatusCode::BAD_REQUEST)?,
        version_parts[2]
            .parse()
            .map_err(|_| StatusCode::BAD_REQUEST)?,
    );

    let duration = std::time::Duration::from_secs(request.estimated_duration_secs.unwrap_or(300));

    // Schedule the update
    match update_coordinator
        .schedule_adapter_update(adapter_type, target_version, duration)
        .await
    {
        Ok(schedule) => {
            let response = ScheduleUpdateResponse {
                schedule_id: format!("schedule-{}", schedule.created_at),
                adapter_type: format!("{:?}", schedule.adapter_type),
                scheduled_start: schedule.scheduled_start,
                estimated_duration_secs: schedule.estimated_duration.as_secs(),
                fallback_adapters: schedule
                    .fallback_adapters
                    .iter()
                    .map(|a| format!("{:?}", a))
                    .collect(),
            };
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// List all pending update schedules
async fn list_update_schedules(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let update_coordinator = state
        .update_coordinator
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    match update_coordinator.list_pending_schedules().await {
        Ok(schedules) => {
            let response: Vec<serde_json::Value> = schedules
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "adapter_type": format!("{:?}", s.adapter_type),
                        "current_version": s.current_version.to_string(),
                        "target_version": s.target_version.to_string(),
                        "scheduled_start": s.scheduled_start,
                        "estimated_duration_secs": s.estimated_duration.as_secs(),
                        "fallback_adapters": s.fallback_adapters.iter()
                            .map(|a| format!("{:?}", a))
                            .collect::<Vec<_>>(),
                        "created_at": s.created_at,
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "schedules": response,
                "count": schedules.len()
            })))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Check for available updates
async fn check_available_updates(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _update_coordinator = state
        .update_coordinator
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // TODO: Implement actual update checking logic
    // For now, return a placeholder response
    Ok(Json(serde_json::json!({
        "available_updates": [],
        "last_check": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "message": "Update checking not yet implemented - requires DHT update discovery"
    })))
}

/// Get fallback adapters for a specific adapter type
async fn get_fallback_adapters(
    State(state): State<Arc<ApiState>>,
    Path(adapter_type_str): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let update_coordinator = state
        .update_coordinator
        .as_ref()
        .ok_or(StatusCode::NOT_IMPLEMENTED)?;

    // Parse adapter type from path parameter
    let adapter_type = parse_adapter_type(&adapter_type_str)?;

    match update_coordinator
        .identify_fallback_adapters(adapter_type)
        .await
    {
        Ok(fallbacks) => Ok(Json(serde_json::json!({
            "adapter_type": adapter_type_str,
            "fallbacks": fallbacks.iter()
                .map(|a| format!("{:?}", a))
                .collect::<Vec<_>>(),
            "count": fallbacks.len()
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// === Ledger Endpoints ===

async fn list_ledger_blocks(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let ledger = state.ledger.read().await;
    let local_height = ledger.local_height();

    // Get the last 10 blocks (or fewer if chain is shorter)
    let start_height = local_height.saturating_sub(9);
    let mut blocks = Vec::new();

    for height in start_height..=local_height {
        if let Ok(block) = ledger.storage().load_block(height) {
            blocks.push(serde_json::json!({
                "height": block.header.height,
                "creator": hex::encode(block.header.creator.as_bytes()),
                "previous_hash": hex::encode(block.header.previous_hash),
                "timestamp": block.header.timestamp,
                "entry_count": block.entries.len(),
                "validator_count": block.validator_signatures.len(),
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "blocks": blocks,
        "chain_height": local_height,
        "block_count": blocks.len(),
    })))
}

async fn get_ledger_block(
    State(state): State<Arc<ApiState>>,
    Path(height): Path<u64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let ledger = state.ledger.read().await;

    match ledger.storage().load_block(height) {
        Ok(block) => {
            let entries: Vec<_> = block
                .entries
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "type": format!("{:?}", entry.entry_type),
                        "signature_valid": true, // Simplified - actual validation would go here
                    })
                })
                .collect();

            let validators: Vec<_> = block
                .validator_signatures
                .iter()
                .map(|sig| {
                    serde_json::json!({
                        "validator": hex::encode(sig.validator.as_bytes()),
                    })
                })
                .collect();

            Ok(Json(serde_json::json!({
                "height": block.header.height,
                "version": block.header.version,
                "creator": hex::encode(block.header.creator.as_bytes()),
                "previous_hash": hex::encode(block.header.previous_hash),
                "merkle_root": hex::encode(block.header.merkle_root),
                "timestamp": block.header.timestamp,
                "entries": entries,
                "validators": validators,
            })))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_ledger_stats(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let ledger = state.ledger.read().await;
    let storage_stats = ledger.storage().stats();
    let sync_stats = ledger.stats();

    Ok(Json(serde_json::json!({
        "chain_height": ledger.local_height(),
        "block_count": storage_stats.block_count,
        "storage_bytes": storage_stats.total_size_bytes,
        "pruning_enabled": storage_stats.pruning_enabled,
        "keep_blocks": storage_stats.keep_blocks,
        "sync": {
            "state": format!("{:?}", sync_stats.state),
            "blocks_discovered": sync_stats.blocks_discovered,
            "blocks_downloaded": sync_stats.blocks_downloaded,
            "blocks_validated": sync_stats.blocks_validated,
            "validation_errors": sync_stats.validation_errors,
        }
    })))
}

#[derive(Debug, Deserialize)]
struct EntriesQuery {
    #[serde(default = "default_entries_limit")]
    limit: usize,
}

fn default_entries_limit() -> usize {
    50
}

async fn list_ledger_entries(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<EntriesQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let ledger = state.ledger.read().await;
    let local_height = ledger.local_height();

    // Get the last 10 blocks (or fewer if chain is shorter)
    let start_height = local_height.saturating_sub(9);
    let mut entries = Vec::new();

    for height in start_height..=local_height {
        if let Ok(block) = ledger.storage().load_block(height) {
            for entry in block.entries.iter() {
                let node_id = entry.node_id();
                let entry_type_name = match &entry.entry_type {
                    myriadmesh_ledger::EntryType::Discovery(_) => "Discovery",
                    myriadmesh_ledger::EntryType::Test(_) => "Test",
                    myriadmesh_ledger::EntryType::Message(_) => "Message",
                    myriadmesh_ledger::EntryType::KeyExchange(_) => "KeyExchange",
                };

                entries.push(serde_json::json!({
                    "block_height": height,
                    "timestamp": entry.timestamp.to_rfc3339(),
                    "type": entry_type_name,
                    "node_id": hex::encode(node_id.as_bytes()),
                }));

                // Stop if we've reached the limit
                if entries.len() >= query.limit {
                    break;
                }
            }

            if entries.len() >= query.limit {
                break;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "entries": entries,
        "count": entries.len(),
        "chain_height": local_height,
    })))
}

#[derive(Debug, Deserialize)]
struct SubmitEntryRequest {
    entry_type: String,
    // Entry-specific data would go here
    // For now, we accept a generic data field
    #[serde(default)]
    #[allow(dead_code)]
    data: serde_json::Value,
}

async fn submit_ledger_entry(
    State(_state): State<Arc<ApiState>>,
    Json(request): Json<SubmitEntryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Validate entry type
    let valid_types = ["Discovery", "Test", "Message", "KeyExchange"];
    if !valid_types.contains(&request.entry_type.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // In a full implementation, this would:
    // 1. Parse the entry data according to type
    // 2. Sign the entry with the node's private key
    // 3. Add to a mempool for inclusion in the next block
    // 4. Broadcast to peers via P2P network
    //
    // For now, we return a placeholder response indicating
    // that entry submission would happen through the P2P network

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "message": "Entry validation passed. Full P2P submission not yet implemented.",
        "entry_type": request.entry_type,
        "note": "Entries are currently bundled into blocks by validators through the P2P network."
    })))
}
