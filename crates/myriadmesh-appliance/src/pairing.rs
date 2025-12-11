//! Secure pairing protocol for mobile devices

use crate::types::{ApplianceError, ApplianceResult};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Pairing methods supported
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PairingMethod {
    QrCode,
    Pin,
}

/// Pairing request from mobile device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequest {
    pub device_id: String,
    pub public_key: Vec<u8>,
    pub method: PairingMethod,
    pub timestamp: i64,
}

/// Pairing token for QR code or PIN exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingToken {
    pub token: String,
    pub challenge: Vec<u8>,
    pub node_id: String,
    pub timestamp: i64,
    pub expires_at: i64,
    pub signature: Vec<u8>,
}

impl PairingToken {
    /// Create a new pairing token
    pub fn new(node_id: String, signing_key: &SigningKey) -> Self {
        let token = Uuid::new_v4().to_string();
        let mut challenge = [0u8; 32];
        OsRng.fill_bytes(&mut challenge);

        let now = Utc::now();
        let timestamp = now.timestamp();
        let expires_at = (now + Duration::minutes(5)).timestamp();

        // Sign the token
        let message = format!("{}{}{}", token, node_id, timestamp);
        let signature = signing_key.sign(message.as_bytes()).to_bytes().to_vec();

        Self {
            token,
            challenge: challenge.to_vec(),
            node_id,
            timestamp,
            expires_at,
            signature,
        }
    }

    /// Verify token signature
    pub fn verify(&self, public_key: &VerifyingKey) -> ApplianceResult<()> {
        let message = format!("{}{}{}", self.token, self.node_id, self.timestamp);
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| ApplianceError::SignatureVerificationFailed)?;

        public_key
            .verify(message.as_bytes(), &signature)
            .map_err(|_| ApplianceError::SignatureVerificationFailed)?;

        Ok(())
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.expires_at
    }

    /// Generate QR code data
    pub fn to_qr_data(&self) -> ApplianceResult<String> {
        serde_json::to_string(self).map_err(ApplianceError::Serialization)
    }
}

/// Pairing challenge response from mobile device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingResponse {
    pub pairing_token: String,
    pub challenge_signature: Vec<u8>,
}

/// Result of a pairing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingResult {
    pub success: bool,
    pub session_token: Option<String>,
    pub error: Option<String>,
}

/// Pending pairing information
#[derive(Debug, Clone)]
struct PendingPairing {
    pub request: PairingRequest,
    pub token: PairingToken,
    pub created_at: DateTime<Utc>,
    pub approved: bool,
}

/// Pairing manager handles device pairing operations
pub struct PairingManager {
    pending: RwLock<HashMap<String, PendingPairing>>,
    signing_key: SigningKey,
    node_id: String,
    require_approval: bool,
}

impl PairingManager {
    /// Create a new pairing manager
    pub fn new(signing_key: SigningKey, node_id: String, require_approval: bool) -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            signing_key,
            node_id,
            require_approval,
        }
    }

    /// Initiate a pairing request
    pub async fn initiate_pairing(&self, request: PairingRequest) -> ApplianceResult<PairingToken> {
        // Validate request
        if request.device_id.is_empty() {
            return Err(ApplianceError::Configuration(
                "Device ID cannot be empty".to_string(),
            ));
        }

        if request.public_key.len() != 32 {
            return Err(ApplianceError::Configuration(
                "Invalid public key length".to_string(),
            ));
        }

        // Generate pairing token
        let token = PairingToken::new(self.node_id.clone(), &self.signing_key);

        // Store pending pairing
        let pending = PendingPairing {
            request: request.clone(),
            token: token.clone(),
            created_at: Utc::now(),
            approved: !self.require_approval, // Auto-approve if not required
        };

        let mut pending_map = self.pending.write().await;
        pending_map.insert(token.token.clone(), pending);

        Ok(token)
    }

    /// Approve a pending pairing (manual approval)
    pub async fn approve_pairing(&self, token: &str) -> ApplianceResult<()> {
        let mut pending_map = self.pending.write().await;

        let pairing = pending_map
            .get_mut(token)
            .ok_or_else(|| ApplianceError::InvalidPairingToken(token.to_string()))?;

        if pairing.token.is_expired() {
            pending_map.remove(token);
            return Err(ApplianceError::PairingExpired);
        }

        pairing.approved = true;
        Ok(())
    }

    /// Complete pairing with challenge response
    pub async fn complete_pairing(
        &self,
        response: PairingResponse,
    ) -> ApplianceResult<PairingResult> {
        let mut pending_map = self.pending.write().await;

        let pending = pending_map
            .get(&response.pairing_token)
            .ok_or_else(|| ApplianceError::InvalidPairingToken(response.pairing_token.clone()))?;

        // Check if expired
        if pending.token.is_expired() {
            pending_map.remove(&response.pairing_token);
            return Ok(PairingResult {
                success: false,
                session_token: None,
                error: Some("Pairing token expired".to_string()),
            });
        }

        // Check if approved
        if !pending.approved {
            return Ok(PairingResult {
                success: false,
                session_token: None,
                error: Some("Pairing not yet approved".to_string()),
            });
        }

        // Verify challenge signature
        let device_public_key = VerifyingKey::from_bytes(
            &pending
                .request
                .public_key
                .clone()
                .try_into()
                .map_err(|_| ApplianceError::Crypto("Invalid public key format".to_string()))?,
        )
        .map_err(|_| ApplianceError::Crypto("Failed to parse public key".to_string()))?;

        let challenge_sig = Signature::from_slice(&response.challenge_signature)
            .map_err(|_| ApplianceError::SignatureVerificationFailed)?;

        device_public_key
            .verify(&pending.token.challenge, &challenge_sig)
            .map_err(|_| ApplianceError::SignatureVerificationFailed)?;

        // Generate session token
        let session_token = Uuid::new_v4().to_string();

        // Remove from pending
        pending_map.remove(&response.pairing_token);

        Ok(PairingResult {
            success: true,
            session_token: Some(session_token),
            error: None,
        })
    }

    /// Get pending pairing requests (for UI display)
    pub async fn list_pending(&self) -> Vec<PairingRequestInfo> {
        let pending_map = self.pending.read().await;

        pending_map
            .iter()
            .filter(|(_, p)| !p.token.is_expired())
            .map(|(token, p)| PairingRequestInfo {
                token: token.clone(),
                device_id: p.request.device_id.clone(),
                method: p.request.method.clone(),
                created_at: p.created_at.timestamp(),
                approved: p.approved,
            })
            .collect()
    }

    /// Cleanup expired pending pairings
    pub async fn cleanup_expired(&self) {
        let mut pending_map = self.pending.write().await;
        pending_map.retain(|_, p| !p.token.is_expired());
    }

    /// Reject a pending pairing
    pub async fn reject_pairing(&self, token: &str) -> ApplianceResult<()> {
        let mut pending_map = self.pending.write().await;
        pending_map.remove(token);
        Ok(())
    }
}

/// Pairing request information for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequestInfo {
    pub token: String,
    pub device_id: String,
    pub method: PairingMethod,
    pub created_at: i64,
    pub approved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use rand::RngCore;

    #[tokio::test]
    async fn test_pairing_flow() {
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let node_id = "test-node-123".to_string();
        let manager = PairingManager::new(signing_key.clone(), node_id.clone(), false);

        // Mobile device generates keys
        let mut device_key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut device_key_bytes);
        let device_signing_key = SigningKey::from_bytes(&device_key_bytes);
        let device_public_key = device_signing_key.verifying_key().to_bytes().to_vec();

        // Initiate pairing
        let request = PairingRequest {
            device_id: "mobile-device-1".to_string(),
            public_key: device_public_key,
            method: PairingMethod::QrCode,
            timestamp: Utc::now().timestamp(),
        };

        let token = manager.initiate_pairing(request).await.unwrap();
        assert!(!token.token.is_empty());
        assert!(!token.is_expired());

        // Mobile device signs challenge
        let challenge_sig = device_signing_key.sign(&token.challenge);

        // Complete pairing
        let response = PairingResponse {
            pairing_token: token.token.clone(),
            challenge_signature: challenge_sig.to_bytes().to_vec(),
        };

        let result = manager.complete_pairing(response).await.unwrap();
        assert!(result.success);
        assert!(result.session_token.is_some());
    }

    #[tokio::test]
    async fn test_pairing_with_approval() {
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let node_id = "test-node-123".to_string();
        let manager = PairingManager::new(signing_key.clone(), node_id.clone(), true);

        let mut device_key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut device_key_bytes);
        let device_signing_key = SigningKey::from_bytes(&device_key_bytes);
        let device_public_key = device_signing_key.verifying_key().to_bytes().to_vec();

        let request = PairingRequest {
            device_id: "mobile-device-1".to_string(),
            public_key: device_public_key,
            method: PairingMethod::QrCode,
            timestamp: Utc::now().timestamp(),
        };

        let token = manager.initiate_pairing(request).await.unwrap();

        // Try to complete without approval
        let challenge_sig = device_signing_key.sign(&token.challenge);
        let response = PairingResponse {
            pairing_token: token.token.clone(),
            challenge_signature: challenge_sig.to_bytes().to_vec(),
        };

        let result = manager.complete_pairing(response.clone()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not yet approved"));

        // Approve pairing
        manager.approve_pairing(&token.token).await.unwrap();

        // Now complete should succeed
        let result = manager.complete_pairing(response).await.unwrap();
        assert!(result.success);
    }
}
