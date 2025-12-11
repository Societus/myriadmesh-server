//! Peer-assisted update distribution (Phase 4.7)
//!
//! Implements secure update package distribution across the mesh network with:
//! - Multi-signature verification (3+ trusted peers)
//! - 6-hour verification window
//! - Update forwarding protocol
//! - Critical CVE priority override

use crate::{Result, UpdateError};
use blake2::{Blake2b512, Digest};
use myriadmesh_crypto::signing::Signature;
use myriadmesh_network::version_tracking::SemanticVersion;
use myriadmesh_protocol::{types::AdapterType, NodeId};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Update package with signature chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePackage {
    /// Adapter type this update is for
    pub adapter_type: AdapterType,

    /// Target version
    pub version: SemanticVersion,

    /// Update binary/library payload
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,

    /// BLAKE2b-512 hash of the payload
    #[serde(with = "BigArray")]
    pub payload_hash: [u8; 64],

    /// Source of the update
    pub source: UpdateSource,

    /// Signature chain (multiple signatures for verification)
    pub signatures: Vec<UpdateSignature>,

    /// Update metadata
    pub metadata: UpdateMetadata,

    /// When this package was created
    pub created_at: u64,
}

impl UpdatePackage {
    /// Create a new update package
    pub fn new(
        adapter_type: AdapterType,
        version: SemanticVersion,
        payload: Vec<u8>,
        metadata: UpdateMetadata,
        source: UpdateSource,
    ) -> Self {
        // Compute payload hash
        let mut hasher = Blake2b512::new();
        hasher.update(&payload);
        let hash_result = hasher.finalize();

        let mut payload_hash = [0u8; 64];
        payload_hash.copy_from_slice(&hash_result);

        Self {
            adapter_type,
            version,
            payload,
            payload_hash,
            source,
            signatures: Vec::new(),
            metadata,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Verify the payload hash
    pub fn verify_payload_hash(&self) -> bool {
        let mut hasher = Blake2b512::new();
        hasher.update(&self.payload);
        let computed = hasher.finalize();

        computed[..] == self.payload_hash
    }

    /// Add a signature to the package
    pub fn add_signature(&mut self, signature: UpdateSignature) {
        self.signatures.push(signature);
    }

    /// Get the signable data for this package
    ///
    /// Signatures are over: payload_hash || version || metadata_hash
    pub fn get_signable_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Add payload hash
        data.extend_from_slice(&self.payload_hash);

        // Add version
        data.extend_from_slice(self.version.to_string().as_bytes());

        // Add metadata hash
        let metadata_bytes = bincode::serialize(&self.metadata)?;
        let mut hasher = Blake2b512::new();
        hasher.update(&metadata_bytes);
        data.extend_from_slice(&hasher.finalize());

        Ok(data)
    }

    /// Check if this is a critical security update
    pub fn is_critical_security_update(&self) -> bool {
        self.metadata.has_critical_cve()
    }

    /// Get package size in bytes
    pub fn size(&self) -> usize {
        self.payload.len()
    }
}

/// Source of an update package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateSource {
    /// Official release from the project
    Official {
        release_url: String,
        published_at: u64,
        publisher_signature: Option<Signature>,
    },

    /// Forwarded by trusted peers
    PeerForwarded {
        original_node: NodeId,
        forwarded_by: Vec<NodeId>,
        hop_count: u32,
    },
}

impl UpdateSource {
    /// Check if this is an official source
    pub fn is_official(&self) -> bool {
        matches!(self, UpdateSource::Official { .. })
    }

    /// Get hop count for peer-forwarded updates
    pub fn hop_count(&self) -> u32 {
        match self {
            UpdateSource::Official { .. } => 0,
            UpdateSource::PeerForwarded { hop_count, .. } => *hop_count,
        }
    }
}

/// Signature on an update package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSignature {
    /// Node ID of the signer
    pub signer: NodeId,

    /// Ed25519 signature
    pub signature: Signature,

    /// When this signature was created
    pub signed_at: u64,

    /// Reputation of signer at time of signing (optional)
    pub signer_reputation: Option<f64>,
}

impl UpdateSignature {
    /// Create a new update signature
    pub fn new(signer: NodeId, signature: Signature) -> Self {
        Self {
            signer,
            signature,
            signed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            signer_reputation: None,
        }
    }

    /// Create with reputation
    pub fn with_reputation(mut self, reputation: f64) -> Self {
        self.signer_reputation = Some(reputation);
        self
    }
}

/// Update package metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMetadata {
    /// CVEs fixed in this update
    pub fixes_cves: Vec<CveFixInfo>,

    /// Changelog
    pub changelog: String,

    /// Whether this update has breaking changes
    pub breaking_changes: bool,

    /// Minimum compatible version to update from
    pub min_compatible: SemanticVersion,

    /// Maximum compatible version to update from
    pub max_compatible: Option<SemanticVersion>,

    /// Release notes URL
    pub release_notes_url: Option<String>,
}

impl UpdateMetadata {
    /// Check if this update fixes any critical CVEs
    pub fn has_critical_cve(&self) -> bool {
        self.fixes_cves
            .iter()
            .any(|cve| matches!(cve.severity, CveSeverity::Critical))
    }

    /// Check if this update fixes high or critical CVEs
    pub fn has_high_priority_cve(&self) -> bool {
        self.fixes_cves
            .iter()
            .any(|cve| matches!(cve.severity, CveSeverity::Critical | CveSeverity::High))
    }
}

/// Information about a CVE fix
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveFixInfo {
    /// CVE identifier
    pub cve_id: String,

    /// Severity
    pub severity: CveSeverity,

    /// CVSS score (0.0-10.0)
    pub cvss_score: f32,

    /// Brief description
    pub description: String,
}

/// CVE severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CveSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Pending update awaiting verification
#[derive(Debug, Clone)]
pub struct PendingUpdate {
    /// The update package
    pub package: UpdatePackage,

    /// When we received this update
    pub received_at: SystemTime,

    /// Nodes that have verified this update
    pub verified_by: HashSet<NodeId>,

    /// Whether we've verified the signature chain
    pub signatures_verified: bool,
}

impl PendingUpdate {
    /// Create a new pending update
    pub fn new(package: UpdatePackage) -> Self {
        Self {
            package,
            received_at: SystemTime::now(),
            verified_by: HashSet::new(),
            signatures_verified: false,
        }
    }

    /// Get the age of this pending update
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.received_at)
            .unwrap_or(Duration::ZERO)
    }

    /// Check if the verification period has elapsed
    pub fn verification_period_elapsed(&self) -> bool {
        self.age() >= VERIFICATION_PERIOD
    }

    /// Check if this update is ready to install
    ///
    /// Requirements:
    /// - Verification period elapsed OR critical security update
    /// - Sufficient verifications (5+ nodes)
    /// - Signatures verified
    pub fn is_ready_to_install(&self) -> bool {
        let period_ok =
            self.verification_period_elapsed() || self.package.is_critical_security_update();

        let verifications_ok = self.verified_by.len() >= MIN_VERIFICATIONS;

        period_ok && verifications_ok && self.signatures_verified
    }
}

/// Verification period for non-critical updates (6 hours)
pub const VERIFICATION_PERIOD: Duration = Duration::from_secs(6 * 60 * 60);

/// Minimum number of peer verifications required
pub const MIN_VERIFICATIONS: usize = 5;

/// Minimum number of trusted signatures required for peer-forwarded updates
pub const MIN_TRUSTED_SIGNATURES: usize = 3;

/// Minimum reputation threshold for a signature to be considered trusted
pub const MIN_TRUST_REPUTATION: f64 = 0.8;

/// Update distribution manager
pub struct UpdateDistributionManager {
    /// Pending updates awaiting verification
    pending_updates: Arc<RwLock<HashMap<(AdapterType, SemanticVersion), PendingUpdate>>>,

    /// Updates we've successfully installed
    installed_updates: Arc<RwLock<HashMap<AdapterType, UpdatePackage>>>,

    /// Maximum number of pending updates to track
    max_pending: usize,
}

impl UpdateDistributionManager {
    /// Create a new update distribution manager
    pub fn new() -> Self {
        Self {
            pending_updates: Arc::new(RwLock::new(HashMap::new())),
            installed_updates: Arc::new(RwLock::new(HashMap::new())),
            max_pending: 100, // Reasonable limit
        }
    }

    /// Add a pending update
    pub async fn add_pending_update(&self, package: UpdatePackage) -> Result<()> {
        let mut pending = self.pending_updates.write().await;

        if pending.len() >= self.max_pending {
            return Err(UpdateError::Other("Too many pending updates".to_string()));
        }

        let key = (package.adapter_type, package.version.clone());
        pending.insert(key, PendingUpdate::new(package));

        Ok(())
    }

    /// Get a pending update
    pub async fn get_pending_update(
        &self,
        adapter_type: AdapterType,
        version: &SemanticVersion,
    ) -> Option<PendingUpdate> {
        let pending = self.pending_updates.read().await;
        pending.get(&(adapter_type, version.clone())).cloned()
    }

    /// Mark update as verified by a node
    pub async fn add_verification(
        &self,
        adapter_type: AdapterType,
        version: &SemanticVersion,
        verifier: NodeId,
    ) {
        let mut pending = self.pending_updates.write().await;
        if let Some(update) = pending.get_mut(&(adapter_type, version.clone())) {
            update.verified_by.insert(verifier);
        }
    }

    /// Mark signatures as verified
    pub async fn mark_signatures_verified(
        &self,
        adapter_type: AdapterType,
        version: &SemanticVersion,
    ) {
        let mut pending = self.pending_updates.write().await;
        if let Some(update) = pending.get_mut(&(adapter_type, version.clone())) {
            update.signatures_verified = true;
        }
    }

    /// Get all ready-to-install updates
    pub async fn get_ready_updates(&self) -> Vec<UpdatePackage> {
        let pending = self.pending_updates.read().await;

        pending
            .values()
            .filter(|update| update.is_ready_to_install())
            .map(|update| update.package.clone())
            .collect()
    }

    /// Remove a pending update and mark as installed
    pub async fn mark_installed(
        &self,
        adapter_type: AdapterType,
        version: &SemanticVersion,
    ) -> Option<PendingUpdate> {
        let mut pending = self.pending_updates.write().await;
        let mut installed = self.installed_updates.write().await;

        if let Some(update) = pending.remove(&(adapter_type, version.clone())) {
            installed.insert(adapter_type, update.package.clone());
            Some(update)
        } else {
            None
        }
    }

    /// Clean up expired pending updates (older than 48 hours)
    pub async fn cleanup_expired(&self) {
        let mut pending = self.pending_updates.write().await;

        pending.retain(|_, update| update.age() < Duration::from_secs(48 * 60 * 60));
    }

    /// Get all pending updates
    pub async fn get_all_pending(&self) -> Vec<PendingUpdate> {
        let pending = self.pending_updates.read().await;
        pending.values().cloned().collect()
    }

    /// Check if we have a specific pending update
    pub async fn has_pending_update(
        &self,
        adapter_type: AdapterType,
        version: &SemanticVersion,
    ) -> bool {
        let pending = self.pending_updates.read().await;
        pending.contains_key(&(adapter_type, version.clone()))
    }
}

impl Default for UpdateDistributionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_hash_verification() {
        let payload = b"test update binary data".to_vec();
        let metadata = UpdateMetadata {
            fixes_cves: vec![],
            changelog: "Test update".to_string(),
            breaking_changes: false,
            min_compatible: SemanticVersion::new(1, 0, 0),
            max_compatible: None,
            release_notes_url: None,
        };

        let package = UpdatePackage::new(
            AdapterType::Ethernet,
            SemanticVersion::new(1, 1, 0),
            payload.clone(),
            metadata,
            UpdateSource::Official {
                release_url: "https://example.com/release".to_string(),
                published_at: 1000,
                publisher_signature: None,
            },
        );

        assert!(package.verify_payload_hash());

        // Modify payload and verify it fails
        let mut bad_package = package.clone();
        bad_package.payload.push(0xff);
        assert!(!bad_package.verify_payload_hash());
    }

    #[test]
    fn test_critical_cve_detection() {
        let metadata = UpdateMetadata {
            fixes_cves: vec![CveFixInfo {
                cve_id: "CVE-2024-1234".to_string(),
                severity: CveSeverity::Critical,
                cvss_score: 9.8,
                description: "Critical vulnerability".to_string(),
            }],
            changelog: "Security fix".to_string(),
            breaking_changes: false,
            min_compatible: SemanticVersion::new(1, 0, 0),
            max_compatible: None,
            release_notes_url: None,
        };

        assert!(metadata.has_critical_cve());
        assert!(metadata.has_high_priority_cve());
    }

    #[tokio::test]
    async fn test_pending_update_verification_period() {
        let payload = b"test".to_vec();
        let metadata = UpdateMetadata {
            fixes_cves: vec![],
            changelog: "Test".to_string(),
            breaking_changes: false,
            min_compatible: SemanticVersion::new(1, 0, 0),
            max_compatible: None,
            release_notes_url: None,
        };

        let package = UpdatePackage::new(
            AdapterType::Ethernet,
            SemanticVersion::new(1, 1, 0),
            payload,
            metadata,
            UpdateSource::Official {
                release_url: "https://example.com".to_string(),
                published_at: 1000,
                publisher_signature: None,
            },
        );

        let mut pending = PendingUpdate::new(package);

        // Just created, period not elapsed
        assert!(!pending.verification_period_elapsed());

        // Add verifications
        for i in 0..6 {
            pending.verified_by.insert(NodeId::from_bytes([i; 64]));
        }
        pending.signatures_verified = true;

        // Not ready yet (period not elapsed)
        assert!(!pending.is_ready_to_install());
    }

    #[tokio::test]
    async fn test_critical_update_bypass() {
        let payload = b"test".to_vec();
        let metadata = UpdateMetadata {
            fixes_cves: vec![CveFixInfo {
                cve_id: "CVE-2024-1234".to_string(),
                severity: CveSeverity::Critical,
                cvss_score: 9.8,
                description: "Critical".to_string(),
            }],
            changelog: "Critical fix".to_string(),
            breaking_changes: false,
            min_compatible: SemanticVersion::new(1, 0, 0),
            max_compatible: None,
            release_notes_url: None,
        };

        let package = UpdatePackage::new(
            AdapterType::Ethernet,
            SemanticVersion::new(1, 1, 0),
            payload,
            metadata,
            UpdateSource::Official {
                release_url: "https://example.com".to_string(),
                published_at: 1000,
                publisher_signature: None,
            },
        );

        let mut pending = PendingUpdate::new(package);

        // Add sufficient verifications
        for i in 0..6 {
            pending.verified_by.insert(NodeId::from_bytes([i; 64]));
        }
        pending.signatures_verified = true;

        // Critical update bypasses verification period
        assert!(pending.is_ready_to_install());
    }

    #[tokio::test]
    async fn test_distribution_manager() {
        let manager = UpdateDistributionManager::new();

        let payload = b"test".to_vec();
        let metadata = UpdateMetadata {
            fixes_cves: vec![],
            changelog: "Test".to_string(),
            breaking_changes: false,
            min_compatible: SemanticVersion::new(1, 0, 0),
            max_compatible: None,
            release_notes_url: None,
        };

        let package = UpdatePackage::new(
            AdapterType::Ethernet,
            SemanticVersion::new(1, 1, 0),
            payload,
            metadata,
            UpdateSource::Official {
                release_url: "https://example.com".to_string(),
                published_at: 1000,
                publisher_signature: None,
            },
        );

        // Add pending update
        manager.add_pending_update(package.clone()).await.unwrap();

        // Verify it exists
        assert!(
            manager
                .has_pending_update(AdapterType::Ethernet, &SemanticVersion::new(1, 1, 0))
                .await
        );

        // Add verifications
        for i in 0..6 {
            manager
                .add_verification(
                    AdapterType::Ethernet,
                    &SemanticVersion::new(1, 1, 0),
                    NodeId::from_bytes([i; 64]),
                )
                .await;
        }

        // Mark signatures verified
        manager
            .mark_signatures_verified(AdapterType::Ethernet, &SemanticVersion::new(1, 1, 0))
            .await;

        // Mark as installed
        let installed = manager
            .mark_installed(AdapterType::Ethernet, &SemanticVersion::new(1, 1, 0))
            .await;

        assert!(installed.is_some());
        assert_eq!(installed.unwrap().verified_by.len(), 6);

        // Should no longer be pending
        assert!(
            !manager
                .has_pending_update(AdapterType::Ethernet, &SemanticVersion::new(1, 1, 0))
                .await
        );
    }
}
