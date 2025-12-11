//! Update coordination and distribution system for MyriadMesh
//!
//! This crate provides:
//! - Coordinated update scheduling (Phase 4.6)
//! - Peer-assisted update distribution (Phase 4.7)
//! - Multi-signature verification
//! - Critical CVE priority override

pub mod coordination;
pub mod distribution;
pub mod schedule;
pub mod verification;

pub use coordination::UpdateCoordinator;
pub use distribution::{UpdateMetadata, UpdatePackage, UpdateSignature, UpdateSource};
pub use schedule::{
    UpdateSchedule, UpdateScheduleRequest, UpdateScheduleResponse, UpdateWindowSelector,
};
pub use verification::{SignatureVerifier, VerificationResult};

use thiserror::Error;

/// Error types for update system
#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Insufficient signatures: {0}")]
    InsufficientSignatures(String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("No fallback adapters available")]
    NoFallbackAdapters,

    #[error("Update schedule conflict: {0}")]
    ScheduleConflict(String),

    #[error("Verification period not elapsed")]
    VerificationPeriodNotElapsed,

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Cryptographic error: {0}")]
    Crypto(#[from] myriadmesh_crypto::error::CryptoError),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, UpdateError>;
