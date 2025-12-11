//! Block structure and validation
//!
//! Implements the blockchain block structure with:
//! - Block header (version, height, timestamp, previous hash, merkle root, signature)
//! - Block body (entries)
//! - Block validation logic

use blake2::{Blake2b512, Digest};
use chrono::{DateTime, Utc};
use myriadmesh_crypto::signing::Signature;
use myriadmesh_protocol::NodeId;
use serde::{Deserialize, Serialize};

use crate::entry::LedgerEntry;
use crate::error::{LedgerError, Result};
use crate::merkle::{calculate_merkle_root, MerkleRoot};

/// Block version number
pub const BLOCK_VERSION: u32 = 1;

/// Size of block hash (32 bytes)
pub const BLOCK_HASH_SIZE: usize = 32;

/// A block hash
pub type BlockHash = [u8; BLOCK_HASH_SIZE];

/// A block in the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// Block entries
    pub entries: Vec<LedgerEntry>,
    /// Consensus signatures from validators
    pub validator_signatures: Vec<ValidatorSignature>,
}

/// Block header containing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Block format version
    pub version: u32,
    /// Block number in the chain (0 = genesis)
    pub height: u64,
    /// Block creation timestamp
    pub timestamp: DateTime<Utc>,
    /// Hash of the previous block (all zeros for genesis)
    pub previous_hash: BlockHash,
    /// Merkle root of all entries
    pub merkle_root: MerkleRoot,
    /// Node ID of the block creator
    pub creator: NodeId,
    /// Signature of the block creator
    pub creator_signature: Signature,
}

/// A validator's signature on a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSignature {
    /// Node ID of the validator
    pub validator: NodeId,
    /// Validator's signature
    pub signature: Signature,
}

impl Block {
    /// Create a new block
    pub fn new(
        height: u64,
        previous_hash: BlockHash,
        creator: NodeId,
        creator_signature: Signature,
        entries: Vec<LedgerEntry>,
    ) -> Result<Self> {
        // Calculate Merkle root from entries
        let entry_bytes: Vec<Vec<u8>> = entries
            .iter()
            .map(|e| e.to_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| LedgerError::SerializationError(e.to_string()))?;

        let merkle_root = calculate_merkle_root(&entry_bytes);

        let header = BlockHeader {
            version: BLOCK_VERSION,
            height,
            timestamp: Utc::now(),
            previous_hash,
            merkle_root,
            creator,
            creator_signature,
        };

        Ok(Block {
            header,
            entries,
            validator_signatures: Vec::new(),
        })
    }

    /// Create the genesis block (height 0, no previous hash)
    pub fn genesis(creator: NodeId, creator_signature: Signature) -> Result<Self> {
        Self::new(
            0,
            [0u8; BLOCK_HASH_SIZE],
            creator,
            creator_signature,
            vec![],
        )
    }

    /// Calculate the hash of this block
    pub fn calculate_hash(&self) -> Result<BlockHash> {
        let header_bytes = bincode::serialize(&self.header)
            .map_err(|e| LedgerError::SerializationError(e.to_string()))?;

        let mut hasher = Blake2b512::new();
        hasher.update(&header_bytes);
        let hash = hasher.finalize();

        let mut result = [0u8; BLOCK_HASH_SIZE];
        result.copy_from_slice(&hash[..BLOCK_HASH_SIZE]);
        Ok(result)
    }

    /// Get the hash as a hex string
    pub fn hash_hex(&self) -> Result<String> {
        Ok(hex::encode(self.calculate_hash()?))
    }

    /// Add a validator signature
    pub fn add_validator_signature(&mut self, validator: NodeId, signature: Signature) {
        self.validator_signatures.push(ValidatorSignature {
            validator,
            signature,
        });
    }

    /// Validate the block structure and merkle root
    pub fn validate_structure(&self) -> Result<()> {
        // Check version
        if self.header.version != BLOCK_VERSION {
            return Err(LedgerError::ValidationFailed(format!(
                "unsupported block version: {}",
                self.header.version
            )));
        }

        // Calculate and verify merkle root
        let entry_bytes: Vec<Vec<u8>> = self
            .entries
            .iter()
            .map(|e| e.to_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| LedgerError::SerializationError(e.to_string()))?;

        let calculated_root = calculate_merkle_root(&entry_bytes);
        if calculated_root != self.header.merkle_root {
            return Err(LedgerError::MerkleRootMismatch {
                expected: hex::encode(self.header.merkle_root),
                actual: hex::encode(calculated_root),
            });
        }

        Ok(())
    }

    /// Validate that this block follows the previous block
    pub fn validate_chain_link(&self, previous_block: &Block) -> Result<()> {
        // Check height is sequential
        if self.header.height != previous_block.header.height + 1 {
            return Err(LedgerError::InvalidHeight {
                expected: previous_block.header.height + 1,
                actual: self.header.height,
            });
        }

        // Check previous hash matches
        let previous_hash = previous_block.calculate_hash()?;
        if self.header.previous_hash != previous_hash {
            return Err(LedgerError::HashMismatch {
                expected: hex::encode(previous_hash),
                actual: hex::encode(self.header.previous_hash),
            });
        }

        // Check timestamp is after previous block
        if self.header.timestamp <= previous_block.header.timestamp {
            return Err(LedgerError::ValidationFailed(format!(
                "block timestamp {} is not after previous block timestamp {}",
                self.header.timestamp, previous_block.header.timestamp
            )));
        }

        Ok(())
    }

    /// Check if this block has sufficient validator signatures for consensus
    pub fn has_consensus(&self, total_known_nodes: usize) -> bool {
        // Need ceiling of 2/3 (e.g., 10 nodes needs 7, not 6)
        let required = (total_known_nodes * 2).div_ceil(3);
        self.validator_signatures.len() >= required
    }

    /// Get the number of validator signatures
    pub fn signature_count(&self) -> usize {
        self.validator_signatures.len()
    }

    /// Serialize block to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| LedgerError::SerializationError(e.to_string()))
    }

    /// Deserialize block from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes).map_err(|e| LedgerError::DeserializationError(e.to_string()))
    }
}

impl BlockHeader {
    /// Get the bytes for signing
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| LedgerError::SerializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use myriadmesh_protocol::types::NODE_ID_SIZE;

    fn create_test_signature() -> Signature {
        Signature::from_bytes([0u8; 64])
    }

    fn create_test_node_id(val: u8) -> NodeId {
        NodeId::from_bytes([val; NODE_ID_SIZE])
    }

    #[test]
    fn test_genesis_block() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let genesis = Block::genesis(creator, signature).unwrap();

        assert_eq!(genesis.header.height, 0);
        assert_eq!(genesis.header.previous_hash, [0u8; BLOCK_HASH_SIZE]);
        assert!(genesis.entries.is_empty());
        assert_eq!(genesis.header.version, BLOCK_VERSION);
    }

    #[test]
    fn test_block_hash_calculation() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let block = Block::genesis(creator, signature).unwrap();
        let hash1 = block.calculate_hash().unwrap();
        let hash2 = block.calculate_hash().unwrap();

        // Hash should be deterministic
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), BLOCK_HASH_SIZE);
    }

    #[test]
    fn test_block_structure_validation() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let block = Block::genesis(creator, signature).unwrap();

        // Should validate successfully
        assert!(block.validate_structure().is_ok());
    }

    #[test]
    fn test_chain_link_validation() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let genesis = Block::genesis(creator, signature).unwrap();
        let genesis_hash = genesis.calculate_hash().unwrap();

        let block2 = Block::new(1, genesis_hash, creator, signature, vec![]).unwrap();

        // Should validate successfully
        assert!(block2.validate_chain_link(&genesis).is_ok());
    }

    #[test]
    fn test_chain_link_validation_wrong_height() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let genesis = Block::genesis(creator, signature).unwrap();
        let genesis_hash = genesis.calculate_hash().unwrap();

        // Wrong height (should be 1, but we use 2)
        let block2 = Block::new(2, genesis_hash, creator, signature, vec![]).unwrap();

        // Should fail validation
        assert!(block2.validate_chain_link(&genesis).is_err());
    }

    #[test]
    fn test_chain_link_validation_wrong_hash() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let genesis = Block::genesis(creator, signature).unwrap();

        // Wrong previous hash
        let block2 = Block::new(1, [0xFF; BLOCK_HASH_SIZE], creator, signature, vec![]).unwrap();

        // Should fail validation
        assert!(block2.validate_chain_link(&genesis).is_err());
    }

    #[test]
    fn test_validator_signatures() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let mut block = Block::genesis(creator, signature).unwrap();

        assert_eq!(block.signature_count(), 0);

        // Add validator signatures
        block.add_validator_signature(create_test_node_id(2), signature);
        assert_eq!(block.signature_count(), 1);

        block.add_validator_signature(create_test_node_id(3), signature);
        assert_eq!(block.signature_count(), 2);
    }

    #[test]
    fn test_consensus_check() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let mut block = Block::genesis(creator, signature).unwrap();

        // With 10 total nodes, need 7 signatures (2/3 of 10 = 6.67 -> 7)
        assert!(!block.has_consensus(10));

        // Add 6 signatures - not enough
        for i in 0..6 {
            block.add_validator_signature(create_test_node_id(i + 2), signature);
        }
        assert!(!block.has_consensus(10));

        // Add one more - now we have consensus
        block.add_validator_signature(create_test_node_id(8), signature);
        assert!(block.has_consensus(10));
    }

    #[test]
    fn test_block_serialization() {
        let creator = create_test_node_id(1);
        let signature = create_test_signature();

        let block = Block::genesis(creator, signature).unwrap();

        let bytes = block.to_bytes().unwrap();
        let deserialized = Block::from_bytes(&bytes).unwrap();

        assert_eq!(block.header.height, deserialized.header.height);
        assert_eq!(block.header.version, deserialized.header.version);
        assert_eq!(block.header.creator, deserialized.header.creator);
    }

    #[test]
    fn test_different_blocks_different_hashes() {
        let creator1 = create_test_node_id(1);
        let creator2 = create_test_node_id(2);
        let signature = create_test_signature();

        let block1 = Block::genesis(creator1, signature).unwrap();
        let block2 = Block::genesis(creator2, signature).unwrap();

        let hash1 = block1.calculate_hash().unwrap();
        let hash2 = block2.calculate_hash().unwrap();

        // Different creators should produce different hashes
        assert_ne!(hash1, hash2);
    }
}
