//! Signature verification for update packages
//!
//! Implements multi-signature verification with reputation-based trust.

use crate::distribution::{
    UpdatePackage, UpdateSignature, UpdateSource, MIN_TRUSTED_SIGNATURES, MIN_TRUST_REPUTATION,
};
use crate::{Result, UpdateError};
use blake2::{Blake2b512, Digest};
use myriadmesh_crypto::signing::{verify_signature, Signature};
use myriadmesh_protocol::NodeId;
use sodiumoxide::crypto::sign::ed25519::PublicKey;
use std::collections::HashMap;

/// Result of signature verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Total signatures on the package
    pub total_signatures: usize,

    /// Number of valid signatures
    pub valid_signatures: usize,

    /// Number of trusted signatures (high reputation)
    pub trusted_signatures: usize,

    /// Whether verification passed
    pub passed: bool,

    /// Reason for failure (if any)
    pub failure_reason: Option<String>,

    /// Breakdown by signer
    pub signer_details: HashMap<NodeId, SignerVerification>,
}

/// Verification details for a specific signer
#[derive(Debug, Clone)]
pub struct SignerVerification {
    /// Node ID of the signer
    pub node_id: NodeId,

    /// Whether the signature is valid
    pub signature_valid: bool,

    /// Reputation of the signer (if known)
    pub reputation: Option<f64>,

    /// Whether this signer is trusted
    pub trusted: bool,
}

/// Signature verifier with reputation tracking
pub struct SignatureVerifier {
    /// Callback to get a node's public key
    #[allow(clippy::type_complexity)]
    public_key_provider: Box<dyn Fn(&NodeId) -> Option<PublicKey> + Send + Sync>,

    /// Callback to get a node's reputation
    #[allow(clippy::type_complexity)]
    reputation_provider: Box<dyn Fn(&NodeId) -> Option<f64> + Send + Sync>,

    /// Publisher's public key for verifying official releases
    publisher_public_key: Option<PublicKey>,
}

impl SignatureVerifier {
    /// Create a new signature verifier
    pub fn new<K, R>(public_key_provider: K, reputation_provider: R) -> Self
    where
        K: Fn(&NodeId) -> Option<PublicKey> + Send + Sync + 'static,
        R: Fn(&NodeId) -> Option<f64> + Send + Sync + 'static,
    {
        Self {
            public_key_provider: Box::new(public_key_provider),
            reputation_provider: Box::new(reputation_provider),
            publisher_public_key: None,
        }
    }

    /// Set the publisher's public key for verifying official releases
    pub fn with_publisher_key(mut self, public_key: PublicKey) -> Self {
        self.publisher_public_key = Some(public_key);
        self
    }

    /// Verify the publisher's signature on an update package
    fn verify_publisher_signature(&self, payload_hash: &[u8], signature: &Signature) -> bool {
        if let Some(publisher_key) = &self.publisher_public_key {
            verify_signature(publisher_key, payload_hash, signature).is_ok()
        } else {
            // No publisher key configured, cannot verify
            false
        }
    }

    /// Verify all signatures on an update package
    pub fn verify_package(&self, package: &UpdatePackage) -> Result<VerificationResult> {
        // First, verify the payload hash
        if !package.verify_payload_hash() {
            let mut hasher = Blake2b512::new();
            hasher.update(&package.payload);
            let computed = hasher.finalize();
            return Err(UpdateError::HashMismatch {
                expected: hex::encode(package.payload_hash),
                actual: hex::encode(&computed[..]),
            });
        }

        // Get signable data
        let signable_data = package.get_signable_data()?;

        let mut signer_details = HashMap::new();
        let mut valid_count = 0;
        let mut trusted_count = 0;

        // Verify each signature
        for sig_info in &package.signatures {
            let verification = self.verify_signature(sig_info, &signable_data);

            if verification.signature_valid {
                valid_count += 1;
            }

            if verification.trusted {
                trusted_count += 1;
            }

            signer_details.insert(sig_info.signer, verification);
        }

        // Determine if verification passed
        let (passed, failure_reason) =
            self.evaluate_verification(package, valid_count, trusted_count);

        Ok(VerificationResult {
            total_signatures: package.signatures.len(),
            valid_signatures: valid_count,
            trusted_signatures: trusted_count,
            passed,
            failure_reason,
            signer_details,
        })
    }

    /// Verify a single signature
    fn verify_signature(
        &self,
        sig_info: &UpdateSignature,
        signable_data: &[u8],
    ) -> SignerVerification {
        // Get public key
        let public_key = match (self.public_key_provider)(&sig_info.signer) {
            Some(key) => key,
            None => {
                return SignerVerification {
                    node_id: sig_info.signer,
                    signature_valid: false,
                    reputation: None,
                    trusted: false,
                };
            }
        };

        // Verify signature
        let signature_valid =
            verify_signature(&public_key, signable_data, &sig_info.signature).is_ok();

        // Get reputation
        let reputation = (self.reputation_provider)(&sig_info.signer);

        // Check if trusted
        let trusted = signature_valid
            && reputation
                .map(|r| r >= MIN_TRUST_REPUTATION)
                .unwrap_or(false);

        SignerVerification {
            node_id: sig_info.signer,
            signature_valid,
            reputation,
            trusted,
        }
    }

    /// Evaluate whether the verification passed
    fn evaluate_verification(
        &self,
        package: &UpdatePackage,
        _valid_count: usize,
        trusted_count: usize,
    ) -> (bool, Option<String>) {
        // Check based on source type
        match &package.source {
            UpdateSource::Official {
                publisher_signature,
                ..
            } => {
                // For official sources, verify the publisher signature
                if let Some(sig) = publisher_signature {
                    if self.verify_publisher_signature(&package.payload_hash, sig) {
                        (true, None)
                    } else {
                        (
                            false,
                            Some("Publisher signature verification failed".to_string()),
                        )
                    }
                } else {
                    // If no publisher signature, fall back to peer verification
                    if trusted_count >= MIN_TRUSTED_SIGNATURES {
                        (true, None)
                    } else {
                        (
                            false,
                            Some(format!(
                                "Insufficient trusted signatures for official update ({}/{})",
                                trusted_count, MIN_TRUSTED_SIGNATURES
                            )),
                        )
                    }
                }
            }
            UpdateSource::PeerForwarded { .. } => {
                // Peer-forwarded updates require minimum trusted signatures
                if trusted_count >= MIN_TRUSTED_SIGNATURES {
                    (true, None)
                } else {
                    (
                        false,
                        Some(format!(
                            "Insufficient trusted signatures ({}/{} required)",
                            trusted_count, MIN_TRUSTED_SIGNATURES
                        )),
                    )
                }
            }
        }
    }

    /// Quick check if a package has enough signatures
    pub fn has_sufficient_signatures(&self, package: &UpdatePackage) -> bool {
        match package.source {
            UpdateSource::Official {
                ref publisher_signature,
                ..
            } => {
                publisher_signature.is_some() || package.signatures.len() >= MIN_TRUSTED_SIGNATURES
            }
            UpdateSource::PeerForwarded { .. } => {
                package.signatures.len() >= MIN_TRUSTED_SIGNATURES
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distribution::UpdateMetadata;
    use myriadmesh_crypto::signing::sign_message;
    use myriadmesh_network::version_tracking::SemanticVersion;
    use myriadmesh_protocol::types::AdapterType;

    fn create_test_package() -> UpdatePackage {
        let payload = b"test update data".to_vec();
        let metadata = UpdateMetadata {
            fixes_cves: vec![],
            changelog: "Test update".to_string(),
            breaking_changes: false,
            min_compatible: SemanticVersion::new(1, 0, 0),
            max_compatible: None,
            release_notes_url: None,
        };

        UpdatePackage::new(
            AdapterType::Ethernet,
            SemanticVersion::new(1, 1, 0),
            payload,
            metadata,
            UpdateSource::PeerForwarded {
                original_node: NodeId::from_bytes([0u8; 64]),
                forwarded_by: vec![],
                hop_count: 0,
            },
        )
    }

    #[test]
    fn test_signature_verification() {
        use myriadmesh_crypto::identity::NodeIdentity;

        myriadmesh_crypto::init().unwrap();

        // Create test identities
        let identity1 = NodeIdentity::generate().unwrap();
        let identity2 = NodeIdentity::generate().unwrap();
        let identity3 = NodeIdentity::generate().unwrap();

        let node1 = NodeId::from_bytes(*identity1.node_id.as_bytes());
        let node2 = NodeId::from_bytes(*identity2.node_id.as_bytes());
        let node3 = NodeId::from_bytes(*identity3.node_id.as_bytes());

        // Create a test package
        let mut package = create_test_package();
        let signable_data = package.get_signable_data().unwrap();

        // Sign with all three identities
        let sig1 = sign_message(&identity1, &signable_data).unwrap();
        let sig2 = sign_message(&identity2, &signable_data).unwrap();
        let sig3 = sign_message(&identity3, &signable_data).unwrap();

        package.add_signature(UpdateSignature::new(node1, sig1).with_reputation(0.9));
        package.add_signature(UpdateSignature::new(node2, sig2).with_reputation(0.85));
        package.add_signature(UpdateSignature::new(node3, sig3).with_reputation(0.7));

        // Create verifier
        let pub_keys: HashMap<NodeId, PublicKey> = [
            (node1, identity1.public_key),
            (node2, identity2.public_key),
            (node3, identity3.public_key),
        ]
        .into_iter()
        .collect();

        let reputations: HashMap<NodeId, f64> = [(node1, 0.9), (node2, 0.85), (node3, 0.7)]
            .into_iter()
            .collect();

        let verifier = SignatureVerifier::new(
            move |node_id| pub_keys.get(node_id).cloned(),
            move |node_id| reputations.get(node_id).copied(),
        );

        // Verify
        let result = verifier.verify_package(&package).unwrap();

        assert_eq!(result.total_signatures, 3);
        assert_eq!(result.valid_signatures, 3);
        assert_eq!(result.trusted_signatures, 2); // Only node1 and node2 have rep >= 0.8
        assert!(!result.passed); // Need 3 trusted signatures
    }

    #[test]
    fn test_hash_mismatch_detection() {
        let mut package = create_test_package();

        // Tamper with payload
        package.payload.push(0xff);

        // Create dummy verifier
        let verifier = SignatureVerifier::new(|_| None, |_| None);

        // Should fail with hash mismatch
        let result = verifier.verify_package(&package);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UpdateError::HashMismatch { .. }
        ));
    }
}
