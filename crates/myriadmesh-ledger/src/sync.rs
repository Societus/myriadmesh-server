//! Chain synchronization
//!
//! Implements distributed blockchain synchronization using DHT:
//! - Block discovery through iterative DHT lookups
//! - On-demand block retrieval
//! - Fork detection and resolution
//! - Validation during sync

use std::collections::{HashSet, VecDeque};

use myriadmesh_protocol::NodeId;

use crate::block::Block;
use crate::consensus::ConsensusManager;
use crate::error::{LedgerError, Result};
use crate::storage::LedgerStorage;

/// Synchronization state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncState {
    /// Not synchronizing
    #[default]
    Idle,
    /// Discovering blocks
    Discovering,
    /// Downloading blocks
    Downloading,
    /// Validating downloaded blocks
    Validating,
    /// Synchronized
    Synced,
    /// Error occurred during sync
    Error,
}

/// Synchronization statistics
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    /// Number of blocks discovered
    pub blocks_discovered: u64,
    /// Number of blocks downloaded
    pub blocks_downloaded: u64,
    /// Number of blocks validated
    pub blocks_validated: u64,
    /// Number of validation errors
    pub validation_errors: u64,
    /// Current sync state
    pub state: SyncState,
}

/// Chain synchronization manager
pub struct ChainSync {
    /// Local storage
    storage: LedgerStorage,
    /// Consensus manager
    consensus: ConsensusManager,
    /// Current sync state
    state: SyncState,
    /// Synchronization statistics
    stats: SyncStats,
    /// Blocks pending validation
    pending_blocks: VecDeque<Block>,
    /// Nodes we've queried
    queried_nodes: HashSet<NodeId>,
}

impl ChainSync {
    /// Create a new chain synchronization manager
    pub fn new(storage: LedgerStorage, consensus: ConsensusManager) -> Self {
        Self {
            storage,
            consensus,
            state: SyncState::Idle,
            stats: SyncStats::default(),
            pending_blocks: VecDeque::new(),
            queried_nodes: HashSet::new(),
        }
    }

    /// Get the current sync state
    pub fn state(&self) -> SyncState {
        self.state
    }

    /// Get synchronization statistics
    pub fn stats(&self) -> &SyncStats {
        &self.stats
    }

    /// Check if we're synchronized
    pub fn is_synced(&self) -> bool {
        self.state == SyncState::Synced
    }

    /// Get the local chain height
    pub fn local_height(&self) -> u64 {
        self.storage.chain_height()
    }

    /// Add a discovered block to the pending queue
    ///
    /// This would typically be called when receiving blocks from peers via DHT
    pub fn add_pending_block(&mut self, block: Block) {
        self.pending_blocks.push_back(block);
        self.stats.blocks_discovered += 1;
    }

    /// Process pending blocks and validate them
    pub fn process_pending_blocks(&mut self) -> Result<usize> {
        if self.pending_blocks.is_empty() {
            return Ok(0);
        }

        self.state = SyncState::Validating;
        let mut processed = 0;

        // Sort pending blocks by height
        let mut sorted_blocks: Vec<Block> = self.pending_blocks.drain(..).collect();
        sorted_blocks.sort_by_key(|b| b.header.height);

        for block in sorted_blocks {
            match self.validate_and_store_block(block) {
                Ok(()) => {
                    processed += 1;
                    self.stats.blocks_validated += 1;
                }
                Err(e) => {
                    self.stats.validation_errors += 1;
                    // Log error but continue processing
                    eprintln!("Block validation error: {}", e);
                }
            }
        }

        if processed > 0 {
            self.state = SyncState::Synced;
        } else {
            self.state = SyncState::Error;
        }

        Ok(processed)
    }

    /// Validate and store a single block
    fn validate_and_store_block(&mut self, block: Block) -> Result<()> {
        // Validate block structure
        block.validate_structure()?;

        // Check if we already have this block
        if self.storage.has_block(block.header.height) {
            return Ok(()); // Already have it
        }

        // If not genesis, validate chain link
        if block.header.height > 0 {
            let previous_height = block.header.height - 1;

            // Check if we have the previous block
            if !self.storage.has_block(previous_height) {
                return Err(LedgerError::ValidationFailed(format!(
                    "missing previous block at height {}",
                    previous_height
                )));
            }

            let previous_block = self.storage.load_block(previous_height)?;
            block.validate_chain_link(&previous_block)?;
        }

        // Validate block creator has sufficient reputation
        self.consensus
            .validate_block_creator(&block.header.creator)?;

        // Validate consensus (2/3 majority)
        self.consensus.validate_consensus(&block)?;

        // Store the block
        self.storage.store_block(&block)?;
        self.stats.blocks_downloaded += 1;

        Ok(())
    }

    /// Request blocks from a specific height range
    ///
    /// This would typically trigger DHT queries to find and retrieve blocks.
    /// For now, it's a placeholder that tracks the request.
    pub fn request_blocks(&mut self, start_height: u64, end_height: u64) -> Result<()> {
        if start_height > end_height {
            return Err(LedgerError::ValidationFailed(
                "invalid height range".to_string(),
            ));
        }

        self.state = SyncState::Downloading;

        // In a real implementation, this would:
        // 1. Query DHT for blocks in the range
        // 2. Connect to peers that have the blocks
        // 3. Download blocks via network protocol
        // 4. Add blocks to pending_blocks queue

        // For now, we just update state
        Ok(())
    }

    /// Detect and resolve forks
    ///
    /// Returns the number of blocks rolled back if a fork was resolved
    pub fn detect_and_resolve_forks(&mut self, alternative_chain: Vec<Block>) -> Result<usize> {
        if alternative_chain.is_empty() {
            return Ok(0);
        }

        // Find the common ancestor
        let mut fork_height = 0;
        for block in &alternative_chain {
            let height = block.header.height;
            if self.storage.has_block(height) {
                let local_block = self.storage.load_block(height)?;
                let local_hash = local_block.calculate_hash()?;
                let alt_hash = block.calculate_hash()?;

                if local_hash != alt_hash {
                    // Fork detected at this height
                    fork_height = height;
                    break;
                }
            }
        }

        if fork_height == 0 {
            // No fork detected
            return Ok(0);
        }

        // Load local chain from fork point
        let local_height = self.storage.chain_height();
        let local_chain = self.storage.load_range(fork_height, local_height)?;

        // Get alternative chain from fork point
        let alt_chain: Vec<Block> = alternative_chain
            .into_iter()
            .filter(|b| b.header.height >= fork_height)
            .collect();

        // Use consensus manager to resolve the fork
        let winner = self.consensus.resolve_fork(&local_chain, &alt_chain);

        if winner == 1 {
            // Alternative chain wins - need to reorganize
            let rollback_count = (local_height - fork_height + 1) as usize;

            // In a full implementation, we would:
            // 1. Remove blocks from fork_height onwards
            // 2. Apply blocks from alternative chain
            // 3. Update indices

            // For now, just add alternative blocks to pending
            for block in alt_chain {
                self.add_pending_block(block);
            }

            Ok(rollback_count)
        } else {
            // Local chain wins - no action needed
            Ok(0)
        }
    }

    /// Get blocks that are missing in a range
    pub fn get_missing_heights(&self, start_height: u64, end_height: u64) -> Vec<u64> {
        let mut missing = Vec::new();

        for height in start_height..=end_height {
            if !self.storage.has_block(height) {
                missing.push(height);
            }
        }

        missing
    }

    /// Check if we need to sync with a peer
    ///
    /// Returns true if the peer has blocks we don't have
    pub fn needs_sync(&self, peer_height: u64) -> bool {
        peer_height > self.storage.chain_height()
    }

    /// Mark a node as queried
    pub fn mark_node_queried(&mut self, node_id: NodeId) {
        self.queried_nodes.insert(node_id);
    }

    /// Check if a node has been queried
    pub fn has_queried_node(&self, node_id: &NodeId) -> bool {
        self.queried_nodes.contains(node_id)
    }

    /// Reset sync state
    pub fn reset(&mut self) {
        self.state = SyncState::Idle;
        self.pending_blocks.clear();
        self.queried_nodes.clear();
    }

    /// Get storage reference (read-only)
    pub fn storage(&self) -> &LedgerStorage {
        &self.storage
    }

    /// Get consensus reference (read-only)
    pub fn consensus(&self) -> &ConsensusManager {
        &self.consensus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageConfig;
    use myriadmesh_crypto::signing::Signature;
    use myriadmesh_protocol::types::NODE_ID_SIZE;
    use tempfile::TempDir;

    fn create_test_node_id(val: u8) -> NodeId {
        NodeId::from_bytes([val; NODE_ID_SIZE])
    }

    fn create_test_sync() -> (ChainSync, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path()).without_pruning();
        let storage = LedgerStorage::new(config).unwrap();
        let consensus = ConsensusManager::with_defaults();
        let sync = ChainSync::new(storage, consensus);
        (sync, temp_dir)
    }

    #[test]
    fn test_sync_creation() {
        let (sync, _temp_dir) = create_test_sync();
        assert_eq!(sync.state(), SyncState::Idle);
        assert_eq!(sync.local_height(), 0);
    }

    #[test]
    fn test_add_pending_blocks() {
        let (mut sync, _temp_dir) = create_test_sync();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);
        let block = Block::genesis(creator, signature).unwrap();

        sync.add_pending_block(block);

        assert_eq!(sync.pending_blocks.len(), 1);
        assert_eq!(sync.stats.blocks_discovered, 1);
    }

    #[test]
    fn test_needs_sync() {
        let (sync, _temp_dir) = create_test_sync();

        assert!(sync.needs_sync(10));
        assert!(!sync.needs_sync(0));
    }

    #[test]
    fn test_get_missing_heights() {
        let (sync, _temp_dir) = create_test_sync();

        let missing = sync.get_missing_heights(0, 5);
        assert_eq!(missing, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_node_querying_tracking() {
        let (mut sync, _temp_dir) = create_test_sync();

        let node1 = create_test_node_id(1);
        let node2 = create_test_node_id(2);

        assert!(!sync.has_queried_node(&node1));

        sync.mark_node_queried(node1);

        assert!(sync.has_queried_node(&node1));
        assert!(!sync.has_queried_node(&node2));
    }

    #[test]
    fn test_reset() {
        let (mut sync, _temp_dir) = create_test_sync();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);
        let block = Block::genesis(creator, signature).unwrap();

        sync.add_pending_block(block);
        sync.mark_node_queried(creator);
        sync.state = SyncState::Downloading;

        sync.reset();

        assert_eq!(sync.state(), SyncState::Idle);
        assert_eq!(sync.pending_blocks.len(), 0);
        assert!(!sync.has_queried_node(&creator));
    }

    #[test]
    fn test_process_pending_blocks_empty() {
        let (mut sync, _temp_dir) = create_test_sync();

        let result = sync.process_pending_blocks();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_sync_state_transitions() {
        let (mut sync, _temp_dir) = create_test_sync();

        assert_eq!(sync.state(), SyncState::Idle);

        sync.state = SyncState::Discovering;
        assert_eq!(sync.state(), SyncState::Discovering);

        sync.state = SyncState::Downloading;
        assert_eq!(sync.state(), SyncState::Downloading);

        sync.state = SyncState::Synced;
        assert!(sync.is_synced());
    }
}
