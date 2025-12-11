//! Coordinated update scheduling (Phase 4.6)
//!
//! Implements the protocol for nodes to coordinate updates with their neighbors,
//! selecting optimal update windows and establishing fallback paths.

use crate::Result;
use myriadmesh_crypto::signing::Signature;
use myriadmesh_network::version_tracking::SemanticVersion;
use myriadmesh_protocol::{types::AdapterType, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Update schedule request sent to neighbors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSchedule {
    /// Node requesting the update
    pub node_id: NodeId,

    /// Adapter to be updated
    pub adapter_type: AdapterType,

    /// Current version
    pub current_version: SemanticVersion,

    /// Target version
    pub target_version: SemanticVersion,

    /// Requested update window start time (Unix timestamp)
    pub scheduled_start: u64,

    /// Estimated duration of the update
    pub estimated_duration: Duration,

    /// Alternative adapters available during update
    pub fallback_adapters: Vec<AdapterType>,

    /// Ed25519 signature of the schedule
    pub signature: Option<Signature>,

    /// When this schedule was created
    pub created_at: u64,
}

impl UpdateSchedule {
    /// Create a new update schedule
    pub fn new(
        node_id: NodeId,
        adapter_type: AdapterType,
        current_version: SemanticVersion,
        target_version: SemanticVersion,
        scheduled_start: u64,
        estimated_duration: Duration,
        fallback_adapters: Vec<AdapterType>,
    ) -> Self {
        Self {
            node_id,
            adapter_type,
            current_version,
            target_version,
            scheduled_start,
            estimated_duration,
            fallback_adapters,
            signature: None,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Get the scheduled end time
    pub fn scheduled_end(&self) -> u64 {
        self.scheduled_start + self.estimated_duration.as_secs()
    }

    /// Check if this schedule overlaps with another
    pub fn overlaps_with(&self, other: &UpdateSchedule) -> bool {
        let self_end = self.scheduled_end();
        let other_end = other.scheduled_end();

        // Check for overlap
        self.scheduled_start < other_end && other.scheduled_start < self_end
    }
}

/// Update schedule request wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScheduleRequest {
    pub schedule: UpdateSchedule,
    pub request_id: String,
}

/// Response from a neighbor to an update schedule request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateScheduleResponse {
    /// Acknowledged - neighbor will accommodate the update
    Acknowledged {
        node_id: NodeId,
        fallback_adapters: Vec<AdapterType>,
        response_id: String,
    },

    /// Reschedule requested - neighbor has a conflict
    Reschedule {
        node_id: NodeId,
        proposed_start: u64,
        reason: String,
        response_id: String,
    },

    /// Rejected - neighbor cannot accommodate
    Rejected {
        node_id: NodeId,
        reason: String,
        response_id: String,
    },
}

impl UpdateScheduleResponse {
    /// Get the node ID from any response type
    pub fn node_id(&self) -> &NodeId {
        match self {
            UpdateScheduleResponse::Acknowledged { node_id, .. } => node_id,
            UpdateScheduleResponse::Reschedule { node_id, .. } => node_id,
            UpdateScheduleResponse::Rejected { node_id, .. } => node_id,
        }
    }

    /// Check if this is an acknowledgement
    pub fn is_acknowledged(&self) -> bool {
        matches!(self, UpdateScheduleResponse::Acknowledged { .. })
    }
}

/// Scheduled downtime entry for a peer
#[derive(Debug, Clone)]
pub struct PeerDowntime {
    pub peer_id: NodeId,
    pub adapter_type: AdapterType,
    pub start_time: u64,
    pub end_time: u64,
    pub fallback_adapters: Vec<AdapterType>,
}

/// Update window selector for finding optimal update times
pub struct UpdateWindowSelector {
    /// Scheduled downtimes for peers
    peer_downtimes: Arc<RwLock<HashMap<NodeId, Vec<PeerDowntime>>>>,

    /// Network load history (hour -> load factor 0.0-1.0)
    network_load_history: Arc<RwLock<HashMap<u8, f64>>>,
}

impl UpdateWindowSelector {
    /// Create a new update window selector
    pub fn new() -> Self {
        Self {
            peer_downtimes: Arc::new(RwLock::new(HashMap::new())),
            network_load_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a peer's scheduled downtime
    pub async fn register_peer_downtime(&self, downtime: PeerDowntime) {
        let mut downtimes = self.peer_downtimes.write().await;
        downtimes
            .entry(downtime.peer_id)
            .or_insert_with(Vec::new)
            .push(downtime);
    }

    /// Remove expired downtimes
    pub async fn cleanup_expired_downtimes(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut downtimes = self.peer_downtimes.write().await;

        for peer_downtimes in downtimes.values_mut() {
            peer_downtimes.retain(|d| d.end_time > now);
        }

        // Remove peers with no downtimes
        downtimes.retain(|_, v| !v.is_empty());
    }

    /// Find the optimal update window for the given neighbors
    ///
    /// Returns the Unix timestamp for the optimal start time
    pub async fn find_optimal_window(
        &self,
        neighbors: &[NodeId],
        estimated_duration: Duration,
    ) -> Result<u64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Look ahead 24 hours
        let window_end = now + (24 * 60 * 60);

        let mut best_window = now + 300; // Default: 5 minutes from now
        let mut best_score = f64::MIN;

        // Evaluate time slots in 5-minute increments
        for slot in (now..window_end).step_by(300) {
            let score = self
                .evaluate_update_slot(slot, neighbors, estimated_duration)
                .await;

            if score > best_score {
                best_score = score;
                best_window = slot;
            }
        }

        Ok(best_window)
    }

    /// Score a time slot based on network conditions
    ///
    /// Higher scores are better
    async fn evaluate_update_slot(
        &self,
        slot: u64,
        neighbors: &[NodeId],
        duration: Duration,
    ) -> f64 {
        let mut score = 100.0;

        // 1. Prefer off-peak hours (2am-5am UTC)
        let hour = ((slot / 3600) % 24) as u8;
        if (2..=5).contains(&hour) {
            score += 50.0; // Significant bonus for off-peak
        } else if (22..=23).contains(&hour) || hour == 0 || hour == 1 {
            score += 30.0; // Moderate bonus for late night
        } else if (9..=17).contains(&hour) {
            score -= 20.0; // Penalty for business hours
        }

        // 2. Penalize if neighbors have scheduled maintenance
        let downtimes = self.peer_downtimes.read().await;
        let slot_end = slot + duration.as_secs();

        for neighbor in neighbors {
            if let Some(peer_downtimes) = downtimes.get(neighbor) {
                for downtime in peer_downtimes {
                    // Check for overlap
                    if slot < downtime.end_time && downtime.start_time < slot_end {
                        score -= 40.0; // Heavy penalty for conflicts
                    }
                }
            }
        }

        // 3. Prefer low network activity times (based on history)
        let load_history = self.network_load_history.read().await;
        if let Some(&load) = load_history.get(&hour) {
            score -= load * 30.0; // Penalty proportional to load (0-30 points)
        }

        // 4. Slight preference for sooner updates (security)
        let hours_from_now = (slot.saturating_sub(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )) / 3600;
        score -= (hours_from_now as f64) * 0.5; // Small penalty for delaying

        score
    }

    /// Update network load history for a given hour
    pub async fn update_network_load(&self, hour: u8, load: f64) {
        let mut history = self.network_load_history.write().await;
        history.insert(hour % 24, load.clamp(0.0, 1.0));
    }

    /// Get scheduled downtimes for a specific neighbor
    pub async fn get_peer_downtimes(&self, neighbor: &NodeId) -> Vec<PeerDowntime> {
        let downtimes = self.peer_downtimes.read().await;
        downtimes.get(neighbor).cloned().unwrap_or_default()
    }

    /// Check if a given time slot conflicts with any peer downtimes
    pub async fn has_conflict(&self, slot: u64, duration: Duration, neighbors: &[NodeId]) -> bool {
        let downtimes = self.peer_downtimes.read().await;
        let slot_end = slot + duration.as_secs();

        for neighbor in neighbors {
            if let Some(peer_downtimes) = downtimes.get(neighbor) {
                for downtime in peer_downtimes {
                    if slot < downtime.end_time && downtime.start_time < slot_end {
                        return true;
                    }
                }
            }
        }

        false
    }
}

impl Default for UpdateWindowSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for pending update schedules
pub struct UpdateScheduleManager {
    /// Pending update schedules keyed by adapter type
    pending_schedules: Arc<RwLock<HashMap<AdapterType, UpdateSchedule>>>,

    /// Requests we've sent to neighbors
    #[allow(clippy::type_complexity)]
    sent_requests: Arc<RwLock<HashMap<String, (UpdateScheduleRequest, HashSet<NodeId>)>>>,

    /// Responses we've received
    received_responses: Arc<RwLock<HashMap<String, HashMap<NodeId, UpdateScheduleResponse>>>>,
}

impl UpdateScheduleManager {
    /// Create a new schedule manager
    pub fn new() -> Self {
        Self {
            pending_schedules: Arc::new(RwLock::new(HashMap::new())),
            sent_requests: Arc::new(RwLock::new(HashMap::new())),
            received_responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a pending schedule
    pub async fn add_pending_schedule(&self, schedule: UpdateSchedule) {
        let mut schedules = self.pending_schedules.write().await;
        schedules.insert(schedule.adapter_type, schedule);
    }

    /// Get a pending schedule for an adapter
    pub async fn get_pending_schedule(&self, adapter_type: AdapterType) -> Option<UpdateSchedule> {
        let schedules = self.pending_schedules.read().await;
        schedules.get(&adapter_type).cloned()
    }

    /// Remove a pending schedule
    pub async fn remove_pending_schedule(
        &self,
        adapter_type: AdapterType,
    ) -> Option<UpdateSchedule> {
        let mut schedules = self.pending_schedules.write().await;
        schedules.remove(&adapter_type)
    }

    /// List all pending schedules
    pub async fn list_pending_schedules(&self) -> crate::Result<Vec<UpdateSchedule>> {
        let schedules = self.pending_schedules.read().await;
        Ok(schedules.values().cloned().collect())
    }

    /// Record a sent request
    pub async fn record_sent_request(
        &self,
        request: UpdateScheduleRequest,
        neighbors: HashSet<NodeId>,
    ) {
        let mut sent = self.sent_requests.write().await;
        sent.insert(request.request_id.clone(), (request, neighbors));
    }

    /// Record a received response
    pub async fn record_response(&self, request_id: String, response: UpdateScheduleResponse) {
        let mut responses = self.received_responses.write().await;
        responses
            .entry(request_id)
            .or_insert_with(HashMap::new)
            .insert(*response.node_id(), response);
    }

    /// Check if all neighbors have responded to a request
    pub async fn all_responded(&self, request_id: &str) -> bool {
        let sent = self.sent_requests.read().await;
        let responses = self.received_responses.read().await;

        if let Some((_, neighbors)) = sent.get(request_id) {
            if let Some(resp_map) = responses.get(request_id) {
                return neighbors.iter().all(|n| resp_map.contains_key(n));
            }
        }

        false
    }

    /// Get all responses for a request
    pub async fn get_responses(
        &self,
        request_id: &str,
    ) -> Option<HashMap<NodeId, UpdateScheduleResponse>> {
        let responses = self.received_responses.read().await;
        responses.get(request_id).cloned()
    }

    /// Clean up old requests (older than 24 hours)
    pub async fn cleanup_old_requests(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut sent = self.sent_requests.write().await;
        let mut responses = self.received_responses.write().await;

        // Remove requests older than 24 hours
        sent.retain(|_, (req, _)| now - req.schedule.created_at < 24 * 60 * 60);

        // Also remove corresponding responses
        responses.retain(|req_id, _| sent.contains_key(req_id));
    }
}

impl Default for UpdateScheduleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schedule_overlap() {
        let schedule1 = UpdateSchedule::new(
            NodeId::from_bytes([1u8; 64]),
            AdapterType::Ethernet,
            SemanticVersion::new(1, 0, 0),
            SemanticVersion::new(1, 1, 0),
            1000,
            Duration::from_secs(100),
            vec![AdapterType::Bluetooth],
        );

        let schedule2 = UpdateSchedule::new(
            NodeId::from_bytes([2u8; 64]),
            AdapterType::Bluetooth,
            SemanticVersion::new(1, 0, 0),
            SemanticVersion::new(1, 1, 0),
            1050, // Overlaps with schedule1
            Duration::from_secs(100),
            vec![AdapterType::Bluetooth],
        );

        assert!(schedule1.overlaps_with(&schedule2));
        assert!(schedule2.overlaps_with(&schedule1));

        let schedule3 = UpdateSchedule::new(
            NodeId::from_bytes([3u8; 64]),
            AdapterType::Cellular,
            SemanticVersion::new(1, 0, 0),
            SemanticVersion::new(1, 1, 0),
            1200, // After schedule1
            Duration::from_secs(100),
            vec![AdapterType::Bluetooth],
        );

        assert!(!schedule1.overlaps_with(&schedule3));
        assert!(!schedule3.overlaps_with(&schedule1));
    }

    #[tokio::test]
    async fn test_window_selector() {
        let selector = UpdateWindowSelector::new();

        // Register a peer downtime
        let downtime = PeerDowntime {
            peer_id: NodeId::from_bytes([1u8; 64]),
            adapter_type: AdapterType::Ethernet,
            start_time: 1000,
            end_time: 1100,
            fallback_adapters: vec![AdapterType::Bluetooth],
        };

        selector.register_peer_downtime(downtime).await;

        // Check for conflict
        let neighbors = vec![NodeId::from_bytes([1u8; 64])];
        let has_conflict = selector
            .has_conflict(1050, Duration::from_secs(50), &neighbors)
            .await;
        assert!(has_conflict);

        // No conflict outside the window
        let has_conflict = selector
            .has_conflict(1200, Duration::from_secs(50), &neighbors)
            .await;
        assert!(!has_conflict);
    }

    #[tokio::test]
    async fn test_schedule_manager() {
        let manager = UpdateScheduleManager::new();

        let schedule = UpdateSchedule::new(
            NodeId::from_bytes([1u8; 64]),
            AdapterType::Ethernet,
            SemanticVersion::new(1, 0, 0),
            SemanticVersion::new(1, 1, 0),
            1000,
            Duration::from_secs(100),
            vec![AdapterType::Bluetooth],
        );

        // Add schedule
        manager.add_pending_schedule(schedule.clone()).await;

        // Retrieve it
        let retrieved = manager.get_pending_schedule(AdapterType::Ethernet).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().adapter_type, AdapterType::Ethernet);

        // Remove it
        let removed = manager.remove_pending_schedule(AdapterType::Ethernet).await;
        assert!(removed.is_some());

        // Should be gone now
        let retrieved = manager.get_pending_schedule(AdapterType::Ethernet).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_response_tracking() {
        let manager = UpdateScheduleManager::new();

        let request = UpdateScheduleRequest {
            schedule: UpdateSchedule::new(
                NodeId::from_bytes([1u8; 64]),
                AdapterType::Ethernet,
                SemanticVersion::new(1, 0, 0),
                SemanticVersion::new(1, 1, 0),
                1000,
                Duration::from_secs(100),
                vec![AdapterType::Bluetooth],
            ),
            request_id: "req-123".to_string(),
        };

        let neighbor1 = NodeId::from_bytes([2u8; 64]);
        let neighbor2 = NodeId::from_bytes([3u8; 64]);
        let neighbors = [neighbor1, neighbor2].into_iter().collect();

        manager.record_sent_request(request, neighbors).await;

        // Initially, not all responded
        assert!(!manager.all_responded("req-123").await);

        // Add first response
        let response1 = UpdateScheduleResponse::Acknowledged {
            node_id: neighbor1,
            fallback_adapters: vec![AdapterType::Bluetooth],
            response_id: "resp-1".to_string(),
        };
        manager
            .record_response("req-123".to_string(), response1)
            .await;

        // Still waiting for second
        assert!(!manager.all_responded("req-123").await);

        // Add second response
        let response2 = UpdateScheduleResponse::Acknowledged {
            node_id: neighbor2,
            fallback_adapters: vec![AdapterType::Bluetooth],
            response_id: "resp-2".to_string(),
        };
        manager
            .record_response("req-123".to_string(), response2)
            .await;

        // Now all responded
        assert!(manager.all_responded("req-123").await);

        // Can retrieve responses
        let responses = manager.get_responses("req-123").await;
        assert!(responses.is_some());
        assert_eq!(responses.unwrap().len(), 2);
    }
}
