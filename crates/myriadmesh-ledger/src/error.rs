//! Error types for the ledger module

use thiserror::Error;

/// Result type for ledger operations
pub type Result<T> = std::result::Result<T, LedgerError>;

/// Errors that can occur during ledger operations
#[derive(Error, Debug)]
pub enum LedgerError {
    /// Block validation failed
    #[error("block validation failed: {0}")]
    ValidationFailed(String),

    /// Block hash mismatch
    #[error("block hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    /// Invalid signature
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// Merkle root mismatch
    #[error("merkle root mismatch: expected {expected}, got {actual}")]
    MerkleRootMismatch { expected: String, actual: String },

    /// Insufficient signatures for consensus
    #[error("insufficient signatures: need {needed}, got {actual}")]
    InsufficientSignatures { needed: usize, actual: usize },

    /// Block not found
    #[error("block not found: height {0}")]
    BlockNotFound(u64),

    /// Invalid block height
    #[error("invalid block height: expected {expected}, got {actual}")]
    InvalidHeight { expected: u64, actual: u64 },

    /// Storage error
    #[error("storage error: {0}")]
    StorageError(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error
    #[error("deserialization error: {0}")]
    DeserializationError(String),

    /// Cryptographic error
    #[error("crypto error: {0}")]
    CryptoError(String),

    /// Invalid entry
    #[error("invalid entry: {0}")]
    InvalidEntry(String),

    /// Consensus error
    #[error("consensus error: {0}")]
    ConsensusError(String),

    /// Insufficient reputation
    #[error("insufficient reputation: node {node_id} has {reputation}, minimum {minimum}")]
    InsufficientReputation {
        node_id: String,
        reputation: f64,
        minimum: f64,
    },

    /// IO error
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    /// Bincode error
    #[error("bincode error: {0}")]
    BincodeError(#[from] bincode::Error),
}

impl From<myriadmesh_crypto::CryptoError> for LedgerError {
    fn from(err: myriadmesh_crypto::CryptoError) -> Self {
        LedgerError::CryptoError(err.to_string())
    }
}
