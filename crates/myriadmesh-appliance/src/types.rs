//! Common types and error definitions for the appliance module

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result type for appliance operations
pub type ApplianceResult<T> = Result<T, ApplianceError>;

/// Errors that can occur in appliance operations
#[derive(Debug, Error)]
pub enum ApplianceError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Device already paired: {0}")]
    DeviceAlreadyPaired(String),

    #[error("Maximum paired devices reached: {0}")]
    MaxDevicesReached(usize),

    #[error("Invalid pairing token: {0}")]
    InvalidPairingToken(String),

    #[error("Pairing expired")]
    PairingExpired,

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Cache full: cannot store more messages")]
    CacheFull,

    #[error("Message not found: {0}")]
    MessageNotFound(String),

    #[error("Invalid message priority: {0}")]
    InvalidPriority(u8),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}

/// Appliance capabilities advertisement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplianceCapabilities {
    /// Maximum number of devices that can pair
    pub max_paired_devices: usize,
    /// Current number of paired devices
    pub current_paired_devices: usize,
    /// Message caching enabled
    pub message_caching: bool,
    /// Maximum messages that can be cached per device
    pub max_cache_messages_per_device: usize,
    /// Relay/proxy functionality enabled
    pub relay_enabled: bool,
    /// Bridge functionality enabled
    pub bridge_enabled: bool,
    /// Pairing currently available
    pub pairing_available: bool,
    /// Supported pairing methods
    pub pairing_methods: Vec<String>,
}

/// Device configuration preferences
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DevicePreferences {
    /// Routing preferences
    pub routing: RoutingPreferences,
    /// Message preferences
    pub messages: MessagePreferences,
    /// Power management preferences
    pub power: PowerPreferences,
    /// Privacy and security preferences
    pub privacy: PrivacyPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPreferences {
    /// Default routing policy: "privacy", "performance", "reliability", "balanced"
    pub default_policy: String,
    /// Adapter priority order
    pub adapter_priority: Vec<String>,
    /// Default QoS class
    pub qos_class_default: String,
    /// Enable multipath routing
    pub multipath_enabled: bool,
    /// Enable geographic routing
    pub geographic_routing_enabled: bool,
}

impl Default for RoutingPreferences {
    fn default() -> Self {
        Self {
            default_policy: "balanced".to_string(),
            adapter_priority: vec![
                "i2p".to_string(),
                "wifi".to_string(),
                "cellular".to_string(),
            ],
            qos_class_default: "normal".to_string(),
            multipath_enabled: true,
            geographic_routing_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePreferences {
    /// Cache messages on appliance
    pub cache_on_appliance: bool,
    /// Default cache priority
    pub cache_priority_default: String,
    /// Auto-forward messages to appliance
    pub auto_forward_to_appliance: bool,
    /// Store-and-forward enabled
    pub store_and_forward: bool,
    /// Message TTL in days
    pub ttl_days: u32,
}

impl Default for MessagePreferences {
    fn default() -> Self {
        Self {
            cache_on_appliance: true,
            cache_priority_default: "normal".to_string(),
            auto_forward_to_appliance: true,
            store_and_forward: true,
            ttl_days: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerPreferences {
    /// Offload DHT operations to appliance
    pub offload_dht_to_appliance: bool,
    /// Offload ledger sync to appliance
    pub offload_ledger_sync: bool,
    /// Heartbeat interval on mobile (seconds)
    pub mobile_heartbeat_interval: u64,
    /// Use appliance as proxy to save power
    pub appliance_as_proxy: bool,
}

impl Default for PowerPreferences {
    fn default() -> Self {
        Self {
            offload_dht_to_appliance: true,
            offload_ledger_sync: true,
            mobile_heartbeat_interval: 300,
            appliance_as_proxy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPreferences {
    /// Always use i2p via appliance
    pub always_use_i2p_via_appliance: bool,
    /// Allow clearnet on mobile
    pub clearnet_allowed_on_mobile: bool,
    /// Require appliance for sensitive messages
    pub require_appliance_for_sensitive: bool,
    /// Only communicate with trusted nodes
    pub trusted_nodes_only: bool,
}

impl Default for PrivacyPreferences {
    fn default() -> Self {
        Self {
            always_use_i2p_via_appliance: false,
            clearnet_allowed_on_mobile: true,
            require_appliance_for_sensitive: true,
            trusted_nodes_only: false,
        }
    }
}
