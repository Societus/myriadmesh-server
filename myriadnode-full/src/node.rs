use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::api::ApiServer;
use crate::backhaul::{BackhaulConfig, BackhaulDetector};
use crate::config::Config;
use crate::failover::FailoverManager;
use crate::heartbeat::HeartbeatService;
use crate::monitor::NetworkMonitor;
use crate::scoring::ScoringWeights;
use crate::storage::Storage;

use myriadmesh_appliance::{ApplianceManager, ApplianceManagerConfig, MessageCacheConfig};
use myriadmesh_crypto::identity::NodeIdentity;
use myriadmesh_dht::routing_table::RoutingTable;
use myriadmesh_ledger::ChainSync;
use myriadmesh_network::{adapters::*, AdapterManager, NetworkAdapter};
use myriadmesh_protocol::{message::Message, types::NODE_ID_SIZE, NodeId};
use myriadmesh_routing::{
    BandwidthMonitor, BandwidthMonitorConfig, ConsensusConfig, ConsensusValidator,
    EmergencyManager, EmergencyManagerConfig, PriorityQueue, Router,
};
use std::collections::HashMap;
use std::fs;

/// Main node orchestrator
pub struct Node {
    config: Config,
    identity: Arc<NodeIdentity>, // SECURITY C3: Node identity for adapter authentication
    storage: Arc<RwLock<Storage>>,
    adapter_manager: Arc<RwLock<AdapterManager>>,
    router: Arc<Router>,
    local_delivery_rx: Option<mpsc::UnboundedReceiver<Message>>,
    #[allow(dead_code)]
    message_queue: PriorityQueue,
    #[allow(dead_code)]
    dht: RoutingTable,
    #[allow(dead_code)]
    ledger: Arc<RwLock<ChainSync>>,
    api_server: Option<ApiServer>,
    appliance_manager: Option<Arc<ApplianceManager>>,
    monitor: NetworkMonitor,
    failover_manager: Arc<FailoverManager>,
    heartbeat_service: Arc<HeartbeatService>,
    bandwidth_monitor: Arc<BandwidthMonitor>,
    diagnostics: crate::diagnostics::DiagnosticsCollector,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: mpsc::Receiver<()>,
}

impl Node {
    pub async fn new(config: Config) -> Result<Self> {
        use crate::diagnostics::{DiagnosticsCollector, InitPhase};
        use std::time::Instant;

        let diagnostics = DiagnosticsCollector::new();
        let init_start = Instant::now();

        info!("Initializing node components...");

        // Initialize storage
        let checkpoint_start = Instant::now();
        let storage = Arc::new(RwLock::new(
            Storage::new(&config.data_directory, config.storage.clone()).await?,
        ));
        let duration = checkpoint_start.elapsed().as_millis() as u64;
        diagnostics
            .checkpoint(InitPhase::StorageInit, duration, true, None)
            .await;
        info!("✓ Storage initialized (mode: {})", config.storage.mode);

        // Initialize adapter manager
        let adapter_manager = Arc::new(RwLock::new(AdapterManager::new()));
        info!("✓ Adapter manager initialized");

        // Initialize message queue
        let message_queue = PriorityQueue::new(1000); // Max 1000 messages per priority level
        info!("✓ Message queue initialized");

        // Initialize DHT
        // SECURITY C6: NodeID is now 64 bytes for collision resistance
        let node_id_bytes: [u8; NODE_ID_SIZE] = config
            .node
            .id
            .as_slice()
            .try_into()
            .expect("Node ID must be 64 bytes");
        let node_id = myriadmesh_protocol::NodeId::from_bytes(node_id_bytes);
        let dht = RoutingTable::new(node_id);
        info!("✓ DHT routing table initialized");

        // Initialize Router with local delivery channel
        // The router handles message routing with DOS protection and rate limiting
        let checkpoint_start = Instant::now();
        let (local_delivery_tx, local_delivery_rx) = Router::create_local_delivery_channel();
        let mut router = Router::new(
            node_id, 1000,  // per-node limit: 1000 msg/min
            10000, // global limit: 10000 msg/min
            1000,  // queue capacity: 1000 messages per priority level
        );
        router.set_local_delivery_channel(local_delivery_tx);
        let duration = checkpoint_start.elapsed().as_millis() as u64;
        diagnostics
            .checkpoint(InitPhase::RouterInit, duration, true, None)
            .await;
        info!("✓ Router initialized");

        // Initialize ledger
        let ledger_dir = config.data_directory.join("ledger");
        fs::create_dir_all(&ledger_dir)?;
        let ledger_config = myriadmesh_ledger::StorageConfig::new(&ledger_dir)
            .with_keep_blocks(config.ledger.keep_blocks);
        let ledger_storage = myriadmesh_ledger::LedgerStorage::new(ledger_config)?;
        let consensus_manager =
            myriadmesh_ledger::ConsensusManager::new(config.ledger.min_reputation);
        let chain_sync = ChainSync::new(ledger_storage, consensus_manager);
        let ledger = Arc::new(RwLock::new(chain_sync));
        info!(
            "✓ Ledger initialized (keep_blocks: {})",
            config.ledger.keep_blocks
        );

        // Set up ledger confirmation callback for router
        // This records message delivery confirmations in the blockchain
        // See: myriadmesh-ledger/src/entry.rs - MessageEntry
        let _ledger_for_callback = Arc::clone(&ledger);
        router.set_confirmation_callback(Arc::new(move |msg_id, src, dest, is_local| {
            // TODO: Implement actual ledger MESSAGE entry creation
            // For now, just log the confirmation
            info!(
                "Message routed: {:?} from {:?} to {:?} (local: {})",
                msg_id, src, dest, is_local
            );
            // Future implementation:
            // let mut ledger = ledger_for_callback.write().await;
            // ledger.add_message_entry(msg_id, src, dest, is_local);
        }));

        // Create smart failover-aware message sender callback for Router
        // This callback uses FailoverManager to select the best adapter based on:
        // - Message priority (Emergency → fastest, Background → cheapest)
        // - Adapter health and scoring
        // - Current network conditions
        let message_sender_callback = Self::create_message_sender(
            Arc::clone(&adapter_manager),
            Arc::clone(&failover_manager),
        );
        router.set_message_sender(message_sender_callback);

        // Initialize emergency abuse prevention system
        info!("Initializing emergency abuse prevention system...");

        // 1. Initialize BandwidthMonitor
        let bw_config = BandwidthMonitorConfig {
            high_speed_threshold_bps: config.emergency_abuse_prevention.high_speed_threshold_bps,
            unused_threshold: config.emergency_abuse_prevention.unused_bandwidth_threshold,
            sampling_window_secs: config.emergency_abuse_prevention.bandwidth_sampling_window_secs,
        };
        let bandwidth_monitor = Arc::new(BandwidthMonitor::new(bw_config));
        info!("✓ BandwidthMonitor initialized (high-speed: {} Mbps, unused threshold: {:.0}%)",
            config.emergency_abuse_prevention.high_speed_threshold_bps / 1_000_000,
            config.emergency_abuse_prevention.unused_bandwidth_threshold * 100.0);

        // 2. Initialize ConsensusValidator (if enabled)
        let consensus_validator = if config.emergency_abuse_prevention.consensus.enabled {
            let consensus_config = ConsensusConfig {
                enabled: config.emergency_abuse_prevention.consensus.enabled,
                required_confirmations: config.emergency_abuse_prevention.consensus.required_confirmations,
                total_validators: config.emergency_abuse_prevention.consensus.total_validators,
                timeout_secs: config.emergency_abuse_prevention.consensus.timeout_secs,
                use_dht_discovery: config.emergency_abuse_prevention.consensus.use_dht_discovery,
                validator_nodes: Vec::new(), // Will be populated via DHT discovery
            };
            let validator = Arc::new(ConsensusValidator::new(consensus_config));
            info!("✓ ConsensusValidator initialized ({}-of-{} consensus)",
                config.emergency_abuse_prevention.consensus.required_confirmations,
                config.emergency_abuse_prevention.consensus.total_validators);
            Some(validator)
        } else {
            info!("  ConsensusValidator disabled");
            None
        };

        // 3. Initialize EmergencyManager
        let em_config = EmergencyManagerConfig {
            enabled: config.emergency_abuse_prevention.enabled,
            size_modulation_enabled: config.emergency_abuse_prevention.size_modulation_enabled,
            infrequent_allowance: config.emergency_abuse_prevention.infrequent_allowance,
            high_reputation_threshold: config.emergency_abuse_prevention.high_reputation_threshold,
            bandwidth_exemption_enabled: config.emergency_abuse_prevention.bandwidth_exemption_enabled,
            high_speed_threshold_bps: config.emergency_abuse_prevention.high_speed_threshold_bps,
            unused_bandwidth_threshold: config.emergency_abuse_prevention.unused_bandwidth_threshold,
        };
        let mut emergency_mgr = EmergencyManager::new(em_config);

        // Wire dependencies to EmergencyManager
        emergency_mgr.set_bandwidth_monitor(Arc::clone(&bandwidth_monitor));
        if let Some(cv) = consensus_validator {
            emergency_mgr.set_consensus_validator(cv);
        }
        // Note: Reputation system integration deferred until ReputationScore is available

        let emergency_mgr = Arc::new(emergency_mgr);
        info!("✓ EmergencyManager initialized (bandwidth exemptions: {}, size modulation: {})",
            config.emergency_abuse_prevention.bandwidth_exemption_enabled,
            config.emergency_abuse_prevention.size_modulation_enabled);

        // 4. Wire EmergencyManager to Router
        router.set_emergency_manager(Arc::clone(&emergency_mgr));
        info!("✓ Emergency abuse prevention system integrated with router");

        // Wrap router in Arc for shared ownership
        // Router uses internal locks for thread-safety, no outer RwLock needed
        let router = Arc::new(router);
        diagnostics
            .checkpoint(InitPhase::RouterCallbacksSet, 0, true, None)
            .await;
        info!("✓ Router fully configured with message sender and ledger callback");

        // Initialize network monitor
        let monitor = NetworkMonitor::new(
            config.network.monitoring.clone(),
            Arc::clone(&adapter_manager),
            Arc::clone(&storage),
        );
        info!("✓ Network monitor initialized");

        // Initialize failover manager
        let scoring_weights = match config.network.scoring.mode.as_str() {
            "battery" => ScoringWeights::battery_optimized(),
            "performance" => ScoringWeights::performance_optimized(),
            "reliability" => ScoringWeights::reliability_optimized(),
            "privacy" => ScoringWeights::privacy_optimized(),
            _ => ScoringWeights::default(),
        };
        let failover_manager = Arc::new(FailoverManager::new(
            config.network.failover.clone(),
            Arc::clone(&adapter_manager),
            scoring_weights,
        ));
        info!(
            "✓ Failover manager initialized (mode: {})",
            config.network.scoring.mode
        );

        // Load node identity
        myriadmesh_crypto::init()?;
        let key_dir = config.data_directory.join("keys");
        let private_key_path = key_dir.join("node.key");
        let public_key_path = key_dir.join("node.pub");

        let identity = if private_key_path.exists() && public_key_path.exists() {
            let secret_bytes = fs::read(&private_key_path)?;
            let public_bytes = fs::read(&public_key_path)?;
            NodeIdentity::from_bytes(&public_bytes, &secret_bytes)?
        } else {
            warn!("Node identity keys not found, generating new identity");
            let new_identity = NodeIdentity::generate()?;
            fs::create_dir_all(&key_dir)?;
            fs::write(&private_key_path, new_identity.export_secret_key())?;
            fs::write(&public_key_path, new_identity.export_public_key())?;
            new_identity
        };
        let identity = Arc::new(identity);
        info!("✓ Node identity loaded");

        // Initialize backhaul detector
        let backhaul_detector = Arc::new(BackhaulDetector::new(BackhaulConfig {
            allow_backhaul_mesh: false,
            check_interval_secs: 300,
        }));
        info!("✓ Backhaul detector initialized");

        // Build adapter configs map
        let mut adapter_configs = HashMap::new();
        adapter_configs.insert(
            "ethernet".to_string(),
            config.network.adapters.ethernet.clone(),
        );
        adapter_configs.insert(
            "bluetooth".to_string(),
            config.network.adapters.bluetooth.clone(),
        );
        adapter_configs.insert(
            "bluetooth_le".to_string(),
            config.network.adapters.bluetooth_le.clone(),
        );
        adapter_configs.insert(
            "cellular".to_string(),
            config.network.adapters.cellular.clone(),
        );

        // Initialize heartbeat service
        let heartbeat_service = Arc::new(HeartbeatService::new(
            crate::heartbeat::HeartbeatConfig {
                enabled: config.heartbeat.enabled,
                interval_secs: config.heartbeat.interval_secs,
                timeout_secs: config.heartbeat.timeout_secs,
                include_geolocation: config.heartbeat.include_geolocation,
                store_remote_geolocation: config.heartbeat.store_remote_geolocation,
                max_nodes: config.heartbeat.max_nodes,
            },
            node_id,
            Arc::clone(&identity),
            Arc::clone(&adapter_manager),
            Arc::clone(&backhaul_detector),
            adapter_configs,
        ));
        info!(
            "✓ Heartbeat service initialized (interval: {}s)",
            config.heartbeat.interval_secs
        );

        // Initialize ApplianceManager if appliance mode is enabled
        let appliance_manager = if config.appliance.enabled {
            info!("Initializing appliance mode...");

            // Convert sodiumoxide secret key to ed25519-dalek SigningKey
            let secret_key_bytes = identity.export_secret_key();
            let signing_key = ed25519_dalek::SigningKey::from_bytes(
                secret_key_bytes
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid secret key length"))?,
            );

            let appliance_config = ApplianceManagerConfig {
                node_id: hex::encode(&config.node.id),
                max_paired_devices: config.appliance.max_paired_devices,
                message_caching: config.appliance.message_caching,
                relay_enabled: config.appliance.enable_relay,
                bridge_enabled: config.appliance.enable_bridge,
                require_pairing_approval: config.appliance.require_pairing_approval,
                pairing_methods: config.appliance.pairing_methods.clone(),
                cache_config: MessageCacheConfig {
                    max_messages_per_device: config.appliance.max_cache_messages_per_device,
                    max_total_messages: config.appliance.max_total_cache_messages,
                },
                data_directory: config.data_directory.join("appliance"),
            };

            let manager = Arc::new(ApplianceManager::new(appliance_config, signing_key).await?);
            info!(
                "✓ Appliance manager initialized (max devices: {}, cache: {})",
                config.appliance.max_paired_devices, config.appliance.max_total_cache_messages
            );
            Some(manager)
        } else {
            info!("Appliance mode disabled");
            None
        };

        // Initialize UpdateCoordinator if updates are enabled
        let update_coordinator = if config.updates.enabled {
            info!("Initializing update coordinator...");

            // Clone the identity for the update coordinator
            let update_identity =
                NodeIdentity::from_keypair(identity.public_key, identity.secret_key.clone());

            let coordinator = Arc::new(myriadmesh_updates::UpdateCoordinator::new(Arc::new(
                update_identity,
            )));

            info!(
                "✓ Update coordinator initialized (auto_install: {}, verification: {}h)",
                config.updates.auto_install, config.updates.verification_period_hours
            );
            Some(coordinator)
        } else {
            info!("Update system disabled");
            None
        };

        // Initialize API server if enabled
        let api_server = if config.api.enabled {
            let server = ApiServer::new(
                config.api.clone(),
                Arc::clone(&adapter_manager),
                Arc::clone(&heartbeat_service),
                Arc::clone(&failover_manager),
                Arc::clone(&ledger),
                appliance_manager.clone(),
                update_coordinator.clone(),
                Some(Arc::clone(&storage)),
                Arc::clone(&router),
                config.storage.clone(),
                hex::encode(&config.node.id),
                config.node.name.clone(),
            )
            .await?;
            info!(
                "✓ API server initialized on {}:{}",
                config.api.bind, config.api.port
            );
            Some(server)
        } else {
            info!("API server disabled");
            None
        };

        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        // Record completion and metadata
        let total_init_duration_ms = init_start.elapsed().as_millis() as u64;
        diagnostics
            .checkpoint(InitPhase::Complete, total_init_duration_ms, true, None)
            .await;

        use crate::diagnostics::StartupMetadata;
        let session_id = diagnostics.session_id().await;
        diagnostics
            .set_startup_metadata(StartupMetadata {
                session_id, // Random UUID, NOT the NodeId (privacy-preserving)
                start_time: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                config_hash: format!("{:?}", config).chars().take(16).collect(), // Simple hash
                version: env!("CARGO_PKG_VERSION").to_string(),
                total_init_duration_ms,
            })
            .await;

        Ok(Self {
            config,
            identity,
            storage,
            adapter_manager,
            router,
            local_delivery_rx: Some(local_delivery_rx),
            message_queue,
            dht,
            ledger,
            api_server,
            appliance_manager,
            monitor,
            failover_manager,
            heartbeat_service,
            bandwidth_monitor,
            diagnostics,
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Create a smart failover-aware message sender callback for the Router
    ///
    /// This callback integrates Router with AdapterManager and FailoverManager to:
    /// 1. Select the best adapter based on message priority and adapter health
    /// 2. Transmit the message via the selected adapter
    /// 3. Return Result<(), String> for Router's retry logic
    ///
    /// # Priority-based Adapter Selection
    /// - Emergency/High: Select based on latency and reliability
    /// - Normal: Balanced scoring
    /// - Low/Background: Select based on cost and power efficiency
    fn create_message_sender(
        adapter_manager: Arc<RwLock<AdapterManager>>,
        failover_manager: Arc<FailoverManager>,
    ) -> myriadmesh_routing::MessageSenderCallback {
        Arc::new(move |destination: NodeId, message: Message| {
            let adapter_manager = Arc::clone(&adapter_manager);
            let failover_manager = Arc::clone(&failover_manager);

            Box::pin(async move {
                // Get priority from message for adapter selection
                let priority = message.priority.as_u8();

                // Select best adapter based on priority and current conditions
                let selected_adapter_id = {
                    let mgr = adapter_manager.read().await;
                    let adapter_ids = mgr.adapter_ids();

                    // TODO: Implement priority-aware adapter scoring in FailoverManager
                    // For now, use the first available adapter
                    // Future: failover_manager.select_best_for_priority(priority, &adapter_ids).await
                    adapter_ids.into_iter().next()
                };

                let adapter_id = selected_adapter_id.ok_or_else(|| {
                    "No adapters available for message transmission".to_string()
                })?;

                // Get adapter and send message
                let mgr = adapter_manager.read().await;
                let adapter_arc = mgr.get_adapter(&adapter_id).ok_or_else(|| {
                    format!("Adapter {} not found", adapter_id)
                })?;

                let adapter = adapter_arc.read().await;

                // Encode message to wire format
                let encoded = bincode::serialize(&message).map_err(|e| {
                    format!("Failed to encode message: {}", e)
                })?;

                // TODO: Implement adapter.send_to(destination, encoded)
                // For now, this is a placeholder
                tracing::debug!(
                    "Would send message {:?} to {:?} via adapter {} ({} bytes)",
                    message.id,
                    destination,
                    adapter_id,
                    encoded.len()
                );

                // Placeholder: Simulate successful send
                // Real implementation will call adapter.send_to(destination, encoded).await
                Ok(())
            }) as Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        use crate::diagnostics::LifecycleEventType;

        self.diagnostics
            .event(LifecycleEventType::NodeStarting, None)
            .await;
        info!("Starting MyriadNode services...");

        // Start network adapters
        self.start_network_adapters().await?;

        // Start API server
        if let Some(api_server) = &self.api_server {
            let server_handle = api_server.start().await?;
            info!("✓ API server running");

            // Store handle for shutdown
            tokio::spawn(async move {
                if let Err(e) = server_handle.await {
                    error!("API server error: {}", e);
                }
            });
        }

        // Start network monitor
        self.monitor.start().await?;
        info!("✓ Network monitor running");

        // Start failover manager
        self.failover_manager.start().await?;
        if self.config.network.failover.auto_failover {
            info!("✓ Automatic failover enabled");
        } else {
            info!("  Automatic failover disabled");
        }

        // Start heartbeat service
        self.heartbeat_service.start().await?;
        if self.config.heartbeat.enabled {
            info!("✓ Heartbeat service running (NodeMap discovery enabled)");
        } else {
            info!("  Heartbeat service disabled");
        }

        // Start router local delivery processor
        // This background task processes messages destined for this node
        if let Some(mut local_delivery_rx) = self.local_delivery_rx.take() {
            let appliance_mgr = self.appliance_manager.clone();
            tokio::spawn(async move {
                info!("Router local delivery processor started");
                while let Some(msg) = local_delivery_rx.recv().await {
                    if let Some(_appliance_mgr) = &appliance_mgr {
                        // Forward to appliance manager if in appliance mode
                        // TODO: Implement proper message handling in appliance manager
                        info!(
                            "Received local message for appliance: {:?} from {:?}",
                            msg.id, msg.source
                        );
                        // Future: appliance_mgr.handle_message(msg).await;
                    } else {
                        // Log message for non-appliance nodes
                        info!(
                            "Received local message: {:?} from {:?} to {:?}",
                            msg.id, msg.source, msg.destination
                        );
                        // TODO: Implement message processing for non-appliance nodes
                        // This could be:
                        // - Passing to application layer
                        // - Storing in local inbox
                        // - Triggering notifications
                    }
                }
                info!("Router local delivery processor stopped");
            });
            info!("✓ Router local delivery processor running");
        }

        // Start router queue processor
        // This background task processes queued messages with exponential backoff retry
        let router_for_processor = Arc::clone(&self.router);
        tokio::spawn(async move {
            router_for_processor.run_queue_processor().await;
        });
        self.diagnostics
            .event(LifecycleEventType::QueueProcessorStarted, None)
            .await;
        info!("✓ Router queue processor running (retry with exponential backoff)");

        // Start DHT (if enabled)
        if self.config.dht.enabled {
            info!("✓ DHT service running");
        }

        // Update router health snapshot
        use crate::diagnostics::RouterHealthSnapshot;
        let router_stats = self.router.get_stats().await;
        self.diagnostics
            .update_router_health(RouterHealthSnapshot {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                has_local_delivery_channel: true,
                has_confirmation_callback: true,
                has_message_sender: false, // Set by adapters later
                has_dht: false,            // TODO: Check if DHT was set
                queue_processor_running: true,
                local_delivery_processor_running: true,
                messages_routed: router_stats.messages_routed,
                messages_queued: 0, // TODO: Get from queue stats
                messages_failed: router_stats.messages_dropped,
            })
            .await;

        self.diagnostics
            .event(LifecycleEventType::NodeReady, None)
            .await;

        info!("═══════════════════════════════════════════════");
        info!("  MyriadNode is now running");
        info!("═══════════════════════════════════════════════");
        if self.config.api.enabled {
            info!(
                "  API: http://{}:{}",
                self.config.api.bind, self.config.api.port
            );
        }
        info!("  Node ID: {}", hex::encode(&self.config.node.id));
        info!("  Data Dir: {}", self.config.data_directory.display());
        info!("═══════════════════════════════════════════════");

        // Wait for shutdown signal
        self.wait_for_shutdown().await;

        info!("Shutting down MyriadNode...");
        self.shutdown().await?;

        Ok(())
    }

    async fn start_network_adapters(&mut self) -> Result<()> {
        info!("Starting network adapters...");

        // Start Ethernet adapter if enabled
        if self.config.network.adapters.ethernet.enabled {
            info!("  Starting Ethernet adapter...");
            match self.initialize_ethernet_adapter().await {
                Ok(()) => info!("  ✓ Ethernet adapter registered"),
                Err(e) => warn!("  ✗ Ethernet adapter failed: {}", e),
            }
        }

        // Start Bluetooth adapter if enabled
        if self.config.network.adapters.bluetooth.enabled {
            info!("  Starting Bluetooth adapter...");
            match self.initialize_bluetooth_adapter().await {
                Ok(()) => info!("  ✓ Bluetooth adapter registered"),
                Err(e) => warn!("  ✗ Bluetooth adapter failed: {}", e),
            }
        }

        // Start Bluetooth LE adapter if enabled
        if self.config.network.adapters.bluetooth_le.enabled {
            info!("  Starting Bluetooth LE adapter...");
            match self.initialize_ble_adapter().await {
                Ok(()) => info!("  ✓ Bluetooth LE adapter registered"),
                Err(e) => warn!("  ✗ Bluetooth LE adapter failed: {}", e),
            }
        }

        // Start Cellular adapter if enabled
        if self.config.network.adapters.cellular.enabled {
            info!("  Starting Cellular adapter...");
            match self.initialize_cellular_adapter().await {
                Ok(()) => info!("  ✓ Cellular adapter registered"),
                Err(e) => warn!("  ✗ Cellular adapter failed: {}", e),
            }
        }

        Ok(())
    }

    async fn initialize_ethernet_adapter(&mut self) -> Result<()> {
        let config = EthernetConfig::default();

        // SECURITY C3: Pass identity for authenticated UDP
        let mut adapter = EthernetAdapter::new(Arc::clone(&self.identity), config);

        adapter.initialize().await?;

        // Check for backhaul status before auto-starting
        if self.config.network.adapters.ethernet.auto_start {
            let backhaul_config = BackhaulConfig {
                allow_backhaul_mesh: self.config.network.adapters.ethernet.allow_backhaul_mesh,
                check_interval_secs: 300,
            };
            let detector = BackhaulDetector::new(backhaul_config);

            // Try to detect all backhaul interfaces
            match detector.detect_all_backhauls() {
                Ok(backhauls) => {
                    if !backhauls.is_empty() {
                        info!("  Detected backhaul interfaces: {:?}", backhauls);

                        if !self.config.network.adapters.ethernet.allow_backhaul_mesh {
                            warn!("  Ethernet adapter may be backhaul - not auto-starting");
                            warn!("  Set 'allow_backhaul_mesh: true' to override");
                        } else {
                            adapter.start().await?;
                        }
                    } else {
                        adapter.start().await?;
                    }
                }
                Err(e) => {
                    warn!("  Failed to detect backhaul status: {}", e);
                    // Start anyway if detection fails
                    adapter.start().await?;
                }
            }
        }

        let mut manager = self.adapter_manager.write().await;
        manager
            .register_adapter("ethernet".to_string(), Box::new(adapter))
            .await?;

        Ok(())
    }

    async fn initialize_bluetooth_adapter(&mut self) -> Result<()> {
        let config = BluetoothConfig::default();
        let mut adapter = BluetoothAdapter::new(config);

        adapter.initialize().await?;

        if self.config.network.adapters.bluetooth.auto_start {
            adapter.start().await?;
        }

        let mut manager = self.adapter_manager.write().await;
        manager
            .register_adapter("bluetooth".to_string(), Box::new(adapter))
            .await?;

        Ok(())
    }

    async fn initialize_ble_adapter(&mut self) -> Result<()> {
        let config = BleConfig::default();
        let mut adapter = BleAdapter::new(config);

        adapter.initialize().await?;

        if self.config.network.adapters.bluetooth_le.auto_start {
            adapter.start().await?;
        }

        let mut manager = self.adapter_manager.write().await;
        manager
            .register_adapter("bluetooth_le".to_string(), Box::new(adapter))
            .await?;

        Ok(())
    }

    async fn initialize_cellular_adapter(&mut self) -> Result<()> {
        let config = CellularConfig::default();
        let mut adapter = CellularAdapter::new(config);

        adapter.initialize().await?;

        if self.config.network.adapters.cellular.auto_start {
            adapter.start().await?;
        }

        let mut manager = self.adapter_manager.write().await;
        manager
            .register_adapter("cellular".to_string(), Box::new(adapter))
            .await?;

        Ok(())
    }

    async fn wait_for_shutdown(&mut self) {
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("Received Ctrl+C signal");
            }
            _ = self.shutdown_rx.recv() => {
                info!("Received shutdown signal");
            }
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        info!("Stopping network monitor...");
        self.monitor.stop().await?;

        info!("Stopping network adapters...");
        {
            let mut manager = self.adapter_manager.write().await;
            if let Err(e) = manager.stop_all().await {
                error!("Failed to stop adapters: {}", e);
            }
        }

        if let Some(_api_server) = &self.api_server {
            info!("Stopping API server...");
            // Server will be dropped and cleaned up
        }

        if let Some(appliance_manager) = &self.appliance_manager {
            info!("Stopping appliance manager...");
            appliance_manager.shutdown().await;
        }

        info!("Closing storage...");
        {
            let storage = self.storage.write().await;
            storage.close().await?;
        }

        info!("Shutdown complete");
        Ok(())
    }

    pub fn shutdown_handle(&self) -> mpsc::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Get diagnostics collector for testing and monitoring
    pub fn diagnostics(&self) -> &crate::diagnostics::DiagnosticsCollector {
        &self.diagnostics
    }
}
