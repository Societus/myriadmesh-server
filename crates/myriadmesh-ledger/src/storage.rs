//! Blockchain storage and indexing
//!
//! Provides persistent storage for blocks with:
//! - Individual block files (named by height)
//! - Block index for fast lookup
//! - Pruning strategy for resource-constrained devices

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::block::{Block, BlockHash};
use crate::error::{LedgerError, Result};

/// Default number of blocks to keep when pruning
pub const DEFAULT_KEEP_BLOCKS: u64 = 10_000;

/// Configuration for ledger storage
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Directory for block storage
    pub storage_dir: PathBuf,
    /// Whether pruning is enabled
    pub pruning_enabled: bool,
    /// Number of most recent blocks to keep
    pub keep_blocks: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from("/var/lib/myriadnode/ledger"),
            pruning_enabled: true,
            keep_blocks: DEFAULT_KEEP_BLOCKS,
        }
    }
}

impl StorageConfig {
    /// Create a new storage configuration
    pub fn new<P: Into<PathBuf>>(storage_dir: P) -> Self {
        Self {
            storage_dir: storage_dir.into(),
            pruning_enabled: true,
            keep_blocks: DEFAULT_KEEP_BLOCKS,
        }
    }

    /// Disable pruning (keep all blocks)
    pub fn without_pruning(mut self) -> Self {
        self.pruning_enabled = false;
        self
    }

    /// Set the number of blocks to keep
    pub fn with_keep_blocks(mut self, keep_blocks: u64) -> Self {
        self.keep_blocks = keep_blocks;
        self
    }
}

/// Block metadata for the index
#[derive(Debug, Clone)]
pub struct BlockMetadata {
    /// Block height
    pub height: u64,
    /// Block hash
    pub hash: BlockHash,
    /// File path where block is stored
    pub file_path: PathBuf,
    /// Block size in bytes
    pub size: u64,
}

/// Blockchain storage manager
pub struct LedgerStorage {
    /// Storage configuration
    config: StorageConfig,
    /// Block index: height -> metadata
    index: HashMap<u64, BlockMetadata>,
    /// Hash lookup: hash -> height
    hash_lookup: HashMap<BlockHash, u64>,
    /// Highest block height
    chain_height: u64,
}

impl LedgerStorage {
    /// Create a new ledger storage with the given configuration
    pub fn new(config: StorageConfig) -> Result<Self> {
        // Create storage directories
        fs::create_dir_all(&config.storage_dir)?;

        let blocks_dir = config.storage_dir.join("blocks");
        fs::create_dir_all(&blocks_dir)?;

        Ok(Self {
            config,
            index: HashMap::new(),
            hash_lookup: HashMap::new(),
            chain_height: 0,
        })
    }

    /// Create storage with default configuration
    pub fn with_defaults() -> Result<Self> {
        Self::new(StorageConfig::default())
    }

    /// Store a block
    pub fn store_block(&mut self, block: &Block) -> Result<()> {
        let height = block.header.height;
        let hash = block.calculate_hash()?;

        // Serialize block
        let block_bytes = block.to_bytes()?;

        // Determine file path
        let blocks_dir = self.config.storage_dir.join("blocks");
        let file_path = blocks_dir.join(format!("block_{:010}.bin", height));

        // Write block to file
        fs::write(&file_path, &block_bytes)?;

        // Update index
        let metadata = BlockMetadata {
            height,
            hash,
            file_path: file_path.clone(),
            size: block_bytes.len() as u64,
        };

        self.index.insert(height, metadata);
        self.hash_lookup.insert(hash, height);

        // Update chain height
        if height > self.chain_height {
            self.chain_height = height;
        }

        // Prune if necessary
        if self.config.pruning_enabled && self.index.len() as u64 > self.config.keep_blocks {
            self.prune()?;
        }

        Ok(())
    }

    /// Load a block by height
    pub fn load_block(&self, height: u64) -> Result<Block> {
        let metadata = self
            .index
            .get(&height)
            .ok_or(LedgerError::BlockNotFound(height))?;

        let block_bytes = fs::read(&metadata.file_path)?;
        Block::from_bytes(&block_bytes)
    }

    /// Load a block by hash
    pub fn load_block_by_hash(&self, hash: &BlockHash) -> Result<Block> {
        let height = self
            .hash_lookup
            .get(hash)
            .ok_or_else(|| LedgerError::StorageError("block not found by hash".to_string()))?;

        self.load_block(*height)
    }

    /// Get the latest block
    pub fn get_latest_block(&self) -> Result<Block> {
        if self.chain_height == 0 && self.index.is_empty() {
            return Err(LedgerError::StorageError(
                "no blocks in storage".to_string(),
            ));
        }

        self.load_block(self.chain_height)
    }

    /// Get the current chain height
    pub fn chain_height(&self) -> u64 {
        self.chain_height
    }

    /// Check if a block exists at the given height
    pub fn has_block(&self, height: u64) -> bool {
        self.index.contains_key(&height)
    }

    /// Check if a block with the given hash exists
    pub fn has_block_with_hash(&self, hash: &BlockHash) -> bool {
        self.hash_lookup.contains_key(hash)
    }

    /// Get block metadata by height
    pub fn get_metadata(&self, height: u64) -> Option<&BlockMetadata> {
        self.index.get(&height)
    }

    /// Prune old blocks, keeping only the most recent ones
    fn prune(&mut self) -> Result<()> {
        if self.index.len() as u64 <= self.config.keep_blocks {
            return Ok(());
        }

        // Determine which blocks to delete
        let keep_from_height = self
            .chain_height
            .saturating_sub(self.config.keep_blocks - 1);

        let heights_to_delete: Vec<u64> = self
            .index
            .keys()
            .filter(|&&height| height < keep_from_height)
            .copied()
            .collect();

        // Delete blocks
        for height in heights_to_delete {
            if let Some(metadata) = self.index.remove(&height) {
                // Remove file
                let _ = fs::remove_file(&metadata.file_path);

                // Remove from hash lookup
                self.hash_lookup.remove(&metadata.hash);
            }
        }

        Ok(())
    }

    /// Get the total number of blocks in storage
    pub fn block_count(&self) -> usize {
        self.index.len()
    }

    /// Load a range of blocks
    pub fn load_range(&self, start_height: u64, end_height: u64) -> Result<Vec<Block>> {
        let mut blocks = Vec::new();

        for height in start_height..=end_height {
            if let Ok(block) = self.load_block(height) {
                blocks.push(block);
            }
        }

        Ok(blocks)
    }

    /// Get all block heights in storage
    pub fn get_heights(&self) -> Vec<u64> {
        let mut heights: Vec<u64> = self.index.keys().copied().collect();
        heights.sort_unstable();
        heights
    }

    /// Rebuild the index from disk
    ///
    /// Useful for recovery or initialization from existing blocks
    pub fn rebuild_index(&mut self) -> Result<()> {
        self.index.clear();
        self.hash_lookup.clear();
        self.chain_height = 0;

        let blocks_dir = self.config.storage_dir.join("blocks");
        if !blocks_dir.exists() {
            return Ok(());
        }

        // Read all block files
        for entry in fs::read_dir(&blocks_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some_and(|ext| ext == "bin") {
                // Load block
                let block_bytes = fs::read(&path)?;
                if let Ok(block) = Block::from_bytes(&block_bytes) {
                    let height = block.header.height;
                    let hash = block.calculate_hash()?;
                    let size = block_bytes.len() as u64;

                    let metadata = BlockMetadata {
                        height,
                        hash,
                        file_path: path.clone(),
                        size,
                    };

                    self.index.insert(height, metadata);
                    self.hash_lookup.insert(hash, height);

                    if height > self.chain_height {
                        self.chain_height = height;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get storage statistics
    pub fn stats(&self) -> StorageStats {
        let total_size: u64 = self.index.values().map(|m| m.size).sum();

        StorageStats {
            block_count: self.index.len(),
            chain_height: self.chain_height,
            total_size_bytes: total_size,
            pruning_enabled: self.config.pruning_enabled,
            keep_blocks: self.config.keep_blocks,
        }
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    /// Number of blocks in storage
    pub block_count: usize,
    /// Current chain height
    pub chain_height: u64,
    /// Total storage size in bytes
    pub total_size_bytes: u64,
    /// Whether pruning is enabled
    pub pruning_enabled: bool,
    /// Number of blocks to keep when pruning
    pub keep_blocks: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use myriadmesh_crypto::signing::Signature;
    use myriadmesh_protocol::{types::NODE_ID_SIZE, NodeId};
    use tempfile::TempDir;

    fn create_test_node_id(val: u8) -> NodeId {
        NodeId::from_bytes([val; NODE_ID_SIZE])
    }

    fn create_test_storage() -> (LedgerStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path()).without_pruning();
        let storage = LedgerStorage::new(config).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path());
        let storage = LedgerStorage::new(config);

        assert!(storage.is_ok());
    }

    #[test]
    fn test_store_and_load_block() {
        let (mut storage, _temp_dir) = create_test_storage();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);
        let block = Block::genesis(creator, signature).unwrap();

        // Store block
        storage.store_block(&block).unwrap();

        // Load block
        let loaded = storage.load_block(0).unwrap();
        assert_eq!(loaded.header.height, block.header.height);
        assert_eq!(loaded.header.creator, block.header.creator);
    }

    #[test]
    fn test_load_nonexistent_block() {
        let (storage, _temp_dir) = create_test_storage();

        let result = storage.load_block(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_chain_height_tracking() {
        let (mut storage, _temp_dir) = create_test_storage();

        assert_eq!(storage.chain_height(), 0);

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);

        // Store genesis
        let genesis = Block::genesis(creator, signature).unwrap();
        storage.store_block(&genesis).unwrap();
        assert_eq!(storage.chain_height(), 0);

        // Store block 1
        let hash = genesis.calculate_hash().unwrap();
        let block1 = Block::new(1, hash, creator, signature, vec![]).unwrap();
        storage.store_block(&block1).unwrap();
        assert_eq!(storage.chain_height(), 1);
    }

    #[test]
    fn test_has_block() {
        let (mut storage, _temp_dir) = create_test_storage();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);
        let block = Block::genesis(creator, signature).unwrap();

        assert!(!storage.has_block(0));

        storage.store_block(&block).unwrap();

        assert!(storage.has_block(0));
        assert!(!storage.has_block(1));
    }

    #[test]
    fn test_load_by_hash() {
        let (mut storage, _temp_dir) = create_test_storage();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);
        let block = Block::genesis(creator, signature).unwrap();
        let hash = block.calculate_hash().unwrap();

        storage.store_block(&block).unwrap();

        let loaded = storage.load_block_by_hash(&hash).unwrap();
        assert_eq!(loaded.header.height, block.header.height);
    }

    #[test]
    fn test_get_latest_block() {
        let (mut storage, _temp_dir) = create_test_storage();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);

        // No blocks yet
        assert!(storage.get_latest_block().is_err());

        // Store genesis
        let genesis = Block::genesis(creator, signature).unwrap();
        storage.store_block(&genesis).unwrap();

        let latest = storage.get_latest_block().unwrap();
        assert_eq!(latest.header.height, 0);

        // Store block 1
        let hash = genesis.calculate_hash().unwrap();
        let block1 = Block::new(1, hash, creator, signature, vec![]).unwrap();
        storage.store_block(&block1).unwrap();

        let latest = storage.get_latest_block().unwrap();
        assert_eq!(latest.header.height, 1);
    }

    #[test]
    fn test_pruning() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig::new(temp_dir.path()).with_keep_blocks(3);
        let mut storage = LedgerStorage::new(config).unwrap();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);

        // Store 5 blocks
        let mut prev_hash = [0u8; 32];
        for i in 0..5 {
            let block = Block::new(i, prev_hash, creator, signature, vec![]).unwrap();
            prev_hash = block.calculate_hash().unwrap();
            storage.store_block(&block).unwrap();
        }

        // Should have pruned to keep only 3 most recent
        assert_eq!(storage.block_count(), 3);
        assert!(!storage.has_block(0));
        assert!(!storage.has_block(1));
        assert!(storage.has_block(2));
        assert!(storage.has_block(3));
        assert!(storage.has_block(4));
    }

    #[test]
    fn test_load_range() {
        let (mut storage, _temp_dir) = create_test_storage();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);

        // Store blocks 0-4
        let mut prev_hash = [0u8; 32];
        for i in 0..5 {
            let block = Block::new(i, prev_hash, creator, signature, vec![]).unwrap();
            prev_hash = block.calculate_hash().unwrap();
            storage.store_block(&block).unwrap();
        }

        // Load range
        let blocks = storage.load_range(1, 3).unwrap();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].header.height, 1);
        assert_eq!(blocks[1].header.height, 2);
        assert_eq!(blocks[2].header.height, 3);
    }

    #[test]
    fn test_stats() {
        let (mut storage, _temp_dir) = create_test_storage();

        let creator = create_test_node_id(1);
        let signature = Signature::from_bytes([0u8; 64]);
        let block = Block::genesis(creator, signature).unwrap();

        storage.store_block(&block).unwrap();

        let stats = storage.stats();
        assert_eq!(stats.block_count, 1);
        assert_eq!(stats.chain_height, 0);
        assert!(stats.total_size_bytes > 0);
    }
}
