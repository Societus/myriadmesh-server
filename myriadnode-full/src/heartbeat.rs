use anyhow::{bail, Result};
use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use sodiumoxide::crypto::sign::ed25519;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

use myriadmesh_crypto::identity::NodeIdentity;
use myriadmesh_crypto::signing::{sign_message, verify_signature, Signature};
use myriadmesh_network::AdapterManager;
use myriadmesh_protocol::NodeId;

use crate::backhaul::{BackhaulDetector, BackhaulStatus};
use crate::config::AdapterConfig;

/// Heartbeat message sent between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    /// Node ID of the sender
    pub node_id: NodeId,
    /// Timestamp when heartbeat was generated (Unix epoch seconds)
    pub timestamp: u64,
    /// List of available network adapters
    pub adapters: Vec<AdapterInfo>,
    /// Optional geolocation data (only if node permits)
    pub geolocation: Option<GeolocationData>,
    /// SECURITY H3: Ed25519 public key (32 bytes) for signature verification
    /// Must derive to node_id via BLAKE2b to prevent impersonation
    pub public_key: Vec<u8>,
    /// SECURITY H3: Ed25519 signature (64 bytes) over (node_id || timestamp || adapters || geolocation || public_key)
    /// Prevents route poisoning attacks where malicious nodes advertise fake routes
    pub signature: Vec<u8>,
}

/// Information about a network adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    /// Adapter identifier (e.g., "ethernet", "bluetooth")
    pub adapter_id: String,
    /// Adapter type
    pub adapter_type: String,
    /// Is this adapter currently active?
    pub active: bool,
    /// Is this adapter being used as backhaul?
    pub is_backhaul: bool,
    /// Approximate bandwidth (bps)
    pub bandwidth_bps: u64,
    /// Latency (ms)
    pub latency_ms: u32,
    /// Reliability (0.0 to 1.0)
    pub reliability: f64,
    /// Privacy level (0.0 traceable to 1.0 anonymous)
    pub privacy_level: f64,
}

/// Optional geolocation data (privacy-aware)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeolocationData {
    /// Latitude (degrees)
    pub latitude: f64,
    /// Longitude (degrees)
    pub longitude: f64,
    /// Accuracy radius (meters)
    pub accuracy_meters: f64,
    /// Optional country code
    pub country_code: Option<String>,
    /// Optional city name
    pub city: Option<String>,
}

/// Node information stored in NodeMap
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub last_seen: u64,
    pub adapters: Vec<AdapterInfo>,
    pub geolocation: Option<GeolocationData>,
    /// How many heartbeats received from this node
    pub heartbeat_count: u64,
}

impl NodeInfo {
    fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            last_seen: current_timestamp(),
            adapters: Vec::new(),
            geolocation: None,
            heartbeat_count: 0,
        }
    }

    fn update_from_heartbeat(&mut self, heartbeat: &HeartbeatMessage) {
        self.last_seen = heartbeat.timestamp;
        self.adapters = heartbeat.adapters.clone();
        self.geolocation = heartbeat.geolocation.clone();
        self.heartbeat_count += 1;
    }

    /// Check if this node is stale (hasn't sent heartbeat recently)
    fn is_stale(&self, timeout_secs: u64) -> bool {
        let now = current_timestamp();
        (now - self.last_seen) > timeout_secs
    }
}

/// Configuration for heartbeat service
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Enable heartbeat broadcasting
    pub enabled: bool,
    /// Heartbeat interval (seconds)
    pub interval_secs: u64,
    /// Timeout for considering a node offline (seconds)
    pub timeout_secs: u64,
    /// Include geolocation data in heartbeats
    pub include_geolocation: bool,
    /// Allow storing location data from other nodes
    pub store_remote_geolocation: bool,
    /// Maximum number of nodes to track in NodeMap
    pub max_nodes: usize,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 60,               // Send heartbeat every minute
            timeout_secs: 300,               // Consider node offline after 5 minutes
            include_geolocation: false,      // Privacy-first default
            store_remote_geolocation: false, // Don't store others' locations by default
            max_nodes: 1000,                 // Track up to 1000 nodes
        }
    }
}

/// NodeMap stored in DHT - maps NodeId to NodeInfo
pub type NodeMap = HashMap<NodeId, NodeInfo>;

/// Rate limiter for heartbeat messages
pub struct HeartbeatRateLimiter {
    /// Last heartbeat received per node
    last_received: HashMap<NodeId, Instant>,

    /// Minimum interval between heartbeats from same node
    min_interval: Duration,

    /// Global rate limit (heartbeats per second)
    max_per_second: usize,

    /// Recent heartbeat timestamps (sliding window)
    recent: VecDeque<Instant>,
}

impl HeartbeatRateLimiter {
    pub fn new(min_interval_secs: u64, max_per_second: usize) -> Self {
        Self {
            last_received: HashMap::new(),
            min_interval: Duration::from_secs(min_interval_secs),
            max_per_second,
            recent: VecDeque::new(),
        }
    }

    pub fn allow(&mut self, node_id: &NodeId) -> bool {
        let now = Instant::now();

        // Check per-node rate limit
        if let Some(last) = self.last_received.get(node_id) {
            if now.duration_since(*last) < self.min_interval {
                return false;
            }
        }

        // Check global rate limit
        // Remove entries older than 1 second
        while let Some(front) = self.recent.front() {
            if now.duration_since(*front) > Duration::from_secs(1) {
                self.recent.pop_front();
            } else {
                break;
            }
        }

        if self.recent.len() >= self.max_per_second {
            return false;
        }

        // Allow heartbeat
        self.last_received.insert(*node_id, now);
        self.recent.push_back(now);

        true
    }
}

/// Heartbeat service for node discovery and status reporting
pub struct HeartbeatService {
    config: HeartbeatConfig,
    local_node_id: NodeId,
    node_map: Arc<RwLock<NodeMap>>,
    identity: Arc<NodeIdentity>,
    adapter_manager: Arc<RwLock<AdapterManager>>,
    backhaul_detector: Arc<BackhaulDetector>,
    rate_limiter: Arc<RwLock<HeartbeatRateLimiter>>,
    adapter_configs: HashMap<String, AdapterConfig>,
    // RESOURCE M4: Task handle management for graceful shutdown
    shutdown_tx: broadcast::Sender<()>,
    broadcast_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    cleanup_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl HeartbeatService {
    pub fn new(
        config: HeartbeatConfig,
        local_node_id: NodeId,
        identity: Arc<NodeIdentity>,
        adapter_manager: Arc<RwLock<AdapterManager>>,
        backhaul_detector: Arc<BackhaulDetector>,
        adapter_configs: HashMap<String, AdapterConfig>,
    ) -> Self {
        // Create rate limiter (30 second minimum per node, 100/sec global)
        let rate_limiter = Arc::new(RwLock::new(HeartbeatRateLimiter::new(30, 100)));
        // RESOURCE M4: Create shutdown channel for graceful task termination
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        Self {
            config,
            local_node_id,
            node_map: Arc::new(RwLock::new(HashMap::new())),
            identity,
            adapter_manager,
            backhaul_detector,
            rate_limiter,
            adapter_configs,
            shutdown_tx,
            broadcast_task: Arc::new(RwLock::new(None)),
            cleanup_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Collect adapter information for heartbeat broadcast
    async fn collect_adapter_info(&self) -> Result<Vec<AdapterInfo>> {
        let manager = self.adapter_manager.read().await;
        let mut adapters = Vec::new();

        for adapter_id in manager.adapter_ids() {
            // Check if heartbeat allowed (from config)
            if let Some(adapter_config) = self.adapter_configs.get(&adapter_id) {
                if !adapter_config.allow_heartbeat {
                    debug!(
                        "Adapter {} excluded: heartbeat disabled in config",
                        adapter_id
                    );
                    continue;
                }
            }

            // Get adapter
            let adapter_arc = match manager.get_adapter(&adapter_id) {
                Some(a) => a,
                None => continue,
            };

            let adapter = adapter_arc.read().await;

            // Check if adapter is ready
            let status = adapter.get_status();
            if status != myriadmesh_network::adapter::AdapterStatus::Ready {
                debug!("Adapter {} excluded: status = {:?}", adapter_id, status);
                continue;
            }

            // Check if backhaul (exclude by default unless configured)
            let is_backhaul = self
                .backhaul_detector
                .check_interface(&adapter_id)
                .unwrap_or(BackhaulStatus::Unknown)
                == BackhaulStatus::IsBackhaul;

            if is_backhaul {
                if let Some(adapter_config) = self.adapter_configs.get(&adapter_id) {
                    if !adapter_config.allow_backhaul_mesh {
                        debug!(
                            "Adapter {} excluded: is backhaul and backhaul mesh disabled",
                            adapter_id
                        );
                        continue;
                    }
                }
            }

            // Get capabilities and metrics
            let capabilities = adapter.get_capabilities();
            let metrics = manager.get_metrics(&adapter_id);

            let (bandwidth_bps, latency_ms, reliability) = if let Some(m) = metrics {
                (m.bandwidth_bps, m.latency_ms as u32, m.reliability)
            } else {
                // Use capabilities as fallback
                (
                    capabilities.typical_bandwidth_bps,
                    capabilities.typical_latency_ms as u32,
                    capabilities.reliability,
                )
            };

            // Collect adapter info
            let info = AdapterInfo {
                adapter_id: adapter_id.clone(),
                adapter_type: format!("{:?}", capabilities.adapter_type),
                active: true,
                is_backhaul,
                bandwidth_bps,
                latency_ms,
                reliability,
                privacy_level: estimate_privacy_level(&capabilities.adapter_type),
            };

            adapters.push(info);
        }

        Ok(adapters)
    }

    /// Sign a heartbeat message
    ///
    /// SECURITY H3: Include public_key in signed message
    fn sign_heartbeat(&self, heartbeat: &HeartbeatMessage) -> Result<Vec<u8>> {
        // SECURITY H3: Build message to sign
        // Message format: node_id || timestamp || adapters || geolocation || public_key
        let mut message = Vec::new();
        message.extend_from_slice(heartbeat.node_id.as_bytes());
        message.extend_from_slice(&heartbeat.timestamp.to_be_bytes());

        // Serialize adapters (deterministic)
        let adapters_json = serde_json::to_vec(&heartbeat.adapters)?;
        message.extend_from_slice(&adapters_json);

        // Include geolocation if present
        if let Some(geo) = &heartbeat.geolocation {
            let geo_json = serde_json::to_vec(geo)?;
            message.extend_from_slice(&geo_json);
        }

        // SECURITY H3: Include public key in signed message
        message.extend_from_slice(&heartbeat.public_key);

        // Sign with node's keypair
        let signature = sign_message(&self.identity, &message)?;

        Ok(signature.as_bytes().to_vec())
    }

    /// Start the heartbeat service
    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Heartbeat service disabled");
            return Ok(());
        }

        info!("Starting heartbeat service...");
        info!(
            "  Heartbeat interval: {} seconds",
            self.config.interval_secs
        );
        info!("  Node timeout: {} seconds", self.config.timeout_secs);
        info!("  Geolocation sharing: {}", self.config.include_geolocation);

        // Start heartbeat broadcasting task
        let config = self.config.clone();
        let local_node_id = self.local_node_id;
        let identity = Arc::clone(&self.identity);
        let adapter_manager = Arc::clone(&self.adapter_manager);
        let backhaul_detector = Arc::clone(&self.backhaul_detector);
        let adapter_configs = self.adapter_configs.clone();

        // Clone self for the closure
        let service = HeartbeatService {
            config: config.clone(),
            local_node_id,
            node_map: Arc::clone(&self.node_map),
            identity: Arc::clone(&identity),
            adapter_manager: Arc::clone(&adapter_manager),
            backhaul_detector: Arc::clone(&backhaul_detector),
            rate_limiter: Arc::clone(&self.rate_limiter),
            adapter_configs: adapter_configs.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            broadcast_task: Arc::new(RwLock::new(None)),
            cleanup_task: Arc::new(RwLock::new(None)),
        };

        // RESOURCE M4: Subscribe to shutdown channel
        let mut shutdown_rx1 = self.shutdown_tx.subscribe();

        // RESOURCE M4: Spawn broadcast task with shutdown handling
        let handle1 = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(config.interval_secs));

            loop {
                tokio::select! {
                    _ = shutdown_rx1.recv() => {
                        info!("Heartbeat broadcast task shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        // Collect adapter information
                        match service.collect_adapter_info().await {
                    Ok(adapters) => {
                        if adapters.is_empty() {
                            debug!("No adapters available for heartbeat broadcast");
                            continue;
                        }

                        debug!("Broadcasting heartbeat with {} adapters", adapters.len());

                        // SECURITY H3: Extract public key
                        let public_key_bytes = identity.public_key.as_ref().to_vec();

                        // Generate heartbeat
                        let mut heartbeat = HeartbeatMessage {
                            node_id: local_node_id,
                            timestamp: current_timestamp(),
                            adapters,
                            geolocation: None, // TODO: Implement geolocation collection
                            public_key: public_key_bytes,
                            signature: Vec::new(),
                        };

                        // Sign heartbeat
                        match service.sign_heartbeat(&heartbeat) {
                            Ok(signature) => {
                                heartbeat.signature = signature;

                                // TODO: Broadcast via all eligible adapters
                                // For now, just log that we would broadcast
                                debug!(
                                    "Would broadcast signed heartbeat (signature: {} bytes)",
                                    heartbeat.signature.len()
                                );
                            }
                            Err(e) => {
                                warn!("Failed to sign heartbeat: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to collect adapter info: {}", e);
                    }
                }
                    }
                }
            }
        });

        // RESOURCE M4: Store broadcast task handle
        *self.broadcast_task.write().await = Some(handle1);

        // RESOURCE M4: Start NodeMap cleanup task with shutdown handling
        let node_map = Arc::clone(&self.node_map);
        let timeout_secs = self.config.timeout_secs;
        let mut shutdown_rx2 = self.shutdown_tx.subscribe();

        let handle2 = tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(60)); // Check every minute

            loop {
                tokio::select! {
                    _ = shutdown_rx2.recv() => {
                        info!("Heartbeat cleanup task shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        let mut map = node_map.write().await;
                        let before = map.len();

                        // Remove stale nodes
                        map.retain(|_, node_info| !node_info.is_stale(timeout_secs));

                        let after = map.len();
                        if before != after {
                            debug!("Cleaned up {} stale nodes from NodeMap", before - after);
                        }
                    }
                }
            }
        });

        // RESOURCE M4: Store cleanup task handle
        *self.cleanup_task.write().await = Some(handle2);

        Ok(())
    }

    /// Gracefully shutdown the heartbeat service and wait for tasks to complete
    /// RESOURCE M4: Prevents task handle leaks and ensures cleanup
    pub async fn stop(&self) {
        info!("Stopping heartbeat service...");

        // Send shutdown signal to all tasks
        let _ = self.shutdown_tx.send(());

        // Wait for broadcast task to complete
        if let Some(handle) = self.broadcast_task.write().await.take() {
            let _ = handle.await;
        }

        // Wait for cleanup task to complete
        if let Some(handle) = self.cleanup_task.write().await.take() {
            let _ = handle.await;
        }
    }

    /// Handle an incoming heartbeat from another node
    ///
    /// SECURITY H3: Verify cryptographic signature to prevent route poisoning
    pub async fn handle_heartbeat(&self, heartbeat: HeartbeatMessage) -> Result<()> {
        debug!("Received heartbeat from node {:?}", heartbeat.node_id);

        // SECURITY H3: Verify public key is provided
        if heartbeat.public_key.is_empty() {
            bail!("Heartbeat missing public key");
        }
        if heartbeat.public_key.len() != 32 {
            bail!("Invalid public key length: {}", heartbeat.public_key.len());
        }

        // SECURITY H3: Verify signature is provided
        if heartbeat.signature.is_empty() {
            bail!("Heartbeat missing signature");
        }
        if heartbeat.signature.len() != 64 {
            bail!("Invalid signature length: {}", heartbeat.signature.len());
        }

        // SECURITY H3: Verify that public_key derives to claimed node_id
        // This prevents impersonation attacks where attacker uses valid signature
        // from one node but claims to be a different node
        let mut hasher = Blake2b512::new();
        hasher.update(&heartbeat.public_key);
        let derived_id = hasher.finalize();
        let derived_id_bytes: [u8; 64] = derived_id.into();
        let derived_node_id = NodeId::from_bytes(derived_id_bytes);

        if derived_node_id != heartbeat.node_id {
            bail!("Public key derivation mismatch: public key does not derive to claimed node_id");
        }

        // SECURITY H3: Reconstruct signed message
        // Message format: node_id || timestamp || adapters || geolocation || public_key
        let mut message = Vec::new();
        message.extend_from_slice(heartbeat.node_id.as_bytes());
        message.extend_from_slice(&heartbeat.timestamp.to_be_bytes());
        let adapters_json = serde_json::to_vec(&heartbeat.adapters)?;
        message.extend_from_slice(&adapters_json);
        if let Some(geo) = &heartbeat.geolocation {
            let geo_json = serde_json::to_vec(geo)?;
            message.extend_from_slice(&geo_json);
        }
        message.extend_from_slice(&heartbeat.public_key);

        // SECURITY H3: Parse Ed25519 public key
        let public_key = ed25519::PublicKey::from_slice(&heartbeat.public_key)
            .ok_or_else(|| anyhow::anyhow!("Invalid Ed25519 public key"))?;

        // SECURITY H3: Parse signature
        let signature_bytes: [u8; 64] = heartbeat
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?;
        let signature = Signature::from_bytes(signature_bytes);

        // SECURITY H3: Verify signature
        verify_signature(&public_key, &message, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?;

        // Check timestamp freshness (replay protection)
        let now = current_timestamp();
        let age = (now as i64 - heartbeat.timestamp as i64).abs();
        if age > 300 {
            bail!(
                "Heartbeat timestamp too old or too far in future (age: {}s)",
                age
            );
        }

        // Check rate limiting
        if !self.rate_limiter.write().await.allow(&heartbeat.node_id) {
            warn!("Rate limiting heartbeat from {:?}", heartbeat.node_id);
            return Ok(());
        }

        let mut map = self.node_map.write().await;

        // Check if we're at capacity
        if map.len() >= self.config.max_nodes && !map.contains_key(&heartbeat.node_id) {
            warn!(
                "NodeMap at capacity ({}), ignoring heartbeat",
                self.config.max_nodes
            );
            return Ok(());
        }

        // Update or create node info
        let node_info = map
            .entry(heartbeat.node_id)
            .or_insert_with(|| NodeInfo::new(heartbeat.node_id));

        // Update from heartbeat
        node_info.update_from_heartbeat(&heartbeat);

        // Privacy control: don't store geolocation if not permitted
        if !self.config.store_remote_geolocation {
            node_info.geolocation = None;
        }

        debug!(
            "Updated NodeMap: node {:?} (adapters: {}, heartbeats: {})",
            heartbeat.node_id,
            node_info.adapters.len(),
            node_info.heartbeat_count
        );

        Ok(())
    }

    /// Generate a heartbeat message from local node state
    ///
    /// SECURITY H3: Sign heartbeat to prevent route poisoning
    pub async fn generate_heartbeat(
        &self,
        adapters: Vec<AdapterInfo>,
        geolocation: Option<GeolocationData>,
    ) -> Result<HeartbeatMessage> {
        let timestamp = current_timestamp();

        // Privacy control: only include geolocation if permitted
        let geo = if self.config.include_geolocation {
            geolocation
        } else {
            None
        };

        // SECURITY H3: Extract public key bytes
        let public_key_bytes = self.identity.public_key.as_ref().to_vec();

        // SECURITY H3: Build message to sign
        // Message format: node_id || timestamp || adapters || geolocation || public_key
        let mut message = Vec::new();
        message.extend_from_slice(self.local_node_id.as_bytes());
        message.extend_from_slice(&timestamp.to_be_bytes());
        let adapters_json = serde_json::to_vec(&adapters)?;
        message.extend_from_slice(&adapters_json);
        if let Some(geo) = &geo {
            let geo_json = serde_json::to_vec(geo)?;
            message.extend_from_slice(&geo_json);
        }
        message.extend_from_slice(&public_key_bytes);

        // SECURITY H3: Sign the message
        let signature = sign_message(&self.identity, &message)?;

        let heartbeat = HeartbeatMessage {
            node_id: self.local_node_id,
            timestamp,
            adapters,
            geolocation: geo,
            public_key: public_key_bytes,
            signature: signature.as_bytes().to_vec(),
        };

        Ok(heartbeat)
    }

    /// Get the current NodeMap (read-only access)
    pub async fn get_node_map(&self) -> NodeMap {
        self.node_map.read().await.clone()
    }

    /// Get information about a specific node
    pub async fn get_node_info(&self, node_id: &NodeId) -> Option<NodeInfo> {
        self.node_map.read().await.get(node_id).cloned()
    }

    /// Get list of all known nodes
    pub async fn get_known_nodes(&self) -> Vec<NodeId> {
        self.node_map.read().await.keys().copied().collect()
    }

    /// Get nodes with specific adapter types available
    pub async fn find_nodes_with_adapter(&self, adapter_type: &str) -> Vec<NodeId> {
        let map = self.node_map.read().await;

        map.iter()
            .filter(|(_, info)| {
                info.adapters
                    .iter()
                    .any(|a| a.adapter_type == adapter_type && a.active)
            })
            .map(|(node_id, _)| *node_id)
            .collect()
    }

    /// Get nodes within a geographic area (requires geolocation)
    pub async fn find_nodes_near(
        &self,
        latitude: f64,
        longitude: f64,
        radius_km: f64,
    ) -> Vec<NodeId> {
        if !self.config.store_remote_geolocation {
            return Vec::new();
        }

        let map = self.node_map.read().await;

        map.iter()
            .filter(|(_, info)| {
                if let Some(geo) = &info.geolocation {
                    haversine_distance(latitude, longitude, geo.latitude, geo.longitude)
                        <= radius_km
                } else {
                    false
                }
            })
            .map(|(node_id, _)| *node_id)
            .collect()
    }

    /// Get statistics about the NodeMap
    pub async fn get_stats(&self) -> NodeMapStats {
        let map = self.node_map.read().await;

        let total_nodes = map.len();
        let nodes_with_location = map.values().filter(|n| n.geolocation.is_some()).count();

        let mut adapter_counts: HashMap<String, usize> = HashMap::new();
        for node_info in map.values() {
            for adapter in &node_info.adapters {
                if adapter.active {
                    *adapter_counts
                        .entry(adapter.adapter_type.clone())
                        .or_insert(0) += 1;
                }
            }
        }

        NodeMapStats {
            total_nodes,
            nodes_with_location,
            adapter_counts,
        }
    }
}

/// Statistics about the NodeMap
#[derive(Debug, Clone)]
pub struct NodeMapStats {
    pub total_nodes: usize,
    pub nodes_with_location: usize,
    pub adapter_counts: HashMap<String, usize>,
}

/// Get current Unix timestamp (seconds) with graceful fallback on system time errors
///
/// SECURITY: If system clock goes backwards or other time errors occur,
/// returns a fallback timestamp instead of panicking. This is better than
/// crashing the node during heartbeat processing.
fn current_timestamp() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(e) => {
            eprintln!(
                "WARNING: System time error in heartbeat processing: {}. Using fallback timestamp.",
                e
            );
            // Return a reasonable fallback (1.5 billion seconds since epoch, ~2017)
            1500000000
        }
    }
}

/// Estimate privacy level for an adapter type
/// Returns value from 0.0 (traceable/IP-based) to 1.0 (anonymous)
fn estimate_privacy_level(adapter_type: &myriadmesh_protocol::types::AdapterType) -> f64 {
    use myriadmesh_protocol::types::AdapterType;

    match adapter_type {
        // IP-based adapters are traceable
        AdapterType::Ethernet => 0.15,
        AdapterType::WiFiHaLoW => 0.15,
        AdapterType::Cellular => 0.10, // Most traceable

        // Bluetooth has MAC addresses but short range
        AdapterType::Bluetooth => 0.30,
        AdapterType::BluetoothLE => 0.30,

        // LoRa has limited traceability
        AdapterType::LoRaWAN => 0.50,
        AdapterType::Meshtastic => 0.50,

        // I2P is anonymous
        AdapterType::I2P => 0.95,

        // Other wireless with varying privacy
        AdapterType::APRS => 0.40,
        AdapterType::FRSGMRS => 0.35,
        AdapterType::CBRadio => 0.35,
        AdapterType::Shortwave => 0.30,
        AdapterType::Dialup => 0.20,
        AdapterType::PPPoE => 0.15,

        // Unknown defaults to low privacy
        _ => 0.20,
    }
}

/// Calculate distance between two geographic coordinates using Haversine formula
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in kilometers

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lon = (lon2 - lon1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    r * c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backhaul::{BackhaulConfig, BackhaulDetector};
    use myriadmesh_crypto::identity::NodeIdentity;
    use myriadmesh_network::AdapterManager;
    use myriadmesh_protocol::types::NODE_ID_SIZE;
    use std::collections::HashMap;

    // Helper function to create a HeartbeatService for testing
    fn create_test_service(config: HeartbeatConfig, node_id: NodeId) -> HeartbeatService {
        myriadmesh_crypto::init().ok();
        let identity = Arc::new(NodeIdentity::generate().unwrap());
        let adapter_manager = Arc::new(RwLock::new(AdapterManager::new()));
        let backhaul_detector = Arc::new(BackhaulDetector::new(BackhaulConfig::default()));
        let adapter_configs = HashMap::new();

        HeartbeatService::new(
            config,
            node_id,
            identity,
            adapter_manager,
            backhaul_detector,
            adapter_configs,
        )
    }

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval_secs, 60);
        assert_eq!(config.timeout_secs, 300);
        assert!(!config.include_geolocation); // Privacy-first
        assert!(!config.store_remote_geolocation); // Privacy-first
    }

    #[test]
    fn test_node_info_staleness() {
        let node_id = NodeId::from_bytes([0u8; NODE_ID_SIZE]);
        let mut node_info = NodeInfo::new(node_id);

        // Fresh node should not be stale
        assert!(!node_info.is_stale(300));

        // Set last_seen to 10 minutes ago
        node_info.last_seen = current_timestamp() - 600;

        // Should be stale with 5 minute timeout
        assert!(node_info.is_stale(300));

        // Should not be stale with 15 minute timeout
        assert!(!node_info.is_stale(900));
    }

    #[test]
    fn test_haversine_distance() {
        // Distance between New York and Los Angeles
        let ny_lat = 40.7128;
        let ny_lon = -74.0060;
        let la_lat = 34.0522;
        let la_lon = -118.2437;

        let distance = haversine_distance(ny_lat, ny_lon, la_lat, la_lon);

        // Should be approximately 3944 km
        assert!((distance - 3944.0).abs() < 50.0);
    }

    #[test]
    fn test_privacy_controls() {
        // Privacy-first config should not include geolocation
        let config = HeartbeatConfig::default();
        assert!(!config.include_geolocation);
        assert!(!config.store_remote_geolocation);

        // Explicit config can enable geolocation
        let config_with_geo = HeartbeatConfig {
            include_geolocation: true,
            store_remote_geolocation: true,
            ..Default::default()
        };
        assert!(config_with_geo.include_geolocation);
        assert!(config_with_geo.store_remote_geolocation);
    }

    #[tokio::test]
    async fn test_heartbeat_service_creation() {
        let config = HeartbeatConfig::default();
        let node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let service = create_test_service(config, node_id);

        assert_eq!(service.local_node_id, node_id);
    }

    #[tokio::test]
    async fn test_handle_heartbeat() -> Result<()> {
        myriadmesh_crypto::init().ok();
        let identity = NodeIdentity::generate()?;
        let remote_node_id = NodeId::from_bytes(*identity.node_id.as_bytes());

        let config = HeartbeatConfig::default();
        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        let adapters = vec![AdapterInfo {
            adapter_id: "ethernet".to_string(),
            adapter_type: "ethernet".to_string(),
            active: true,
            is_backhaul: false,
            bandwidth_bps: 100_000_000,
            latency_ms: 10,
            reliability: 0.99,
            privacy_level: 0.15,
        }];

        let timestamp = current_timestamp();
        let public_key_bytes = identity.public_key.as_ref().to_vec();

        // Build message to sign
        let mut message = Vec::new();
        message.extend_from_slice(remote_node_id.as_bytes());
        message.extend_from_slice(&timestamp.to_be_bytes());
        message.extend_from_slice(&serde_json::to_vec(&adapters)?);
        message.extend_from_slice(&public_key_bytes);

        // Sign message
        let signature = sign_message(&identity, &message)?;

        let heartbeat = HeartbeatMessage {
            node_id: remote_node_id,
            timestamp,
            adapters,
            geolocation: None,
            public_key: public_key_bytes,
            signature: signature.as_bytes().to_vec(),
        };

        service.handle_heartbeat(heartbeat).await?;

        // Verify node was added to map
        let node_info = service.get_node_info(&remote_node_id).await;
        assert!(node_info.is_some());

        let info = node_info.unwrap();
        assert_eq!(info.adapters.len(), 1);
        assert_eq!(info.heartbeat_count, 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_privacy_location_filtering() -> Result<()> {
        // Config that doesn't store remote geolocation
        myriadmesh_crypto::init().ok();
        let identity = NodeIdentity::generate()?;
        let remote_node_id = NodeId::from_bytes(*identity.node_id.as_bytes());

        let config = HeartbeatConfig {
            store_remote_geolocation: false,
            ..Default::default()
        };

        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        let geolocation = Some(GeolocationData {
            latitude: 40.7128,
            longitude: -74.0060,
            accuracy_meters: 100.0,
            country_code: Some("US".to_string()),
            city: Some("New York".to_string()),
        });

        let timestamp = current_timestamp();
        let public_key_bytes = identity.public_key.as_ref().to_vec();

        // Build message to sign
        let mut message = Vec::new();
        message.extend_from_slice(remote_node_id.as_bytes());
        message.extend_from_slice(&timestamp.to_be_bytes());
        message.extend_from_slice(&serde_json::to_vec(&Vec::<AdapterInfo>::new())?);
        message.extend_from_slice(&serde_json::to_vec(&geolocation)?);
        message.extend_from_slice(&public_key_bytes);

        // Sign message
        let signature = sign_message(&identity, &message)?;

        let heartbeat = HeartbeatMessage {
            node_id: remote_node_id,
            timestamp,
            adapters: Vec::new(),
            geolocation,
            public_key: public_key_bytes,
            signature: signature.as_bytes().to_vec(),
        };

        service.handle_heartbeat(heartbeat).await?;

        // Verify geolocation was NOT stored due to privacy settings
        let node_info = service.get_node_info(&remote_node_id).await;
        assert!(node_info.is_some());
        assert!(node_info.unwrap().geolocation.is_none());
        Ok(())
    }

    // SECURITY H3: Route poisoning prevention tests

    #[tokio::test]
    async fn test_heartbeat_missing_public_key_rejected() {
        // SECURITY H3: Verify heartbeats without public key are rejected
        let config = HeartbeatConfig::default();
        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let remote_node_id = NodeId::from_bytes([2u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        let heartbeat = HeartbeatMessage {
            node_id: remote_node_id,
            timestamp: current_timestamp(),
            adapters: Vec::new(),
            geolocation: None,
            public_key: Vec::new(), // Missing public key
            signature: vec![0u8; 64],
        };

        let result = service.handle_heartbeat(heartbeat).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing public key"));
    }

    #[tokio::test]
    async fn test_heartbeat_invalid_signature_rejected() -> Result<()> {
        // SECURITY H3: Verify heartbeats with invalid signatures are rejected
        myriadmesh_crypto::init().ok();
        let identity = NodeIdentity::generate()?;
        let node_id = NodeId::from_bytes(*identity.node_id.as_bytes());

        let config = HeartbeatConfig::default();
        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        let heartbeat = HeartbeatMessage {
            node_id,
            timestamp: current_timestamp(),
            adapters: Vec::new(),
            geolocation: None,
            public_key: identity.public_key.as_ref().to_vec(),
            signature: vec![0u8; 64], // Invalid signature
        };

        let result = service.handle_heartbeat(heartbeat).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Signature verification failed"));
        Ok(())
    }

    #[tokio::test]
    async fn test_heartbeat_public_key_derivation_mismatch() -> Result<()> {
        // SECURITY H3: Verify heartbeats with wrong public key are rejected
        myriadmesh_crypto::init().ok();
        let identity1 = NodeIdentity::generate()?;
        let identity2 = NodeIdentity::generate()?;

        let config = HeartbeatConfig::default();
        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        let node_id1 = NodeId::from_bytes(*identity1.node_id.as_bytes());

        // Build message
        let mut message = Vec::new();
        message.extend_from_slice(node_id1.as_bytes());
        message.extend_from_slice(&current_timestamp().to_be_bytes());
        message.extend_from_slice(&serde_json::to_vec(&Vec::<AdapterInfo>::new())?);
        message.extend_from_slice(identity2.public_key.as_ref());

        // Sign with identity2 but claim to be identity1
        let signature = sign_message(&identity2, &message)?;

        let heartbeat = HeartbeatMessage {
            node_id: node_id1, // Claim to be identity1
            timestamp: current_timestamp(),
            adapters: Vec::new(),
            geolocation: None,
            public_key: identity2.public_key.as_ref().to_vec(), // But use identity2's public key
            signature: signature.as_bytes().to_vec(),
        };

        let result = service.handle_heartbeat(heartbeat).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Public key derivation mismatch"));
        Ok(())
    }

    #[tokio::test]
    async fn test_heartbeat_tampered_data_rejected() -> Result<()> {
        // SECURITY H3: Verify tampered heartbeats are rejected
        myriadmesh_crypto::init().ok();
        let identity = NodeIdentity::generate()?;
        let node_id = NodeId::from_bytes(*identity.node_id.as_bytes());

        let config = HeartbeatConfig::default();
        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        // Create and sign original heartbeat
        let original_adapters = vec![AdapterInfo {
            adapter_id: "eth0".to_string(),
            adapter_type: "ethernet".to_string(),
            active: true,
            is_backhaul: false,
            bandwidth_bps: 100_000_000,
            latency_ms: 10,
            reliability: 0.99,
            privacy_level: 0.15,
        }];

        let timestamp = current_timestamp();
        let public_key_bytes = identity.public_key.as_ref().to_vec();

        // Build original message
        let mut message = Vec::new();
        message.extend_from_slice(node_id.as_bytes());
        message.extend_from_slice(&timestamp.to_be_bytes());
        message.extend_from_slice(&serde_json::to_vec(&original_adapters)?);
        message.extend_from_slice(&public_key_bytes);

        // Sign original message
        let signature = sign_message(&identity, &message)?;

        // Tamper with adapters after signing
        let tampered_adapters = vec![AdapterInfo {
            adapter_id: "eth0".to_string(),
            adapter_type: "ethernet".to_string(),
            active: true,
            is_backhaul: true,            // Changed
            bandwidth_bps: 1_000_000_000, // Changed
            latency_ms: 10,
            reliability: 0.99,
            privacy_level: 0.15,
        }];

        let heartbeat = HeartbeatMessage {
            node_id,
            timestamp,
            adapters: tampered_adapters, // Tampered data
            geolocation: None,
            public_key: public_key_bytes,
            signature: signature.as_bytes().to_vec(), // Original signature
        };

        let result = service.handle_heartbeat(heartbeat).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Signature verification failed"));
        Ok(())
    }

    #[tokio::test]
    async fn test_valid_signed_heartbeat_accepted() -> Result<()> {
        // SECURITY H3: Verify properly signed heartbeats are accepted
        myriadmesh_crypto::init().ok();
        let identity = NodeIdentity::generate()?;
        let node_id = NodeId::from_bytes(*identity.node_id.as_bytes());

        let config = HeartbeatConfig::default();
        let local_node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);

        let service = create_test_service(config, local_node_id);

        // Create heartbeat
        let adapters = vec![AdapterInfo {
            adapter_id: "eth0".to_string(),
            adapter_type: "ethernet".to_string(),
            active: true,
            is_backhaul: false,
            bandwidth_bps: 100_000_000,
            latency_ms: 10,
            reliability: 0.99,
            privacy_level: 0.15,
        }];

        let timestamp = current_timestamp();
        let public_key_bytes = identity.public_key.as_ref().to_vec();

        // Build message to sign
        let mut message = Vec::new();
        message.extend_from_slice(node_id.as_bytes());
        message.extend_from_slice(&timestamp.to_be_bytes());
        message.extend_from_slice(&serde_json::to_vec(&adapters)?);
        message.extend_from_slice(&public_key_bytes);

        // Sign message
        let signature = sign_message(&identity, &message)?;

        let heartbeat = HeartbeatMessage {
            node_id,
            timestamp,
            adapters,
            geolocation: None,
            public_key: public_key_bytes,
            signature: signature.as_bytes().to_vec(),
        };

        // Should be accepted
        let result = service.handle_heartbeat(heartbeat).await;
        assert!(result.is_ok());

        // Verify node was added to map
        let node_info = service.get_node_info(&node_id).await;
        assert!(node_info.is_some());
        Ok(())
    }
}
