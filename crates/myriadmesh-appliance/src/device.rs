//! Paired device management

use crate::types::{ApplianceResult, DevicePreferences};
use blake2::Digest;
use chrono::{DateTime, Utc};
use myriadmesh_crypto::identity::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::RwLock;

/// Information about a paired device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    /// Unique device identifier
    pub device_id: String,
    /// Node ID of the device
    #[serde(with = "node_id_serde")]
    pub node_id: NodeId,
    /// Device public key (Ed25519)
    #[serde(with = "hex_bytes")]
    pub public_key: Vec<u8>,
    /// When the device was paired
    pub paired_at: DateTime<Utc>,
    /// Last time the device was seen
    pub last_seen: DateTime<Utc>,
    /// Session token hash (for authentication)
    pub session_token_hash: String,
    /// Device preferences
    pub preferences: DevicePreferences,
    /// Pairing active
    pub active: bool,
}

mod node_id_serde {
    use myriadmesh_crypto::identity::NodeId;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(node_id: &NodeId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&node_id.to_hex())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NodeId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NodeId::from_hex(&s).map_err(serde::de::Error::custom)
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

impl PairedDevice {
    /// Create a new paired device
    pub fn new(
        device_id: String,
        node_id: NodeId,
        public_key: Vec<u8>,
        session_token_hash: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            device_id,
            node_id,
            public_key,
            paired_at: now,
            last_seen: now,
            session_token_hash,
            preferences: DevicePreferences::default(),
            active: true,
        }
    }

    /// Update last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = Utc::now();
    }

    /// Update preferences
    pub fn update_preferences(&mut self, preferences: DevicePreferences) {
        self.preferences = preferences;
    }

    /// Verify session token
    pub fn verify_session_token(&self, token: &str) -> bool {
        let token_hash = format!("{:x}", blake2::Blake2b512::digest(token.as_bytes()));
        self.session_token_hash == token_hash
    }

    /// Revoke pairing
    pub fn revoke(&mut self) {
        self.active = false;
    }
}

/// Lightweight device information for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDeviceInfo {
    pub device_id: String,
    pub node_id: String,
    pub paired_at: i64,
    pub last_seen: i64,
    pub active: bool,
    pub cached_messages: usize,
}

impl From<&PairedDevice> for PairedDeviceInfo {
    fn from(device: &PairedDevice) -> Self {
        Self {
            device_id: device.device_id.clone(),
            node_id: device.node_id.to_hex(),
            paired_at: device.paired_at.timestamp(),
            last_seen: device.last_seen.timestamp(),
            active: device.active,
            cached_messages: 0, // Will be filled by cache query
        }
    }
}

/// Device store using JSON file storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceStoreData {
    devices: HashMap<String, PairedDevice>,
}

pub struct DeviceStore {
    data: RwLock<DeviceStoreData>,
    file_path: PathBuf,
}

impl DeviceStore {
    /// Create a new device store
    pub async fn new<P: AsRef<Path>>(file_path: P) -> ApplianceResult<Self> {
        let file_path = file_path.as_ref().to_path_buf();

        // Load existing data or create new
        let data = if file_path.exists() {
            let contents = fs::read_to_string(&file_path).await?;
            if contents.is_empty() {
                DeviceStoreData {
                    devices: HashMap::new(),
                }
            } else {
                serde_json::from_str(&contents)?
            }
        } else {
            DeviceStoreData {
                devices: HashMap::new(),
            }
        };

        Ok(Self {
            data: RwLock::new(data),
            file_path,
        })
    }

    /// Save data to file
    async fn save(&self) -> ApplianceResult<()> {
        let data = self.data.read().await;
        let json = serde_json::to_string_pretty(&*data)?;

        // Ensure parent directory exists
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&self.file_path, json).await?;
        Ok(())
    }

    /// Store a paired device
    pub async fn store(&self, device: &PairedDevice) -> ApplianceResult<()> {
        let mut data = self.data.write().await;
        data.devices
            .insert(device.device_id.clone(), device.clone());
        drop(data);
        self.save().await
    }

    /// Get a paired device by ID
    pub async fn get(&self, device_id: &str) -> ApplianceResult<Option<PairedDevice>> {
        let data = self.data.read().await;
        Ok(data.devices.get(device_id).cloned())
    }

    /// Get all paired devices
    pub async fn list_all(&self) -> ApplianceResult<Vec<PairedDevice>> {
        let data = self.data.read().await;
        let mut devices: Vec<_> = data.devices.values().cloned().collect();
        devices.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        Ok(devices)
    }

    /// Get count of active paired devices
    pub async fn count_active(&self) -> ApplianceResult<usize> {
        let data = self.data.read().await;
        Ok(data.devices.values().filter(|d| d.active).count())
    }

    /// Delete a paired device
    pub async fn delete(&self, device_id: &str) -> ApplianceResult<()> {
        let mut data = self.data.write().await;
        data.devices.remove(device_id);
        drop(data);
        self.save().await
    }

    /// Update last seen timestamp
    pub async fn update_last_seen(&self, device_id: &str) -> ApplianceResult<()> {
        let mut data = self.data.write().await;
        if let Some(device) = data.devices.get_mut(device_id) {
            device.update_last_seen();
        }
        drop(data);
        self.save().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_device_store_crud() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = DeviceStore::new(temp_file.path()).await.unwrap();

        // Create a test device
        let node_id = NodeId::from_bytes([0u8; 64]);
        let device = PairedDevice::new(
            "test-device-1".to_string(),
            node_id,
            vec![1, 2, 3, 4],
            "hash123".to_string(),
        );

        // Store
        store.store(&device).await.unwrap();

        // Retrieve
        let retrieved = store.get("test-device-1").await.unwrap().unwrap();
        assert_eq!(retrieved.device_id, "test-device-1");
        assert_eq!(retrieved.node_id, node_id);

        // Count
        assert_eq!(store.count_active().await.unwrap(), 1);

        // Delete
        store.delete("test-device-1").await.unwrap();
        assert!(store.get("test-device-1").await.unwrap().is_none());
    }
}
