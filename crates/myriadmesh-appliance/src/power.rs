//! Power Management System for MyriadMesh
//!
//! Provides adaptive power scaling, battery monitoring, and data usage tracking
//! for resource-constrained devices.

use myriadmesh_protocol::types::AdapterType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Power supply type for the device
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PowerSupply {
    /// Mains AC power - always available, no restrictions
    #[default]
    ACMains,

    /// Power over Ethernet with budget management
    PoE {
        /// Total available power in watts
        available_watts: f32,
        /// Reserved power for other devices in watts
        reserved_watts: f32,
    },

    /// Battery-powered device with capacity tracking
    Battery {
        /// Total battery capacity in milliwatt-hours
        capacity_mwh: u32,
        /// Current battery level in milliwatt-hours
        current_mwh: u32,
        /// Charging rate in mWh per hour
        charge_rate_mwh_per_hour: f32,
        /// Discharge rate in mWh per hour
        discharge_rate_mwh_per_hour: f32,
        /// Low power threshold percentage (default: 20%)
        low_power_threshold_percent: u8,
        /// Critical threshold percentage (default: 5%)
        critical_threshold_percent: u8,
    },
}

impl PowerSupply {
    /// Get current battery percentage (0-100)
    pub fn battery_percent(&self) -> Option<u8> {
        match self {
            Self::Battery {
                capacity_mwh,
                current_mwh,
                ..
            } => {
                if *capacity_mwh == 0 {
                    Some(0)
                } else {
                    Some(((*current_mwh as f32 / *capacity_mwh as f32) * 100.0) as u8)
                }
            }
            _ => None,
        }
    }

    /// Check if battery is low
    pub fn is_low_battery(&self) -> bool {
        match self {
            Self::Battery {
                low_power_threshold_percent,
                ..
            } => {
                if let Some(percent) = self.battery_percent() {
                    percent <= *low_power_threshold_percent
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Check if battery is critical
    pub fn is_critical_battery(&self) -> bool {
        match self {
            Self::Battery {
                critical_threshold_percent,
                ..
            } => {
                if let Some(percent) = self.battery_percent() {
                    percent <= *critical_threshold_percent
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Get available power budget in watts
    pub fn available_power_watts(&self) -> f32 {
        match self {
            Self::ACMains => f32::MAX,
            Self::PoE {
                available_watts,
                reserved_watts,
            } => available_watts - reserved_watts,
            Self::Battery { .. } => {
                // Estimate based on battery percentage
                if let Some(percent) = self.battery_percent() {
                    // Scale down power as battery depletes
                    (percent as f32 / 100.0) * 10.0 // Assume max 10W from battery
                } else {
                    5.0 // Default conservative estimate
                }
            }
        }
    }
}

/// Power action to take at specific battery thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum PowerAction {
    /// No restrictions, full power
    FullPower,
    /// Reduce transmit power by specified percentage
    ReducePower { reduction_percent: u8 },
    /// Disable the adapter entirely
    DisableAdapter,
    /// Listen-only mode (no transmission)
    ListenOnly,
}

/// Battery threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryThreshold {
    /// Battery percentage threshold
    pub threshold_percent: u8,
    /// Action to take when battery drops to or below this level
    pub action: PowerAction,
}

/// Power management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerManagerConfig {
    /// Power supply configuration
    pub supply: PowerSupply,
    /// TX power scaling table: battery_percent -> max_tx_power_dbm
    #[serde(default)]
    pub power_scaling_table: HashMap<u8, u8>,
    /// Adapter availability thresholds
    #[serde(default)]
    pub adapter_thresholds: HashMap<AdapterType, Vec<BatteryThreshold>>,
}

impl Default for PowerManagerConfig {
    fn default() -> Self {
        let mut power_scaling_table = HashMap::new();
        power_scaling_table.insert(100, 30); // 100% battery = 30 dBm
        power_scaling_table.insert(50, 25); // 50% battery = 25 dBm
        power_scaling_table.insert(20, 20); // 20% battery = 20 dBm
        power_scaling_table.insert(5, 10); // 5% battery = 10 dBm

        Self {
            supply: PowerSupply::default(),
            power_scaling_table,
            adapter_thresholds: HashMap::new(),
        }
    }
}

/// Power manager for adaptive power scaling
pub struct PowerManager {
    config: Arc<RwLock<PowerManagerConfig>>,
}

impl PowerManager {
    /// Create a new power manager
    pub fn new(config: PowerManagerConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Update battery state (for battery-powered devices)
    pub async fn update_battery_state(&self, current_mwh: u32) -> Result<(), String> {
        let mut config = self.config.write().await;
        if let PowerSupply::Battery {
            current_mwh: ref mut current,
            ..
        } = config.supply
        {
            *current = current_mwh;
            Ok(())
        } else {
            Err("Not a battery-powered device".to_string())
        }
    }

    /// Get current power budget for an adapter in watts
    pub async fn get_power_budget(&self, _adapter: AdapterType) -> Result<u32, String> {
        let config = self.config.read().await;
        Ok(config.supply.available_power_watts() as u32)
    }

    /// Check if an adapter should be active at current power level
    pub async fn is_adapter_active(&self, adapter: AdapterType) -> Result<bool, String> {
        let config = self.config.read().await;

        // Check adapter-specific thresholds
        if let Some(thresholds) = config.adapter_thresholds.get(&adapter) {
            if let Some(battery_percent) = config.supply.battery_percent() {
                for threshold in thresholds {
                    if battery_percent <= threshold.threshold_percent {
                        return Ok(matches!(
                            threshold.action,
                            PowerAction::FullPower | PowerAction::ReducePower { .. }
                        ));
                    }
                }
            }
        }

        // Default: active unless critical battery
        Ok(!config.supply.is_critical_battery())
    }

    /// Get current TX power scaling factor (0.0 - 1.0)
    pub async fn get_power_scaling(&self) -> f64 {
        let config = self.config.read().await;

        if let Some(battery_percent) = config.supply.battery_percent() {
            // Find the closest power scaling entry
            let mut closest_percent = 100u8;
            let mut closest_power = 30u8;

            for (&threshold, &power) in &config.power_scaling_table {
                if threshold <= battery_percent
                    && (battery_percent - threshold) < (battery_percent - closest_percent)
                {
                    closest_percent = threshold;
                    closest_power = power;
                }
            }

            // Return scaling factor relative to maximum
            (closest_power as f64) / 30.0
        } else {
            1.0 // Full power for non-battery devices
        }
    }

    /// Get maximum TX power in dBm for current battery level
    pub async fn get_max_tx_power_dbm(&self) -> u8 {
        let config = self.config.read().await;

        if let Some(battery_percent) = config.supply.battery_percent() {
            // Find the appropriate power level
            let mut result = 30u8; // Default max

            for (&threshold, &power) in &config.power_scaling_table {
                if battery_percent >= threshold {
                    result = result.max(power);
                } else {
                    result = result.min(power);
                }
            }

            result
        } else {
            30 // Full power for non-battery devices
        }
    }

    /// Notify of power threshold crossing
    pub async fn on_power_threshold_crossed(&self, threshold: u8) {
        log::warn!("Power threshold crossed: {}%", threshold);
        // Future: Send notifications, trigger power saving modes
    }

    /// Get current power supply information
    pub async fn get_power_supply(&self) -> PowerSupply {
        self.config.read().await.supply.clone()
    }

    /// Check if device is running on battery
    pub async fn is_battery_powered(&self) -> bool {
        matches!(self.config.read().await.supply, PowerSupply::Battery { .. })
    }
}

/// Data usage tracking period
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResetPeriod {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
}

impl ResetPeriod {
    /// Get period duration in seconds
    pub fn duration_secs(&self) -> u64 {
        match self {
            Self::Daily => 86400,       // 24 hours
            Self::Weekly => 604800,     // 7 days
            Self::Monthly => 2592000,   // 30 days (approximate)
            Self::Quarterly => 7776000, // 90 days (approximate)
        }
    }
}

/// Data usage policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataUsagePolicy {
    /// Enable data limiting (default: false)
    pub enabled: bool,
    /// Warn threshold in MB (default: 1000 = 1GB)
    pub warn_threshold_mb: u32,
    /// Hard limit in MB (0 = unlimited)
    pub hard_limit_mb: u32,
    /// Reset period
    pub reset_period: ResetPeriod,
    /// Period start timestamp
    pub period_start: u64,
}

impl Default for DataUsagePolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            warn_threshold_mb: 1000, // 1GB
            hard_limit_mb: 0,        // Unlimited
            reset_period: ResetPeriod::Monthly,
            period_start: now(),
        }
    }
}

/// Data usage check result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaCheck {
    /// Transmission allowed
    Allow,
    /// Warning threshold reached but transmission allowed
    WarnButAllow,
    /// Hard limit exceeded, transmission denied
    Denied,
}

/// Data usage tracker
pub struct DataUsageTracker {
    policy: Arc<RwLock<DataUsagePolicy>>,
    usage_bytes: Arc<AtomicU32>,
}

impl DataUsageTracker {
    /// Create a new data usage tracker
    pub fn new(policy: DataUsagePolicy) -> Self {
        Self {
            policy: Arc::new(RwLock::new(policy)),
            usage_bytes: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Check if transmission quota is available
    pub async fn check_quota(&self, size_bytes: u32) -> Result<QuotaCheck, String> {
        let policy = self.policy.read().await;

        if !policy.enabled {
            return Ok(QuotaCheck::Allow);
        }

        let current_mb = self.get_usage_mb().await;
        let size_mb = size_bytes / (1024 * 1024);
        let new_total_mb = current_mb + size_mb;

        if policy.hard_limit_mb > 0 && new_total_mb > policy.hard_limit_mb {
            Ok(QuotaCheck::Denied)
        } else if new_total_mb > policy.warn_threshold_mb {
            log::warn!("Data usage warning: {} MB used", new_total_mb);
            Ok(QuotaCheck::WarnButAllow)
        } else {
            Ok(QuotaCheck::Allow)
        }
    }

    /// Add usage to the tracker
    pub async fn add_usage(&self, bytes: u64) {
        self.usage_bytes.fetch_add(bytes as u32, Ordering::Relaxed);

        // Check if period reset is needed
        self.reset_if_needed().await;
    }

    /// Get current usage in MB
    pub async fn get_usage_mb(&self) -> u32 {
        self.usage_bytes.load(Ordering::Relaxed) / (1024 * 1024)
    }

    /// Get remaining quota in MB
    pub async fn get_remaining_mb(&self) -> u32 {
        let policy = self.policy.read().await;

        if !policy.enabled || policy.hard_limit_mb == 0 {
            return u32::MAX; // Unlimited
        }

        let current_mb = self.get_usage_mb().await;
        policy.hard_limit_mb.saturating_sub(current_mb)
    }

    /// Reset usage if period has elapsed
    pub async fn reset_if_needed(&self) {
        let mut policy = self.policy.write().await;
        let current_time = now();

        if current_time >= policy.period_start + policy.reset_period.duration_secs() {
            self.usage_bytes.store(0, Ordering::Relaxed);
            policy.period_start = current_time;
            log::info!("Data usage reset for new period");
        }
    }

    /// Manually reset usage
    pub async fn reset(&self) {
        self.usage_bytes.store(0, Ordering::Relaxed);
        let mut policy = self.policy.write().await;
        policy.period_start = now();
    }

    /// Get current policy
    pub async fn get_policy(&self) -> DataUsagePolicy {
        self.policy.read().await.clone()
    }

    /// Update policy
    pub async fn update_policy(&self, new_policy: DataUsagePolicy) {
        *self.policy.write().await = new_policy;
    }
}

/// Get current Unix timestamp
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_supply_battery_percent() {
        let supply = PowerSupply::Battery {
            capacity_mwh: 5000,
            current_mwh: 2500,
            charge_rate_mwh_per_hour: 1000.0,
            discharge_rate_mwh_per_hour: 500.0,
            low_power_threshold_percent: 20,
            critical_threshold_percent: 5,
        };

        assert_eq!(supply.battery_percent(), Some(50));
    }

    #[test]
    fn test_power_supply_low_battery() {
        let supply = PowerSupply::Battery {
            capacity_mwh: 5000,
            current_mwh: 500, // 10%
            charge_rate_mwh_per_hour: 1000.0,
            discharge_rate_mwh_per_hour: 500.0,
            low_power_threshold_percent: 20,
            critical_threshold_percent: 5,
        };

        assert!(supply.is_low_battery());
        assert!(!supply.is_critical_battery());
    }

    #[test]
    fn test_power_supply_critical_battery() {
        let supply = PowerSupply::Battery {
            capacity_mwh: 5000,
            current_mwh: 200, // 4%
            charge_rate_mwh_per_hour: 1000.0,
            discharge_rate_mwh_per_hour: 500.0,
            low_power_threshold_percent: 20,
            critical_threshold_percent: 5,
        };

        assert!(supply.is_low_battery());
        assert!(supply.is_critical_battery());
    }

    #[tokio::test]
    async fn test_power_manager_scaling() {
        let config = PowerManagerConfig::default();
        let manager = PowerManager::new(config);

        let scaling = manager.get_power_scaling().await;
        assert_eq!(scaling, 1.0); // AC mains = full power
    }

    #[tokio::test]
    async fn test_data_usage_tracker() {
        let policy = DataUsagePolicy {
            enabled: true,
            warn_threshold_mb: 10,
            hard_limit_mb: 20,
            reset_period: ResetPeriod::Daily,
            period_start: now(),
        };

        let tracker = DataUsageTracker::new(policy);

        // Add 5 MB
        tracker.add_usage(5 * 1024 * 1024).await;
        assert_eq!(tracker.get_usage_mb().await, 5);

        // Check quota for 3 MB - should allow
        let check = tracker.check_quota(3 * 1024 * 1024).await.unwrap();
        assert_eq!(check, QuotaCheck::Allow);

        // Add 6 MB more (total 11 MB) - should warn
        tracker.add_usage(6 * 1024 * 1024).await;
        let check = tracker.check_quota(1).await.unwrap();
        assert_eq!(check, QuotaCheck::WarnButAllow);

        // Try to add 10 MB more (would be 21 MB) - should deny
        let check = tracker.check_quota(10 * 1024 * 1024).await.unwrap();
        assert_eq!(check, QuotaCheck::Denied);
    }

    #[test]
    fn test_reset_period_duration() {
        assert_eq!(ResetPeriod::Daily.duration_secs(), 86400);
        assert_eq!(ResetPeriod::Weekly.duration_secs(), 604800);
        assert_eq!(ResetPeriod::Monthly.duration_secs(), 2592000);
        assert_eq!(ResetPeriod::Quarterly.duration_secs(), 7776000);
    }
}
