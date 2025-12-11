//! Appliance manager - coordinates all appliance functionality

use crate::cache::{CachedMessage, MessageCache, MessageCacheConfig};
use crate::device::{DeviceStore, PairedDevice, PairedDeviceInfo};
use crate::pairing::{
    PairingManager, PairingRequest, PairingResponse, PairingResult, PairingToken,
};
use crate::types::{ApplianceCapabilities, ApplianceError, ApplianceResult, DevicePreferences};
use blake2::Digest;
use ed25519_dalek::SigningKey;
use myriadmesh_crypto::identity::NodeId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Appliance manager configuration
#[derive(Debug, Clone)]
pub struct ApplianceManagerConfig {
    pub node_id: String,
    pub max_paired_devices: usize,
    pub message_caching: bool,
    pub relay_enabled: bool,
    pub bridge_enabled: bool,
    pub require_pairing_approval: bool,
    pub pairing_methods: Vec<String>,
    pub cache_config: MessageCacheConfig,
    pub data_directory: PathBuf,
}

impl Default for ApplianceManagerConfig {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            max_paired_devices: 10,
            message_caching: true,
            relay_enabled: true,
            bridge_enabled: true,
            require_pairing_approval: true,
            pairing_methods: vec!["qr_code".to_string(), "pin".to_string()],
            cache_config: MessageCacheConfig::default(),
            data_directory: PathBuf::from("./data/appliance"),
        }
    }
}

/// Appliance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplianceStats {
    pub uptime_secs: u64,
    pub paired_devices: usize,
    pub total_cached_messages: usize,
    pub adapters_online: usize,
}

/// Main appliance manager
pub struct ApplianceManager {
    config: ApplianceManagerConfig,
    device_store: Arc<DeviceStore>,
    message_cache: Arc<MessageCache>,
    pairing_manager: Arc<PairingManager>,
    cleanup_task: RwLock<Option<JoinHandle<()>>>,
}

impl ApplianceManager {
    /// Create a new appliance manager
    pub async fn new(
        config: ApplianceManagerConfig,
        signing_key: SigningKey,
    ) -> ApplianceResult<Self> {
        // Ensure data directory exists
        tokio::fs::create_dir_all(&config.data_directory).await?;

        // Initialize device store
        let device_file_path = config.data_directory.join("paired_devices.json");
        let device_store = Arc::new(DeviceStore::new(device_file_path).await?);

        // Initialize message cache
        let cache_file_path = config.data_directory.join("message_cache.json");
        let message_cache =
            Arc::new(MessageCache::new(cache_file_path, config.cache_config.clone()).await?);

        // Initialize pairing manager
        let pairing_manager = Arc::new(PairingManager::new(
            signing_key,
            config.node_id.clone(),
            config.require_pairing_approval,
        ));

        let manager = Self {
            config,
            device_store,
            message_cache,
            pairing_manager,
            cleanup_task: RwLock::new(None),
        };

        // Start cleanup task
        manager.start_cleanup_task().await;

        Ok(manager)
    }

    /// Start periodic cleanup task
    async fn start_cleanup_task(&self) {
        let pairing_manager = self.pairing_manager.clone();
        let message_cache = self.message_cache.clone();

        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(300)).await; // Every 5 minutes

                // Cleanup expired pairings
                pairing_manager.cleanup_expired().await;

                // Cleanup expired messages
                match message_cache.cleanup_expired().await {
                    Ok(count) => {
                        if count > 0 {
                            info!("Cleaned up {} expired messages", count);
                        }
                    }
                    Err(e) => error!("Failed to cleanup expired messages: {}", e),
                }
            }
        });

        *self.cleanup_task.write().await = Some(task);
    }

    /// Get appliance capabilities
    pub async fn get_capabilities(&self) -> ApplianceResult<ApplianceCapabilities> {
        let current_paired = self.device_store.count_active().await?;

        Ok(ApplianceCapabilities {
            max_paired_devices: self.config.max_paired_devices,
            current_paired_devices: current_paired,
            message_caching: self.config.message_caching,
            max_cache_messages_per_device: self.config.cache_config.max_messages_per_device,
            relay_enabled: self.config.relay_enabled,
            bridge_enabled: self.config.bridge_enabled,
            pairing_available: current_paired < self.config.max_paired_devices,
            pairing_methods: self.config.pairing_methods.clone(),
        })
    }

    /// Initiate device pairing
    pub async fn initiate_pairing(&self, request: PairingRequest) -> ApplianceResult<PairingToken> {
        // Check if max devices reached
        let current_paired = self.device_store.count_active().await?;
        if current_paired >= self.config.max_paired_devices {
            return Err(ApplianceError::MaxDevicesReached(
                self.config.max_paired_devices,
            ));
        }

        // Check if device already paired
        if let Some(existing) = self.device_store.get(&request.device_id).await? {
            if existing.active {
                return Err(ApplianceError::DeviceAlreadyPaired(request.device_id));
            }
        }

        // Initiate pairing
        self.pairing_manager.initiate_pairing(request).await
    }

    /// Approve a pending pairing
    pub async fn approve_pairing(&self, token: &str) -> ApplianceResult<()> {
        self.pairing_manager.approve_pairing(token).await
    }

    /// Reject a pending pairing
    pub async fn reject_pairing(&self, token: &str) -> ApplianceResult<()> {
        self.pairing_manager.reject_pairing(token).await
    }

    /// Complete device pairing
    pub async fn complete_pairing(
        &self,
        response: PairingResponse,
        device_id: String,
        node_id: NodeId,
        public_key: Vec<u8>,
    ) -> ApplianceResult<PairingResult> {
        let result = self.pairing_manager.complete_pairing(response).await?;

        if result.success {
            if let Some(session_token) = &result.session_token {
                // Create session token hash
                let token_hash =
                    format!("{:x}", blake2::Blake2b512::digest(session_token.as_bytes()));

                // Store paired device
                let device = PairedDevice::new(device_id, node_id, public_key, token_hash);
                self.device_store.store(&device).await?;

                info!("Device paired successfully: {}", device.device_id);
            }
        }

        Ok(result)
    }

    /// Get paired device information
    pub async fn get_paired_device(
        &self,
        device_id: &str,
    ) -> ApplianceResult<Option<PairedDevice>> {
        self.device_store.get(device_id).await
    }

    /// List all paired devices
    pub async fn list_paired_devices(&self) -> ApplianceResult<Vec<PairedDeviceInfo>> {
        let devices = self.device_store.list_all().await?;

        let mut device_infos = Vec::new();
        for device in devices {
            let mut info = PairedDeviceInfo::from(&device);

            // Get cached message count
            if let Ok(stats) = self.message_cache.get_stats(&device.device_id).await {
                info.cached_messages = stats.total_cached;
            }

            device_infos.push(info);
        }

        Ok(device_infos)
    }

    /// Unpair a device
    pub async fn unpair_device(&self, device_id: &str) -> ApplianceResult<()> {
        // Delete device
        self.device_store.delete(device_id).await?;

        // Delete cached messages
        self.message_cache.delete_device_messages(device_id).await?;

        info!("Device unpaired: {}", device_id);
        Ok(())
    }

    /// Update device preferences
    pub async fn update_device_preferences(
        &self,
        device_id: &str,
        preferences: DevicePreferences,
    ) -> ApplianceResult<()> {
        let mut device = self
            .device_store
            .get(device_id)
            .await?
            .ok_or_else(|| ApplianceError::DeviceNotFound(device_id.to_string()))?;

        device.update_preferences(preferences);
        self.device_store.store(&device).await?;

        Ok(())
    }

    /// Cache a message for a device
    pub async fn cache_message(&self, message: CachedMessage) -> ApplianceResult<()> {
        if !self.config.message_caching {
            return Err(ApplianceError::Configuration(
                "Message caching is disabled".to_string(),
            ));
        }

        // Verify device is paired
        let device = self
            .device_store
            .get(&message.device_id)
            .await?
            .ok_or_else(|| ApplianceError::DeviceNotFound(message.device_id.clone()))?;

        if !device.active {
            return Err(ApplianceError::DeviceNotFound(message.device_id.clone()));
        }

        // Store message
        self.message_cache.store(&message).await?;

        Ok(())
    }

    /// Retrieve cached messages for a device
    pub async fn retrieve_messages(
        &self,
        device_id: &str,
        limit: Option<usize>,
        only_undelivered: bool,
    ) -> ApplianceResult<Vec<CachedMessage>> {
        // Verify device is paired
        self.device_store
            .get(device_id)
            .await?
            .ok_or_else(|| ApplianceError::DeviceNotFound(device_id.to_string()))?;

        // Update last seen
        self.device_store.update_last_seen(device_id).await?;

        // Retrieve messages
        self.message_cache
            .retrieve(device_id, limit, only_undelivered)
            .await
    }

    /// Mark messages as delivered
    pub async fn mark_messages_delivered(&self, message_ids: Vec<String>) -> ApplianceResult<()> {
        self.message_cache.mark_delivered(&message_ids).await
    }

    /// Get cache statistics for a device
    pub async fn get_cache_stats(
        &self,
        device_id: &str,
    ) -> ApplianceResult<crate::cache::CacheStats> {
        self.message_cache.get_stats(device_id).await
    }

    /// Verify session token for a device
    pub async fn verify_session_token(
        &self,
        device_id: &str,
        token: &str,
    ) -> ApplianceResult<bool> {
        let device = self
            .device_store
            .get(device_id)
            .await?
            .ok_or_else(|| ApplianceError::DeviceNotFound(device_id.to_string()))?;

        Ok(device.verify_session_token(token))
    }

    /// Get appliance statistics
    pub async fn get_stats(
        &self,
        uptime_secs: u64,
        adapters_online: usize,
    ) -> ApplianceResult<ApplianceStats> {
        let paired_devices = self.device_store.count_active().await?;

        // Count total cached messages across all devices
        let devices = self.device_store.list_all().await?;
        let mut total_cached = 0;
        for device in devices {
            if let Ok(stats) = self.message_cache.get_stats(&device.device_id).await {
                total_cached += stats.total_cached;
            }
        }

        Ok(ApplianceStats {
            uptime_secs,
            paired_devices,
            total_cached_messages: total_cached,
            adapters_online,
        })
    }

    /// Shutdown the appliance manager
    pub async fn shutdown(&self) {
        info!("Shutting down appliance manager");

        // Cancel cleanup task
        if let Some(task) = self.cleanup_task.write().await.take() {
            task.abort();
        }
    }
}

impl Drop for ApplianceManager {
    fn drop(&mut self) {
        info!("Appliance manager dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::PairingMethod;
    use ed25519_dalek::{Signer, SigningKey};
    use myriadmesh_crypto::identity::NodeId;
    use rand::rngs::OsRng;
    use rand::RngCore;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_appliance_manager_pairing_flow() {
        let temp_dir = TempDir::new().unwrap();
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let config = ApplianceManagerConfig {
            node_id: "test-appliance".to_string(),
            data_directory: temp_dir.path().to_path_buf(),
            require_pairing_approval: false,
            ..Default::default()
        };

        let manager = ApplianceManager::new(config, signing_key).await.unwrap();

        // Mobile device
        let mut device_key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut device_key_bytes);
        let device_signing_key = SigningKey::from_bytes(&device_key_bytes);
        let device_public_key = device_signing_key.verifying_key().to_bytes().to_vec();

        // Initiate pairing
        let request = PairingRequest {
            device_id: "mobile-1".to_string(),
            public_key: device_public_key.clone(),
            method: PairingMethod::QrCode,
            timestamp: chrono::Utc::now().timestamp(),
        };

        let token = manager.initiate_pairing(request).await.unwrap();

        // Complete pairing
        let challenge_sig = device_signing_key.sign(&token.challenge);
        let response = PairingResponse {
            pairing_token: token.token.clone(),
            challenge_signature: challenge_sig.to_bytes().to_vec(),
        };

        let node_id = NodeId::from_bytes([0u8; 64]);
        let result = manager
            .complete_pairing(response, "mobile-1".to_string(), node_id, device_public_key)
            .await
            .unwrap();

        assert!(result.success);

        // Verify device is paired
        let device = manager.get_paired_device("mobile-1").await.unwrap();
        assert!(device.is_some());
    }

    #[tokio::test]
    async fn test_message_caching() {
        let temp_dir = TempDir::new().unwrap();
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);

        let config = ApplianceManagerConfig {
            node_id: "test-appliance".to_string(),
            data_directory: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let manager = ApplianceManager::new(config, signing_key).await.unwrap();

        // Create a paired device manually
        let node_id = NodeId::from_bytes([0u8; 64]);
        let device = PairedDevice::new(
            "mobile-1".to_string(),
            node_id,
            vec![1, 2, 3, 4],
            "test-hash".to_string(),
        );
        manager.device_store.store(&device).await.unwrap();

        // Cache a message
        let message = CachedMessage::new(
            "msg-1".to_string(),
            "mobile-1".to_string(),
            crate::cache::MessageDirection::Inbound,
            crate::cache::MessagePriority::Normal,
            vec![1, 2, 3, 4],
            None,
            None,
        );

        manager.cache_message(message).await.unwrap();

        // Retrieve messages
        let messages = manager
            .retrieve_messages("mobile-1", None, false)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message_id, "msg-1");

        // Mark delivered
        manager
            .mark_messages_delivered(vec!["msg-1".to_string()])
            .await
            .unwrap();

        // Verify stats
        let stats = manager.get_cache_stats("mobile-1").await.unwrap();
        assert_eq!(stats.undelivered, 0);
    }
}
