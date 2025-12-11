//! MyriadMesh Appliance Module
//!
//! This module provides appliance functionality for MyriadMesh nodes, enabling them to act as
//! gateways and caching proxies for mobile devices.
//!
//! # Features
//!
//! - **Device Pairing**: Secure QR code and PIN-based pairing with mobile devices
//! - **Message Caching**: Store-and-forward messaging with priority queues
//! - **Configuration Sync**: Synchronize preferences and routing policies
//! - **Relay & Bridge**: Proxy routing for mobile devices

pub mod cache;
pub mod device;
pub mod manager;
pub mod pairing;
pub mod power;
pub mod types;

// Re-export commonly used types
pub use cache::{CachedMessage, MessageCache, MessageCacheConfig, MessagePriority};
pub use device::{PairedDevice, PairedDeviceInfo};
pub use manager::{ApplianceManager, ApplianceManagerConfig, ApplianceStats};
pub use pairing::{PairingMethod, PairingRequest, PairingResponse, PairingResult, PairingToken};
pub use power::{
    BatteryThreshold, DataUsagePolicy, DataUsageTracker, PowerAction, PowerManager,
    PowerManagerConfig, PowerSupply, QuotaCheck, ResetPeriod,
};
pub use types::{ApplianceCapabilities, ApplianceError, ApplianceResult};
