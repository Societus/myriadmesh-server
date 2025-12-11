//! Message caching system for appliance nodes

use crate::types::{ApplianceError, ApplianceResult};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::RwLock;

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

impl MessagePriority {
    /// Get TTL in days for this priority
    pub fn default_ttl_days(&self) -> u32 {
        match self {
            MessagePriority::Low => 3,
            MessagePriority::Normal => 7,
            MessagePriority::High => 14,
            MessagePriority::Urgent => 7,
        }
    }

    /// From u8 value
    pub fn from_u8(value: u8) -> ApplianceResult<Self> {
        match value {
            0 => Ok(MessagePriority::Low),
            1 => Ok(MessagePriority::Normal),
            2 => Ok(MessagePriority::High),
            3 => Ok(MessagePriority::Urgent),
            _ => Err(ApplianceError::InvalidPriority(value)),
        }
    }
}

/// Direction of cached message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

/// A cached message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMessage {
    pub message_id: String,
    pub device_id: String,
    pub direction: MessageDirection,
    pub priority: MessagePriority,
    #[serde(with = "hex_bytes")]
    pub payload: Vec<u8>,
    pub metadata: Option<serde_json::Value>,
    pub received_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub delivered: bool,
    pub delivery_attempts: u32,
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

impl CachedMessage {
    /// Create a new cached message
    pub fn new(
        message_id: String,
        device_id: String,
        direction: MessageDirection,
        priority: MessagePriority,
        payload: Vec<u8>,
        metadata: Option<serde_json::Value>,
        ttl_days: Option<u32>,
    ) -> Self {
        let now = Utc::now();
        let ttl = ttl_days.unwrap_or_else(|| priority.default_ttl_days());
        let expires_at = now + Duration::days(ttl as i64);

        Self {
            message_id,
            device_id,
            direction,
            priority,
            payload,
            metadata,
            received_at: now,
            expires_at,
            delivered: false,
            delivery_attempts: 0,
        }
    }

    /// Check if message is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Message cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub device_id: String,
    pub total_cached: usize,
    pub by_priority: PriorityStats,
    pub undelivered: usize,
    pub oldest_message_age_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityStats {
    pub urgent: usize,
    pub high: usize,
    pub normal: usize,
    pub low: usize,
}

/// Message cache configuration
#[derive(Debug, Clone)]
pub struct MessageCacheConfig {
    pub max_messages_per_device: usize,
    pub max_total_messages: usize,
}

impl Default for MessageCacheConfig {
    fn default() -> Self {
        Self {
            max_messages_per_device: 1000,
            max_total_messages: 10000,
        }
    }
}

/// Cache store data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheStoreData {
    messages: HashMap<String, CachedMessage>,
}

/// Message cache implementation using JSON file storage
pub struct MessageCache {
    data: RwLock<CacheStoreData>,
    config: MessageCacheConfig,
    file_path: PathBuf,
}

impl MessageCache {
    /// Create a new message cache
    pub async fn new<P: AsRef<Path>>(
        file_path: P,
        config: MessageCacheConfig,
    ) -> ApplianceResult<Self> {
        let file_path = file_path.as_ref().to_path_buf();

        // Load existing data or create new
        let data = if file_path.exists() {
            let contents = fs::read_to_string(&file_path).await?;
            if contents.is_empty() {
                CacheStoreData {
                    messages: HashMap::new(),
                }
            } else {
                serde_json::from_str(&contents)?
            }
        } else {
            CacheStoreData {
                messages: HashMap::new(),
            }
        };

        Ok(Self {
            data: RwLock::new(data),
            config,
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

    /// Store a message in the cache
    ///
    /// # TOCTOU Race Prevention
    ///
    /// This method uses a single atomic operation to check limits and insert the message.
    /// Previously, there was a race condition where:
    /// 1. check_limits() acquired read lock, checked, dropped lock
    /// 2. store() acquired write lock, inserted message
    ///
    /// Between steps 1 and 2, another thread could insert messages, causing limits to be exceeded.
    ///
    /// Fixed by holding the write lock for both check and insert operations atomically.
    pub async fn store(&self, message: &CachedMessage) -> ApplianceResult<()> {
        // CONCURRENCY FIX: Single atomic operation (write lock held throughout)
        let mut data = self.data.write().await;

        // Check device limit atomically
        let device_count = data
            .messages
            .values()
            .filter(|m| m.device_id == message.device_id)
            .count();

        if device_count >= self.config.max_messages_per_device {
            // Try to evict delivered or low-priority messages for this device
            Self::evict_messages_locked(&mut data, &message.device_id, &self.config);

            // Re-count after eviction
            let device_count = data
                .messages
                .values()
                .filter(|m| m.device_id == message.device_id)
                .count();

            if device_count >= self.config.max_messages_per_device {
                drop(data);
                return Err(ApplianceError::CacheFull);
            }
        }

        // Check total limit atomically
        let total_count = data.messages.len();
        if total_count >= self.config.max_total_messages {
            // Try to evict globally
            Self::evict_global_locked(&mut data, &self.config);

            // Re-check after eviction
            if data.messages.len() >= self.config.max_total_messages {
                drop(data);
                return Err(ApplianceError::CacheFull);
            }
        }

        // Insert message atomically (still holding write lock)
        data.messages
            .insert(message.message_id.clone(), message.clone());
        drop(data);

        self.save().await
    }

    /// Check if storage limits would be exceeded
    #[allow(dead_code)]
    async fn check_limits(&self, device_id: &str) -> ApplianceResult<()> {
        let data = self.data.read().await;

        // Check device limit
        let device_count = data
            .messages
            .values()
            .filter(|m| m.device_id == device_id)
            .count();

        if device_count >= self.config.max_messages_per_device {
            // Try to evict delivered or low-priority messages
            drop(data);
            self.evict_messages(device_id).await?;

            let data = self.data.read().await;
            let device_count = data
                .messages
                .values()
                .filter(|m| m.device_id == device_id)
                .count();

            if device_count >= self.config.max_messages_per_device {
                return Err(ApplianceError::CacheFull);
            }

            // Re-check total limit after eviction
            let total_count = data.messages.len();
            drop(data);
            if total_count >= self.config.max_total_messages {
                self.evict_global().await?;
            }
        } else {
            // Check total limit
            let total_count = data.messages.len();
            drop(data);
            if total_count >= self.config.max_total_messages {
                self.evict_global().await?;
            }
        }

        Ok(())
    }

    /// Evict messages for a specific device
    #[allow(dead_code)]
    async fn evict_messages(&self, device_id: &str) -> ApplianceResult<()> {
        let mut data = self.data.write().await;

        // Remove delivered messages first
        data.messages
            .retain(|_, m| !(m.device_id == device_id && m.delivered));

        // If still over limit, remove oldest low-priority messages
        let mut device_messages: Vec<_> = data
            .messages
            .iter()
            .filter(|(_, m)| m.device_id == device_id)
            .map(|(id, m)| (id.clone(), m.priority, m.received_at))
            .collect();

        if device_messages.len() > self.config.max_messages_per_device {
            // Sort by priority (ascending) and received_at (ascending)
            device_messages.sort_by(|(_, a_priority, a_time), (_, b_priority, b_time)| {
                (a_priority, a_time).cmp(&(b_priority, b_time))
            });

            // Collect IDs to remove
            let to_remove = device_messages.len() - self.config.max_messages_per_device;
            let ids_to_remove: Vec<_> = device_messages
                .iter()
                .take(to_remove)
                .map(|(id, _, _)| id.clone())
                .collect();

            // Remove them
            for id in ids_to_remove {
                data.messages.remove(&id);
            }
        }

        drop(data);
        self.save().await
    }

    /// Global eviction across all devices
    #[allow(dead_code)]
    async fn evict_global(&self) -> ApplianceResult<()> {
        let mut data = self.data.write().await;

        // Remove all delivered messages
        data.messages.retain(|_, m| !m.delivered);

        // Remove expired messages
        let now = Utc::now();
        data.messages.retain(|_, m| m.expires_at > now);

        drop(data);
        self.save().await
    }

    /// Evict messages for a device (lock-free version for atomic operations)
    ///
    /// # TOCTOU Race Prevention
    ///
    /// This helper operates on an already-held write lock, allowing check-and-evict
    /// to be a single atomic operation in store().
    fn evict_messages_locked(
        data: &mut CacheStoreData,
        device_id: &str,
        config: &MessageCacheConfig,
    ) {
        // Remove delivered messages first
        data.messages
            .retain(|_, m| !(m.device_id == device_id && m.delivered));

        // If still over limit, remove oldest low-priority messages
        let mut device_messages: Vec<_> = data
            .messages
            .iter()
            .filter(|(_, m)| m.device_id == device_id)
            .map(|(id, m)| (id.clone(), m.priority, m.received_at))
            .collect();

        if device_messages.len() > config.max_messages_per_device {
            // Sort by priority (ascending) and received_at (ascending)
            device_messages.sort_by(|(_, a_priority, a_time), (_, b_priority, b_time)| {
                (a_priority, a_time).cmp(&(b_priority, b_time))
            });

            // Collect IDs to remove
            let to_remove = device_messages.len() - config.max_messages_per_device;
            let ids_to_remove: Vec<_> = device_messages
                .iter()
                .take(to_remove)
                .map(|(id, _, _)| id.clone())
                .collect();

            // Remove them
            for id in ids_to_remove {
                data.messages.remove(&id);
            }
        }
    }

    /// Global eviction across all devices (lock-free version for atomic operations)
    ///
    /// # TOCTOU Race Prevention
    ///
    /// This helper operates on an already-held write lock, allowing check-and-evict
    /// to be a single atomic operation in store().
    fn evict_global_locked(data: &mut CacheStoreData, _config: &MessageCacheConfig) {
        // Remove all delivered messages
        data.messages.retain(|_, m| !m.delivered);

        // Remove expired messages
        let now = Utc::now();
        data.messages.retain(|_, m| m.expires_at > now);
    }

    /// Retrieve messages for a device
    pub async fn retrieve(
        &self,
        device_id: &str,
        limit: Option<usize>,
        only_undelivered: bool,
    ) -> ApplianceResult<Vec<CachedMessage>> {
        let data = self.data.read().await;
        let limit = limit.unwrap_or(100);

        let mut messages: Vec<_> = data
            .messages
            .values()
            .filter(|m| m.device_id == device_id && (!only_undelivered || !m.delivered))
            .cloned()
            .collect();

        // Sort by priority DESC, then received_at ASC
        messages.sort_by(|a, b| (b.priority, a.received_at).cmp(&(a.priority, b.received_at)));

        messages.truncate(limit);
        Ok(messages)
    }

    /// Mark messages as delivered
    pub async fn mark_delivered(&self, message_ids: &[String]) -> ApplianceResult<()> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let mut data = self.data.write().await;
        for id in message_ids {
            if let Some(msg) = data.messages.get_mut(id) {
                msg.delivered = true;
            }
        }
        drop(data);
        self.save().await
    }

    /// Increment delivery attempt count
    pub async fn increment_delivery_attempts(&self, message_id: &str) -> ApplianceResult<()> {
        let mut data = self.data.write().await;
        if let Some(msg) = data.messages.get_mut(message_id) {
            msg.delivery_attempts += 1;
        }
        drop(data);
        self.save().await
    }

    /// Get cache statistics for a device
    pub async fn get_stats(&self, device_id: &str) -> ApplianceResult<CacheStats> {
        let data = self.data.read().await;

        let device_messages: Vec<_> = data
            .messages
            .values()
            .filter(|m| m.device_id == device_id)
            .collect();

        let total_cached = device_messages.len();
        let undelivered = device_messages.iter().filter(|m| !m.delivered).count();

        let mut urgent = 0;
        let mut high = 0;
        let mut normal = 0;
        let mut low = 0;

        for msg in &device_messages {
            match msg.priority {
                MessagePriority::Urgent => urgent += 1,
                MessagePriority::High => high += 1,
                MessagePriority::Normal => normal += 1,
                MessagePriority::Low => low += 1,
            }
        }

        let oldest_message_age_secs = device_messages
            .iter()
            .map(|m| {
                Utc::now()
                    .signed_duration_since(m.received_at)
                    .num_seconds()
            })
            .max()
            .unwrap_or(0);

        Ok(CacheStats {
            device_id: device_id.to_string(),
            total_cached,
            by_priority: PriorityStats {
                urgent,
                high,
                normal,
                low,
            },
            undelivered,
            oldest_message_age_secs,
        })
    }

    /// Clean up expired messages
    pub async fn cleanup_expired(&self) -> ApplianceResult<usize> {
        let mut data = self.data.write().await;
        let before = data.messages.len();

        let now = Utc::now();
        data.messages.retain(|_, m| m.expires_at > now);

        let deleted = before - data.messages.len();
        drop(data);

        if deleted > 0 {
            self.save().await?;
        }

        Ok(deleted)
    }

    /// Delete all messages for a device
    pub async fn delete_device_messages(&self, device_id: &str) -> ApplianceResult<()> {
        let mut data = self.data.write().await;
        data.messages.retain(|_, m| m.device_id != device_id);
        drop(data);
        self.save().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_message_cache_store_retrieve() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = MessageCache::new(temp_file.path(), MessageCacheConfig::default())
            .await
            .unwrap();

        let msg = CachedMessage::new(
            "msg-1".to_string(),
            "device-1".to_string(),
            MessageDirection::Inbound,
            MessagePriority::High,
            vec![1, 2, 3, 4],
            None,
            None,
        );

        cache.store(&msg).await.unwrap();

        let retrieved = cache.retrieve("device-1", None, false).await.unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].message_id, "msg-1");
        assert_eq!(retrieved[0].priority, MessagePriority::High);
    }

    #[tokio::test]
    async fn test_message_cache_priority_ordering() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = MessageCache::new(temp_file.path(), MessageCacheConfig::default())
            .await
            .unwrap();

        // Store messages with different priorities
        for (id, priority) in [
            ("msg-low", MessagePriority::Low),
            ("msg-urgent", MessagePriority::Urgent),
            ("msg-normal", MessagePriority::Normal),
            ("msg-high", MessagePriority::High),
        ] {
            let msg = CachedMessage::new(
                id.to_string(),
                "device-1".to_string(),
                MessageDirection::Inbound,
                priority,
                vec![1, 2, 3],
                None,
                None,
            );
            cache.store(&msg).await.unwrap();
        }

        let retrieved = cache.retrieve("device-1", None, false).await.unwrap();
        assert_eq!(retrieved.len(), 4);
        // Should be ordered by priority DESC
        assert_eq!(retrieved[0].message_id, "msg-urgent");
        assert_eq!(retrieved[1].message_id, "msg-high");
        assert_eq!(retrieved[2].message_id, "msg-normal");
        assert_eq!(retrieved[3].message_id, "msg-low");
    }

    #[tokio::test]
    async fn test_mark_delivered() {
        let temp_file = NamedTempFile::new().unwrap();
        let cache = MessageCache::new(temp_file.path(), MessageCacheConfig::default())
            .await
            .unwrap();

        let msg = CachedMessage::new(
            "msg-1".to_string(),
            "device-1".to_string(),
            MessageDirection::Inbound,
            MessagePriority::Normal,
            vec![1, 2, 3],
            None,
            None,
        );

        cache.store(&msg).await.unwrap();
        cache.mark_delivered(&["msg-1".to_string()]).await.unwrap();

        let undelivered = cache.retrieve("device-1", None, true).await.unwrap();
        assert_eq!(undelivered.len(), 0);

        let all = cache.retrieve("device-1", None, false).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].delivered);
    }
}
