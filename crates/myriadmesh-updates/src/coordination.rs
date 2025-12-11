//! Update coordination module
//!
//! Ties together scheduling, distribution, and verification into a complete
//! update coordination system.

use crate::distribution::{
    UpdateDistributionManager, UpdatePackage, UpdateSignature, UpdateSource,
};
use crate::schedule::{
    PeerDowntime, UpdateSchedule, UpdateScheduleManager, UpdateScheduleResponse,
    UpdateWindowSelector,
};
use crate::verification::SignatureVerifier;
use crate::{Result, UpdateError};
use myriadmesh_crypto::identity::NodeIdentity;
use myriadmesh_crypto::signing::sign_message;
use myriadmesh_network::version_tracking::SemanticVersion;
use myriadmesh_protocol::{types::AdapterType, NodeId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Central coordinator for the update system
pub struct UpdateCoordinator {
    /// Node's identity
    identity: Arc<NodeIdentity>,

    /// Schedule manager
    schedule_manager: Arc<UpdateScheduleManager>,

    /// Distribution manager
    distribution_manager: Arc<UpdateDistributionManager>,

    /// Window selector
    window_selector: Arc<UpdateWindowSelector>,

    /// Signature verifier
    verifier: Arc<RwLock<Option<Arc<SignatureVerifier>>>>,

    /// Current adapter versions
    adapter_versions: Arc<RwLock<HashMap<AdapterType, SemanticVersion>>>,

    /// Neighbors using each adapter
    adapter_neighbors: Arc<RwLock<HashMap<AdapterType, HashSet<NodeId>>>>,

    /// Available fallback adapters
    available_adapters: Arc<RwLock<HashSet<AdapterType>>>,
}

impl UpdateCoordinator {
    /// Create a new update coordinator
    pub fn new(identity: Arc<NodeIdentity>) -> Self {
        Self {
            identity,
            schedule_manager: Arc::new(UpdateScheduleManager::new()),
            distribution_manager: Arc::new(UpdateDistributionManager::new()),
            window_selector: Arc::new(UpdateWindowSelector::new()),
            verifier: Arc::new(RwLock::new(None)),
            adapter_versions: Arc::new(RwLock::new(HashMap::new())),
            adapter_neighbors: Arc::new(RwLock::new(HashMap::new())),
            available_adapters: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Set the signature verifier
    pub async fn set_verifier(&self, verifier: Arc<SignatureVerifier>) {
        *self.verifier.write().await = Some(verifier);
    }

    /// Register current version for an adapter
    pub async fn register_adapter_version(
        &self,
        adapter_type: AdapterType,
        version: SemanticVersion,
    ) {
        let mut versions = self.adapter_versions.write().await;
        versions.insert(adapter_type, version);
    }

    /// Register neighbors using a specific adapter
    pub async fn register_adapter_neighbors(
        &self,
        adapter_type: AdapterType,
        neighbors: HashSet<NodeId>,
    ) {
        let mut adapter_neighbors = self.adapter_neighbors.write().await;
        adapter_neighbors.insert(adapter_type, neighbors);
    }

    /// Register available adapters
    pub async fn register_available_adapters(&self, adapters: HashSet<AdapterType>) {
        *self.available_adapters.write().await = adapters;
    }

    /// Schedule an adapter update (Phase 4.6)
    ///
    /// This coordinates with neighbors to find an optimal update window
    pub async fn schedule_adapter_update(
        &self,
        adapter_type: AdapterType,
        target_version: SemanticVersion,
        estimated_duration: Duration,
    ) -> Result<UpdateSchedule> {
        // Get current version
        let current_version = {
            let versions = self.adapter_versions.read().await;
            versions
                .get(&adapter_type)
                .cloned()
                .ok_or_else(|| UpdateError::Other("Adapter version not registered".to_string()))?
        };

        // Get neighbors using this adapter
        let neighbors = {
            let adapter_neighbors = self.adapter_neighbors.read().await;
            adapter_neighbors
                .get(&adapter_type)
                .cloned()
                .unwrap_or_default()
        };

        // Identify fallback adapters
        let fallback_adapters = self.identify_fallback_adapters(adapter_type).await?;

        if fallback_adapters.is_empty() {
            return Err(UpdateError::NoFallbackAdapters);
        }

        // Find optimal update window
        let neighbors_vec: Vec<NodeId> = neighbors.iter().cloned().collect();
        let scheduled_start = self
            .window_selector
            .find_optimal_window(&neighbors_vec, estimated_duration)
            .await?;

        // Create update schedule
        let schedule = UpdateSchedule::new(
            NodeId::from_bytes(*self.identity.node_id.as_bytes()),
            adapter_type,
            current_version,
            target_version,
            scheduled_start,
            estimated_duration,
            fallback_adapters,
        );

        // Add to pending schedules
        self.schedule_manager
            .add_pending_schedule(schedule.clone())
            .await;

        Ok(schedule)
    }

    /// Handle incoming update schedule request from a neighbor
    pub async fn handle_schedule_request(
        &self,
        schedule: UpdateSchedule,
    ) -> Result<UpdateScheduleResponse> {
        // Check if we can accommodate this update
        let neighbors = {
            let adapter_neighbors = self.adapter_neighbors.read().await;
            adapter_neighbors
                .get(&schedule.adapter_type)
                .cloned()
                .unwrap_or_default()
        };

        let neighbors_vec: Vec<NodeId> = neighbors.iter().cloned().collect();

        // Check for conflicts
        let has_conflict = self
            .window_selector
            .has_conflict(
                schedule.scheduled_start,
                schedule.estimated_duration,
                &neighbors_vec,
            )
            .await;

        if has_conflict {
            // Try to find alternative window
            let proposed_start = self
                .window_selector
                .find_optimal_window(&neighbors_vec, schedule.estimated_duration)
                .await?;

            return Ok(UpdateScheduleResponse::Reschedule {
                node_id: NodeId::from_bytes(*self.identity.node_id.as_bytes()),
                proposed_start,
                reason: "Conflicting scheduled maintenance".to_string(),
                response_id: format!("resp-{}", uuid::Uuid::new_v4()),
            });
        }

        // Verify we have the fallback adapters
        let available = self.available_adapters.read().await;
        for fallback in &schedule.fallback_adapters {
            if !available.contains(fallback) {
                return Ok(UpdateScheduleResponse::Rejected {
                    node_id: NodeId::from_bytes(*self.identity.node_id.as_bytes()),
                    reason: format!("Fallback adapter {:?} not available", fallback),
                    response_id: format!("resp-{}", uuid::Uuid::new_v4()),
                });
            }
        }

        // Register the peer's downtime
        let downtime = PeerDowntime {
            peer_id: schedule.node_id,
            adapter_type: schedule.adapter_type,
            start_time: schedule.scheduled_start,
            end_time: schedule.scheduled_end(),
            fallback_adapters: schedule.fallback_adapters.clone(),
        };

        self.window_selector.register_peer_downtime(downtime).await;

        // Acknowledge
        Ok(UpdateScheduleResponse::Acknowledged {
            node_id: NodeId::from_bytes(*self.identity.node_id.as_bytes()),
            fallback_adapters: schedule.fallback_adapters.clone(),
            response_id: format!("resp-{}", uuid::Uuid::new_v4()),
        })
    }

    /// Receive an update package from a peer (Phase 4.7)
    pub async fn receive_update_package(&self, package: UpdatePackage) -> Result<()> {
        // Verify payload hash
        if !package.verify_payload_hash() {
            return Err(UpdateError::HashMismatch {
                expected: hex::encode(package.payload_hash),
                actual: "invalid".to_string(),
            });
        }

        // Verify signature chain
        let verifier = self.verifier.read().await;
        let verifier = verifier
            .as_ref()
            .ok_or_else(|| UpdateError::Other("Verifier not set".to_string()))?;

        let verification = verifier.verify_package(&package)?;

        if !verification.passed {
            return Err(UpdateError::InsufficientSignatures(
                verification
                    .failure_reason
                    .unwrap_or_else(|| "Unknown reason".to_string()),
            ));
        }

        // Check if we need this update
        let current_version = {
            let versions = self.adapter_versions.read().await;
            versions.get(&package.adapter_type).cloned()
        };

        if let Some(current) = current_version {
            if current >= package.version {
                // Already at this version or higher
                return Ok(());
            }
        }

        // Add to pending updates
        self.distribution_manager
            .add_pending_update(package.clone())
            .await?;

        // Mark signatures as verified
        self.distribution_manager
            .mark_signatures_verified(package.adapter_type, &package.version)
            .await;

        // If critical security update, schedule immediately
        if package.is_critical_security_update() {
            log::warn!(
                "Critical security update available for {:?} v{}",
                package.adapter_type,
                package.version
            );
            // In a real implementation, we would trigger immediate scheduling here
        }

        Ok(())
    }

    /// Forward an update package to neighbors
    pub async fn forward_update_package(
        &self,
        mut package: UpdatePackage,
        neighbors: Vec<NodeId>,
    ) -> Result<()> {
        // Get signable data
        let signable_data = package.get_signable_data()?;

        // Sign the package
        let signature = sign_message(&self.identity, &signable_data)?;

        // Add our signature
        package.add_signature(UpdateSignature::new(
            NodeId::from_bytes(*self.identity.node_id.as_bytes()),
            signature,
        ));

        // Update source to show we forwarded it
        if let UpdateSource::PeerForwarded {
            original_node,
            mut forwarded_by,
            hop_count,
        } = package.source
        {
            forwarded_by.push(NodeId::from_bytes(*self.identity.node_id.as_bytes()));
            package.source = UpdateSource::PeerForwarded {
                original_node,
                forwarded_by,
                hop_count: hop_count + 1,
            };
        }

        // In a real implementation, we would send the package to each neighbor
        // For now, we just log it
        log::info!(
            "Would forward update package for {:?} v{} to {} neighbors",
            package.adapter_type,
            package.version,
            neighbors.len()
        );

        Ok(())
    }

    /// Get all ready-to-install updates
    pub async fn get_ready_updates(&self) -> Vec<UpdatePackage> {
        self.distribution_manager.get_ready_updates().await
    }

    /// Mark an update as installed
    pub async fn mark_update_installed(
        &self,
        adapter_type: AdapterType,
        version: &SemanticVersion,
    ) {
        self.distribution_manager
            .mark_installed(adapter_type, version)
            .await;

        // Update our version record
        let mut versions = self.adapter_versions.write().await;
        versions.insert(adapter_type, version.clone());
    }

    /// Cleanup expired data
    pub async fn cleanup(&self) {
        // Cleanup old schedules
        self.schedule_manager.cleanup_old_requests().await;

        // Cleanup expired pending updates
        self.distribution_manager.cleanup_expired().await;

        // Cleanup expired downtimes
        self.window_selector.cleanup_expired_downtimes().await;
    }

    /// Identify fallback adapters for a given adapter
    pub async fn identify_fallback_adapters(
        &self,
        adapter_type: AdapterType,
    ) -> Result<Vec<AdapterType>> {
        let available = self.available_adapters.read().await;

        let fallbacks: Vec<AdapterType> = available
            .iter()
            .filter(|&&a| a != adapter_type)
            .copied()
            .collect();

        Ok(fallbacks)
    }

    /// List all pending update schedules
    pub async fn list_pending_schedules(&self) -> Result<Vec<UpdateSchedule>> {
        self.schedule_manager.list_pending_schedules().await
    }

    /// Get schedule manager reference
    pub fn schedule_manager(&self) -> &Arc<UpdateScheduleManager> {
        &self.schedule_manager
    }

    /// Get distribution manager reference
    pub fn distribution_manager(&self) -> &Arc<UpdateDistributionManager> {
        &self.distribution_manager
    }

    /// Get window selector reference
    pub fn window_selector(&self) -> &Arc<UpdateWindowSelector> {
        &self.window_selector
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_coordinator_initialization() {
        myriadmesh_crypto::init().unwrap();
        let identity = Arc::new(NodeIdentity::generate().unwrap());
        let coordinator = UpdateCoordinator::new(identity);

        // Register adapter versions
        coordinator
            .register_adapter_version(AdapterType::Ethernet, SemanticVersion::new(1, 0, 0))
            .await;

        // Register available adapters
        let available = [
            AdapterType::Ethernet,
            AdapterType::Bluetooth,
            AdapterType::BluetoothLE,
        ]
        .into_iter()
        .collect();
        coordinator.register_available_adapters(available).await;

        // Should be able to find fallbacks
        let fallbacks = coordinator
            .identify_fallback_adapters(AdapterType::Ethernet)
            .await
            .unwrap();
        assert_eq!(fallbacks.len(), 2); // Bluetooth and BluetoothLE
    }

    #[tokio::test]
    async fn test_schedule_creation() {
        myriadmesh_crypto::init().unwrap();
        let identity = Arc::new(NodeIdentity::generate().unwrap());
        let coordinator = UpdateCoordinator::new(identity);

        // Setup
        coordinator
            .register_adapter_version(AdapterType::Ethernet, SemanticVersion::new(1, 0, 0))
            .await;

        let available = [AdapterType::Ethernet, AdapterType::Bluetooth]
            .into_iter()
            .collect();
        coordinator.register_available_adapters(available).await;

        coordinator
            .register_adapter_neighbors(AdapterType::Ethernet, HashSet::new())
            .await;

        // Schedule an update
        let schedule = coordinator
            .schedule_adapter_update(
                AdapterType::Ethernet,
                SemanticVersion::new(1, 1, 0),
                Duration::from_secs(60),
            )
            .await
            .unwrap();

        assert_eq!(schedule.adapter_type, AdapterType::Ethernet);
        assert_eq!(schedule.target_version, SemanticVersion::new(1, 1, 0));
        assert!(!schedule.fallback_adapters.is_empty());
    }
}
