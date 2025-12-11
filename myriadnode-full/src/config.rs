use anyhow::{Context, Result};
use myriadmesh_crypto::identity::NodeIdentity;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub node: NodeConfig,
    pub api: ApiConfig,
    pub dht: DhtConfig,
    pub ledger: LedgerConfig,
    pub network: NetworkConfig,
    pub security: SecurityConfig,
    pub i2p: I2pConfig,
    pub routing: RoutingConfig,
    pub logging: LoggingConfig,
    pub heartbeat: HeartbeatConfig,
    pub appliance: ApplianceConfig,
    pub updates: UpdateConfig,
    pub storage: StorageConfig,
    pub emergency_abuse_prevention: EmergencyAbusePrevention,
    pub advanced_features: AdvancedFeaturesConfig,

    #[serde(skip)]
    config_file_path: PathBuf,
    #[serde(skip)]
    pub data_directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    #[serde(with = "hex_bytes")]
    pub id: Vec<u8>,
    pub name: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub enabled: bool,
    pub bind: String,
    pub port: u16,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub enabled: bool,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtConfig {
    pub enabled: bool,
    pub bootstrap_nodes: Vec<String>,
    pub port: u16,
    pub cache_messages: bool,
    pub cache_ttl_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerConfig {
    pub enabled: bool,
    pub keep_blocks: u64,
    pub min_reputation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub adapters: AdapterConfigs,
    pub monitoring: MonitoringConfig,
    pub failover: FailoverConfig,
    pub scoring: ScoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfigs {
    pub ethernet: AdapterConfig,
    pub bluetooth: AdapterConfig,
    pub bluetooth_le: AdapterConfig,
    pub cellular: AdapterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    /// Allow mesh networking on backhaul interfaces (IP adapters only)
    #[serde(default)]
    pub allow_backhaul_mesh: bool,
    /// Allow heartbeat broadcasting on this adapter
    #[serde(default = "default_allow_heartbeat")]
    pub allow_heartbeat: bool,
    /// Override heartbeat interval for this adapter (optional)
    #[serde(default)]
    pub heartbeat_interval_override: Option<u64>,
}

fn default_allow_heartbeat() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub ping_interval_secs: u64,
    pub throughput_interval_secs: u64,
    pub reliability_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    pub auto_failover: bool,
    pub latency_threshold_multiplier: f32,
    pub loss_threshold: f32,
    pub retry_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    pub mode: String, // "default", "battery", "performance", "reliability", "privacy"
    pub weight_latency: f64,
    pub weight_bandwidth: f64,
    pub weight_reliability: f64,
    pub weight_power: f64,
    pub weight_privacy: f64,
    #[serde(default = "default_recalculation_interval")]
    pub recalculation_interval_secs: u64,
}

fn default_recalculation_interval() -> u64 {
    60 // Recalculate scores every minute
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub require_signatures: bool,
    pub trusted_nodes_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I2pConfig {
    pub enabled: bool,
    pub sam_host: String,
    pub sam_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub max_hops: u32,
    pub store_and_forward: bool,
    pub message_ttl_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub include_geolocation: bool,
    pub store_remote_geolocation: bool,
    pub max_nodes: usize,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 60,
            timeout_secs: 300,
            include_geolocation: false,
            store_remote_geolocation: false,
            max_nodes: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplianceConfig {
    /// Enable appliance mode (gateway/caching for mobile devices)
    pub enabled: bool,
    /// Maximum number of devices that can pair with this appliance
    pub max_paired_devices: usize,
    /// Enable message caching for paired devices
    pub message_caching: bool,
    /// Maximum number of messages to cache per device
    pub max_cache_messages_per_device: usize,
    /// Maximum total cached messages across all devices
    pub max_total_cache_messages: usize,
    /// Enable relay/proxy functionality for paired devices
    pub enable_relay: bool,
    /// Enable bridge functionality (connect different network segments)
    pub enable_bridge: bool,
    /// Require manual approval for pairing requests
    pub require_pairing_approval: bool,
    /// Pairing methods supported: "qr_code", "pin"
    pub pairing_methods: Vec<String>,
    /// mDNS service advertisement for local discovery
    pub mdns_enabled: bool,
    /// Publish appliance availability to DHT
    pub dht_advertisement: bool,
}

impl Default for ApplianceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_paired_devices: 10,
            message_caching: true,
            max_cache_messages_per_device: 1000,
            max_total_cache_messages: 10000,
            enable_relay: true,
            enable_bridge: true,
            require_pairing_approval: true,
            pairing_methods: vec!["qr_code".to_string(), "pin".to_string()],
            mdns_enabled: true,
            dht_advertisement: true,
        }
    }
}

/// Update system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Enable automatic update checking
    pub enabled: bool,
    /// Enable automatic update installation (requires enabled=true)
    pub auto_install: bool,
    /// Require manual approval for non-critical updates
    pub require_approval: bool,
    /// Verification period in hours (time to wait for peer signatures)
    pub verification_period_hours: u64,
    /// Minimum number of trusted peer signatures required
    pub min_trusted_signatures: usize,
    /// Minimum reputation threshold for trusted signers
    pub min_trust_reputation: f64,
    /// Automatically create update schedules
    pub auto_schedule: bool,
    /// Check for updates interval in hours
    pub check_interval_hours: u64,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_install: false,
            require_approval: true,
            verification_period_hours: 6,
            min_trusted_signatures: 3,
            min_trust_reputation: 0.8,
            auto_schedule: true,
            check_interval_hours: 24,
        }
    }
}

/// Storage persistence configuration - granular control over what data is stored
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Enable compression for stored data (zstd)
    pub compression_enabled: bool,
    /// Compression level (1-22, default: 3 for balanced speed/ratio)
    #[serde(default = "default_compression_level")]
    pub compression_level: i32,

    /// Persist DHT routing table snapshots
    pub persist_dht: bool,
    /// Persist outbound message queue (for crash recovery)
    pub persist_message_queue: bool,
    /// Persist sent message log (for audit trail)
    pub persist_sent_messages: bool,
    /// Persist received messages (for local inbox)
    pub persist_received_messages: bool,
    /// Persist appliance offline cache
    pub persist_appliance_cache: bool,
    /// Persist ledger MESSAGE entries
    pub persist_ledger_messages: bool,
    /// Persist metrics and diagnostics
    pub persist_metrics: bool,

    /// Auto-cleanup delivered messages after N days (0 = immediate)
    #[serde(default = "default_cleanup_after_days")]
    pub cleanup_delivered_after_days: u32,
    /// Auto-cleanup failed messages after N days
    #[serde(default = "default_cleanup_failed_days")]
    pub cleanup_failed_after_days: u32,
    /// Auto-cleanup metrics older than N days
    #[serde(default = "default_metrics_retention_days")]
    pub metrics_retention_days: u32,

    /// Storage mode: "minimal", "balanced", "full"
    #[serde(default = "default_storage_mode")]
    pub mode: String,
}

fn default_compression_level() -> i32 {
    3 // Balanced: ~3x compression, fast
}

fn default_cleanup_after_days() -> u32 {
    0 // Delete delivered messages immediately
}

fn default_cleanup_failed_days() -> u32 {
    7 // Keep failed messages for a week
}

fn default_metrics_retention_days() -> u32 {
    30 // Keep metrics for 30 days
}

fn default_storage_mode() -> String {
    "balanced".to_string()
}

/// Emergency abuse prevention system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyAbusePrevention {
    /// Enable emergency abuse prevention system
    pub enabled: bool,

    /// Enable size-based penalty modulation
    pub size_modulation_enabled: bool,

    /// Infrequent sender allowance (messages before enforcement)
    pub infrequent_allowance: u32,

    /// High reputation threshold for bypassing quotas
    pub high_reputation_threshold: f64,

    /// Enable bandwidth-aware exemptions
    pub bandwidth_exemption_enabled: bool,

    /// High-speed threshold for bandwidth exemption (bits per second)
    pub high_speed_threshold_bps: u64,

    /// Unused bandwidth threshold for exemption (0.0 - 1.0)
    pub unused_bandwidth_threshold: f64,

    /// Bandwidth sampling window (seconds)
    pub bandwidth_sampling_window_secs: u64,

    /// Consensus configuration for Global realm
    pub consensus: ConsensusValidatorConfig,
}

impl Default for EmergencyAbusePrevention {
    fn default() -> Self {
        Self {
            enabled: true,
            size_modulation_enabled: true,
            infrequent_allowance: 3,
            high_reputation_threshold: 0.8,
            bandwidth_exemption_enabled: true,
            high_speed_threshold_bps: 10_000_000, // 10 Mbps
            unused_bandwidth_threshold: 0.6, // 60% unused
            bandwidth_sampling_window_secs: 60,
            consensus: ConsensusValidatorConfig::default(),
        }
    }
}

/// Consensus validator configuration for K-of-N consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusValidatorConfig {
    /// Enable consensus validation
    pub enabled: bool,

    /// Required confirmations (K)
    pub required_confirmations: u32,

    /// Total validators to query (N)
    pub total_validators: u32,

    /// Timeout for consensus requests (seconds)
    pub timeout_secs: u64,

    /// Use DHT for validator discovery (vs manual list)
    pub use_dht_discovery: bool,
}

impl Default for ConsensusValidatorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            required_confirmations: 3,
            total_validators: 5,
            timeout_secs: 10,
            use_dht_discovery: true,
        }
    }
}

/// Advanced features configuration for Phase 4
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedFeaturesConfig {
    /// Consensus protocol configuration
    pub consensus: ConsensusProtocolConfig,

    /// Relay scoring configuration
    pub relay_scoring: RelayScoringConfig,

    /// Failure detection configuration
    pub failure_detection: FailureDetectionConfig,
}

impl Default for AdvancedFeaturesConfig {
    fn default() -> Self {
        Self {
            consensus: ConsensusProtocolConfig::default(),
            relay_scoring: RelayScoringConfig::default(),
            failure_detection: FailureDetectionConfig::default(),
        }
    }
}

/// Consensus protocol configuration for relay selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusProtocolConfig {
    /// Enable consensus protocol
    pub enabled: bool,

    /// Minimum participants (default: 7 for f=2 Byzantine faults)
    pub min_participants: usize,

    /// Maximum participants (default: 21 for f=6)
    pub max_participants: usize,

    /// Quorum threshold (default: 0.67 for >2/3)
    pub quorum_threshold: f32,

    /// Proposal timeout (seconds)
    pub proposal_timeout_secs: u64,

    /// Voting timeout (seconds)
    pub voting_timeout_secs: u64,

    /// Commit timeout (seconds)
    pub commit_timeout_secs: u64,

    /// Require cryptographic signatures
    pub require_signatures: bool,
}

impl Default for ConsensusProtocolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_participants: 7,
            max_participants: 21,
            quorum_threshold: 0.67,
            proposal_timeout_secs: 10,
            voting_timeout_secs: 10,
            commit_timeout_secs: 5,
            require_signatures: true,
        }
    }
}

/// Relay scoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayScoringConfig {
    /// Enable relay scoring
    pub enabled: bool,

    /// Score update interval (seconds)
    pub update_interval_secs: u64,

    /// Metrics window (seconds)
    pub metrics_window_secs: u64,

    /// Minimum viable score (0-100)
    pub min_viable_score: f32,

    /// Sticky session duration (seconds)
    pub sticky_session_secs: u64,

    /// Top relays count for selection
    pub top_relays_count: usize,

    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker reset timeout (seconds)
    pub circuit_breaker_reset_secs: u64,
}

impl Default for RelayScoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            update_interval_secs: 60,
            metrics_window_secs: 300,
            min_viable_score: 30.0,
            sticky_session_secs: 3600,
            top_relays_count: 3,
            circuit_breaker_threshold: 5,
            circuit_breaker_reset_secs: 300,
        }
    }
}

/// Failure detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureDetectionConfig {
    /// Enable failure detection
    pub enabled: bool,

    /// Missed heartbeat threshold
    pub missed_heartbeat_threshold: u32,

    /// Heartbeat interval (seconds)
    pub heartbeat_interval_secs: u64,

    /// Grace period before declaring down (seconds)
    pub grace_period_secs: u64,

    /// Consensus peers count
    pub consensus_peers_count: usize,

    /// Consensus threshold (0.0-1.0)
    pub consensus_threshold: f32,

    /// Consensus timeout (seconds)
    pub consensus_timeout_secs: u64,

    /// Recovery confirmation count
    pub recovery_confirmation_count: u32,
}

impl Default for FailureDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            missed_heartbeat_threshold: 3,
            heartbeat_interval_secs: 60,
            grace_period_secs: 180,
            consensus_peers_count: 7,
            consensus_threshold: 0.67,
            consensus_timeout_secs: 10,
            recovery_confirmation_count: 2,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            compression_enabled: true,
            compression_level: 3,
            persist_dht: false, // DHT rebuilds on startup
            persist_message_queue: true, // Crash recovery
            persist_sent_messages: false, // Not needed for normal operation
            persist_received_messages: true, // Local inbox
            persist_appliance_cache: true, // Critical for appliance mode
            persist_ledger_messages: true, // Audit trail
            persist_metrics: true, // Network monitoring
            cleanup_delivered_after_days: 0, // Immediate cleanup
            cleanup_failed_after_days: 7,
            metrics_retention_days: 30,
            mode: "balanced".to_string(),
        }
    }
}

impl StorageConfig {
    /// Get storage mode preset
    pub fn from_mode(mode: &str) -> Self {
        match mode {
            "minimal" => Self {
                compression_enabled: true,
                compression_level: 3,
                persist_dht: false,
                persist_message_queue: true, // Always keep for crash recovery
                persist_sent_messages: false,
                persist_received_messages: false, // Route-only
                persist_appliance_cache: true, // Keep if appliance mode
                persist_ledger_messages: false,
                persist_metrics: false,
                cleanup_delivered_after_days: 0,
                cleanup_failed_after_days: 1,
                metrics_retention_days: 7,
                mode: "minimal".to_string(),
            },
            "full" => Self {
                compression_enabled: true,
                compression_level: 1, // Fast compression for write-heavy
                persist_dht: true,
                persist_message_queue: true,
                persist_sent_messages: true,
                persist_received_messages: true,
                persist_appliance_cache: true,
                persist_ledger_messages: true,
                persist_metrics: true,
                cleanup_delivered_after_days: 90, // Archive for 3 months
                cleanup_failed_after_days: 30,
                metrics_retention_days: 365,
                mode: "full".to_string(),
            },
            _ => Self::default(), // "balanced"
        }
    }
}

impl Config {
    /// Load configuration from file or use defaults
    pub fn load(config_path: Option<PathBuf>, data_dir: Option<PathBuf>) -> Result<Self> {
        let config_path = config_path.unwrap_or_else(Self::default_config_path);
        let data_dir = data_dir.unwrap_or_else(Self::default_data_dir);

        if !config_path.exists() {
            anyhow::bail!(
                "Configuration file not found: {}\nRun with --init to create a new configuration",
                config_path.display()
            );
        }

        let contents =
            fs::read_to_string(&config_path).context("Failed to read configuration file")?;

        let mut config: Config =
            serde_yaml::from_str(&contents).context("Failed to parse configuration file")?;

        config.config_file_path = config_path;
        config.data_directory = data_dir;

        Ok(config)
    }

    /// Create a new default configuration
    pub fn create_default(config_path: Option<PathBuf>, data_dir: Option<PathBuf>) -> Result<Self> {
        let config_path = config_path.unwrap_or_else(Self::default_config_path);
        let data_dir = data_dir.unwrap_or_else(Self::default_data_dir);

        // Create directories
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&data_dir)?;

        // Initialize crypto library
        myriadmesh_crypto::init()?;

        // Generate node identity
        let identity = NodeIdentity::generate()?;
        let node_id = identity.node_id.as_bytes().to_vec();

        // Save identity keys
        let key_dir = data_dir.join("keys");
        fs::create_dir_all(&key_dir)?;

        let private_key_path = key_dir.join("node.key");
        let public_key_path = key_dir.join("node.pub");

        fs::write(&private_key_path, identity.export_secret_key())?;
        fs::write(&public_key_path, identity.export_public_key())?;

        // Create default config
        let config = Config {
            node: NodeConfig {
                id: node_id,
                name: format!("myriad-{}", hex::encode(&identity.node_id.as_bytes()[..4])),
                primary: true,
            },
            api: ApiConfig {
                enabled: true,
                bind: "127.0.0.1".to_string(),
                port: 8080,
                auth: AuthConfig {
                    enabled: false,
                    token: None,
                },
            },
            dht: DhtConfig {
                enabled: true,
                bootstrap_nodes: vec![],
                port: 4001,
                cache_messages: true,
                cache_ttl_days: 7,
            },
            ledger: LedgerConfig {
                enabled: true,
                keep_blocks: 1000,
                min_reputation: 0.5,
            },
            network: NetworkConfig {
                adapters: AdapterConfigs {
                    ethernet: AdapterConfig {
                        enabled: true,
                        auto_start: true,
                        allow_backhaul_mesh: false,
                        allow_heartbeat: true,
                        heartbeat_interval_override: None,
                    },
                    bluetooth: AdapterConfig {
                        enabled: false,
                        auto_start: false,
                        allow_backhaul_mesh: false,
                        allow_heartbeat: true,
                        heartbeat_interval_override: None,
                    },
                    bluetooth_le: AdapterConfig {
                        enabled: false,
                        auto_start: false,
                        allow_backhaul_mesh: false,
                        allow_heartbeat: true,
                        heartbeat_interval_override: None,
                    },
                    cellular: AdapterConfig {
                        enabled: false,
                        auto_start: false,
                        allow_backhaul_mesh: false,
                        allow_heartbeat: false, // Don't use cellular for heartbeats
                        heartbeat_interval_override: None,
                    },
                },
                monitoring: MonitoringConfig {
                    ping_interval_secs: 300,
                    throughput_interval_secs: 1800,
                    reliability_interval_secs: 3600,
                },
                failover: FailoverConfig {
                    auto_failover: true,
                    latency_threshold_multiplier: 5.0,
                    loss_threshold: 0.25,
                    retry_attempts: 3,
                },
                scoring: ScoringConfig {
                    mode: "default".to_string(),
                    weight_latency: 0.25,
                    weight_bandwidth: 0.20,
                    weight_reliability: 0.30,
                    weight_power: 0.10,
                    weight_privacy: 0.15,
                    recalculation_interval_secs: 60,
                },
            },
            security: SecurityConfig {
                require_signatures: true,
                trusted_nodes_only: false,
            },
            i2p: I2pConfig {
                enabled: true,
                sam_host: "127.0.0.1".to_string(),
                sam_port: 7656,
            },
            routing: RoutingConfig {
                max_hops: 10,
                store_and_forward: true,
                message_ttl_days: 7,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                file: Some(data_dir.join("logs").join("myriadnode.log")),
            },
            heartbeat: HeartbeatConfig {
                enabled: true,
                interval_secs: 60,
                timeout_secs: 300,
                include_geolocation: false,
                store_remote_geolocation: false,
                max_nodes: 1000,
            },
            appliance: ApplianceConfig::default(),
            updates: UpdateConfig::default(),
            storage: StorageConfig::default(),
            emergency_abuse_prevention: EmergencyAbusePrevention::default(),
            advanced_features: AdvancedFeaturesConfig::default(),
            config_file_path: config_path.clone(),
            data_directory: data_dir,
        };

        // Save configuration
        let yaml = serde_yaml::to_string(&config)?;
        fs::write(&config_path, yaml)?;

        Ok(config)
    }

    pub fn config_path(&self) -> &Path {
        &self.config_file_path
    }

    fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("myriadnode")
            .join("config.yaml")
    }

    fn default_data_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("myriadnode")
    }
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(s).map_err(serde::de::Error::custom)
    }
}
