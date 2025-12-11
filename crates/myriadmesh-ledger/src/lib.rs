//! MyriadMesh Blockchain Ledger
//!
//! This module implements a lightweight blockchain ledger for the MyriadMesh network using
//! Proof of Participation (PoP) consensus.
//!
//! ## Features
//!
//! - **Block Structure**: Blocks with headers, entries, and validator signatures
//! - **Entry Types**: Discovery, Test, Message, and Key Exchange entries
//! - **Merkle Trees**: Efficient entry verification using BLAKE2b-512
//! - **Consensus**: Reputation-based Proof of Participation (PoP)
//! - **Storage**: Persistent block storage with pruning support
//! - **Synchronization**: DHT-based chain sync with fork resolution
//!
//! ## Consensus Mechanism
//!
//! The ledger uses Proof of Participation (PoP) consensus:
//! - Nodes earn reputation by relaying messages (50% weight)
//! - Uptime score contributes 30% to reputation
//! - Ledger participation contributes 20% to reputation
//! - Block creation rotates among high-reputation nodes
//! - Blocks require 2/3 majority signatures
//! - Fork resolution: longest chain with highest total reputation wins
//!
//! ## Example
//!
//! ```no_run
//! use myriadmesh_ledger::{
//!     Block, LedgerEntry, EntryType, DiscoveryEntry,
//!     ConsensusManager, LedgerStorage, ChainSync,
//!     StorageConfig,
//! };
//! use myriadmesh_protocol::{NodeId, types::{AdapterType, NODE_ID_SIZE}};
//! use myriadmesh_crypto::signing::Signature;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create storage
//! let config = StorageConfig::default();
//! let mut storage = LedgerStorage::new(config)?;
//!
//! // Create consensus manager
//! let mut consensus = ConsensusManager::with_defaults();
//!
//! // Create genesis block
//! let creator = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
//! let signature = Signature::from_bytes([0u8; 64]);
//! let genesis = Block::genesis(creator, signature)?;
//!
//! // Store genesis block
//! storage.store_block(&genesis)?;
//!
//! // Create chain sync
//! let mut sync = ChainSync::new(storage, consensus);
//! # Ok(())
//! # }
//! ```

pub mod block;
pub mod consensus;
pub mod entry;
pub mod error;
pub mod merkle;
pub mod storage;
pub mod sync;

// Re-export main types
pub use block::{Block, BlockHash, BlockHeader, ValidatorSignature, BLOCK_VERSION};
pub use consensus::{ConsensusManager, NodeReputation, DEFAULT_MIN_REPUTATION};
pub use entry::{
    DiscoveryEntry, EntryType, KeyExchangeEntry, LedgerEntry, MessageEntry, TestEntry,
};
pub use error::{LedgerError, Result};
pub use merkle::{calculate_merkle_root, verify_entry_inclusion, MerkleRoot, HASH_SIZE};
pub use storage::{LedgerStorage, StorageConfig, StorageStats, DEFAULT_KEEP_BLOCKS};
pub use sync::{ChainSync, SyncState, SyncStats};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use myriadmesh_crypto::signing::Signature;
    use myriadmesh_protocol::{types::NODE_ID_SIZE, NodeId};
    use tempfile::TempDir;

    fn create_test_node_id(val: u8) -> NodeId {
        NodeId::from_bytes([val; NODE_ID_SIZE])
    }

    #[test]
    fn test_full_ledger_workflow() {
        // Setup
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path()).without_pruning();
        let mut storage = LedgerStorage::new(config).unwrap();
        let mut consensus = ConsensusManager::with_defaults();

        // Create nodes with reputation
        let node1 = create_test_node_id(1);
        let node2 = create_test_node_id(2);

        let rep1 = consensus.get_reputation_mut(&node1);
        for _ in 0..10 {
            rep1.record_relay_success();
        }
        rep1.update_uptime(0.95);

        let rep2 = consensus.get_reputation_mut(&node2);
        for _ in 0..10 {
            rep2.record_relay_success();
        }
        rep2.update_uptime(0.90);

        // Create and store genesis block
        let signature = Signature::from_bytes([0u8; 64]);
        let genesis = Block::genesis(node1, signature).unwrap();
        storage.store_block(&genesis).unwrap();

        // Verify we can load it
        let loaded_genesis = storage.load_block(0).unwrap();
        assert_eq!(loaded_genesis.header.height, 0);
        assert_eq!(loaded_genesis.header.creator, node1);

        // Create block 1
        let genesis_hash = genesis.calculate_hash().unwrap();
        let mut block1 = Block::new(1, genesis_hash, node2, signature, vec![]).unwrap();

        // Add validator signature
        block1.add_validator_signature(node1, signature);

        // Store block 1
        storage.store_block(&block1).unwrap();

        // Verify chain height
        assert_eq!(storage.chain_height(), 1);

        // Verify we can load the latest block
        let latest = storage.get_latest_block().unwrap();
        assert_eq!(latest.header.height, 1);
        assert_eq!(latest.header.creator, node2);

        // Test chain sync
        let sync = ChainSync::new(storage, consensus);
        assert_eq!(sync.local_height(), 1);
        assert!(!sync.needs_sync(1));
        assert!(sync.needs_sync(10));
    }

    #[test]
    fn test_block_validation_chain() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path()).without_pruning();
        let mut storage = LedgerStorage::new(config).unwrap();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);

        // Create a chain of blocks
        let genesis = Block::genesis(creator, signature).unwrap();
        storage.store_block(&genesis).unwrap();

        let mut prev_hash = genesis.calculate_hash().unwrap();

        for i in 1..=5 {
            let block = Block::new(i, prev_hash, creator, signature, vec![]).unwrap();

            // Validate it links correctly
            let prev_block = storage.load_block(i - 1).unwrap();
            assert!(block.validate_chain_link(&prev_block).is_ok());

            prev_hash = block.calculate_hash().unwrap();
            storage.store_block(&block).unwrap();
        }

        assert_eq!(storage.chain_height(), 5);
        assert_eq!(storage.block_count(), 6); // Genesis + 5 blocks
    }

    #[test]
    fn test_consensus_block_creator_selection() {
        let mut consensus = ConsensusManager::new(0.5);

        // Add 5 nodes with varying reputations
        for i in 1..=5 {
            let node_id = create_test_node_id(i);
            let rep = consensus.get_reputation_mut(&node_id);

            // Give different reputation levels
            let relay_count = i as u64 * 2;
            for _ in 0..relay_count {
                rep.record_relay_success();
            }
            rep.update_uptime(0.8 + (i as f64 * 0.02));
        }

        // Get eligible creators
        let eligible = consensus.get_eligible_creators();
        assert!(!eligible.is_empty());

        // Test round-robin selection
        let creator0 = consensus.select_next_creator(0);
        let creator1 = consensus.select_next_creator(1);
        let creator2 = consensus.select_next_creator(2);

        assert!(creator0.is_some());
        assert!(creator1.is_some());
        assert!(creator2.is_some());

        // Should be deterministic
        let creator0_again = consensus.select_next_creator(0);
        assert_eq!(creator0, creator0_again);
    }

    #[test]
    fn test_ledger_entry_creation() {
        use entry::{DiscoveryEntry, EntryType};
        use myriadmesh_protocol::types::AdapterType;

        let node_id = create_test_node_id(1);
        let discovered_by = create_test_node_id(2);
        let public_key = [3u8; 32];
        let adapters = vec![AdapterType::Ethernet, AdapterType::Bluetooth];

        let discovery = DiscoveryEntry::new(node_id, public_key, adapters, discovered_by);
        let signature = Signature::from_bytes([0u8; 64]);
        let entry = LedgerEntry::new(EntryType::Discovery(discovery), signature);

        // Verify serialization
        let bytes = entry.to_bytes().unwrap();
        assert!(!bytes.is_empty());

        let deserialized: LedgerEntry = bincode::deserialize(&bytes).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_merkle_root_calculation() {
        let entry1 = b"entry1".to_vec();
        let entry2 = b"entry2".to_vec();
        let entry3 = b"entry3".to_vec();

        let entries = vec![entry1.clone(), entry2.clone(), entry3.clone()];
        let root = calculate_merkle_root(&entries);

        // Verify inclusion
        assert!(verify_entry_inclusion(&entries, &entry1, &root));
        assert!(verify_entry_inclusion(&entries, &entry2, &root));
        assert!(verify_entry_inclusion(&entries, &entry3, &root));

        // Verify non-inclusion
        let entry4 = b"entry4".to_vec();
        assert!(!verify_entry_inclusion(&entries, &entry4, &root));
    }

    #[test]
    fn test_storage_pruning_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path()).with_keep_blocks(3);
        let mut storage = LedgerStorage::new(config).unwrap();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);

        // Create 10 blocks
        let mut prev_hash = [0u8; 32];
        for i in 0..10 {
            let block = Block::new(i, prev_hash, creator, signature, vec![]).unwrap();
            prev_hash = block.calculate_hash().unwrap();
            storage.store_block(&block).unwrap();
        }

        // Should have pruned to keep only 3 most recent
        assert_eq!(storage.block_count(), 3);
        assert!(storage.has_block(7));
        assert!(storage.has_block(8));
        assert!(storage.has_block(9));
        assert!(!storage.has_block(0)); // Pruned
        assert!(!storage.has_block(6)); // Pruned
    }
}
