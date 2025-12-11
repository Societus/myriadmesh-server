//! Proof of Participation (PoP) Consensus Mechanism
//!
//! Implements a lightweight reputation-based consensus where:
//! - Nodes earn reputation by successfully relaying messages
//! - Block creation rotates among high-reputation nodes
//! - Blocks require signatures from 2/3 of known nodes
//! - Fork resolution: longest chain with highest total reputation wins

use myriadmesh_protocol::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::block::Block;
use crate::error::{LedgerError, Result};

/// Default minimum reputation required to create blocks
pub const DEFAULT_MIN_REPUTATION: f64 = 0.5;

/// Reputation weights
const SUCCESSFUL_RELAYS_WEIGHT: f64 = 0.50; // 50%
const UPTIME_WEIGHT: f64 = 0.30; // 30%
const LEDGER_PARTICIPATION_WEIGHT: f64 = 0.20; // 20%

/// Node reputation for consensus participation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeReputation {
    /// Node identifier
    pub node_id: NodeId,
    /// Number of successful message relays
    pub successful_relays: u64,
    /// Total relay attempts
    pub total_relay_attempts: u64,
    /// Uptime percentage (0.0 - 1.0)
    pub uptime: f64,
    /// Number of blocks created
    pub blocks_created: u64,
    /// Number of blocks validated
    pub blocks_validated: u64,
    /// Last seen timestamp (Unix timestamp)
    pub last_seen: u64,
}

impl NodeReputation {
    /// Create a new reputation entry for a node
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            successful_relays: 0,
            total_relay_attempts: 0,
            uptime: 0.0,
            blocks_created: 0,
            blocks_validated: 0,
            last_seen: 0,
        }
    }

    /// Calculate the overall reputation score (0.0 - 1.0)
    pub fn score(&self) -> f64 {
        // Relay success rate (0.0 - 1.0)
        let relay_score = if self.total_relay_attempts > 0 {
            self.successful_relays as f64 / self.total_relay_attempts as f64
        } else {
            0.0
        };

        // Uptime score (0.0 - 1.0)
        let uptime_score = self.uptime;

        // Ledger participation score (0.0 - 1.0)
        // Based on ratio of validations to creations (balanced participation is good)
        let participation_score = if self.blocks_created > 0 || self.blocks_validated > 0 {
            let total_participation = self.blocks_created + self.blocks_validated;
            // Normalize to 0.0-1.0 range, cap at 100 participations
            (total_participation as f64 / 100.0).min(1.0)
        } else {
            0.0
        };

        // Weighted average
        (relay_score * SUCCESSFUL_RELAYS_WEIGHT)
            + (uptime_score * UPTIME_WEIGHT)
            + (participation_score * LEDGER_PARTICIPATION_WEIGHT)
    }

    /// Record a successful relay
    pub fn record_relay_success(&mut self) {
        self.successful_relays += 1;
        self.total_relay_attempts += 1;
    }

    /// Record a failed relay
    pub fn record_relay_failure(&mut self) {
        self.total_relay_attempts += 1;
    }

    /// Record block creation
    pub fn record_block_creation(&mut self) {
        self.blocks_created += 1;
    }

    /// Record block validation
    pub fn record_block_validation(&mut self) {
        self.blocks_validated += 1;
    }

    /// Update uptime percentage
    pub fn update_uptime(&mut self, uptime: f64) {
        self.uptime = uptime.clamp(0.0, 1.0);
    }

    /// Update last seen timestamp
    pub fn update_last_seen(&mut self, timestamp: u64) {
        self.last_seen = timestamp;
    }
}

/// Consensus manager for Proof of Participation
pub struct ConsensusManager {
    /// Reputation tracking for all nodes
    reputations: HashMap<NodeId, NodeReputation>,
    /// Minimum reputation required to create blocks
    min_reputation: f64,
}

impl ConsensusManager {
    /// Create a new consensus manager
    pub fn new(min_reputation: f64) -> Self {
        Self {
            reputations: HashMap::new(),
            min_reputation,
        }
    }

    /// Create with default minimum reputation
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MIN_REPUTATION)
    }

    /// Get or create reputation entry for a node
    pub fn get_reputation_mut(&mut self, node_id: &NodeId) -> &mut NodeReputation {
        self.reputations
            .entry(*node_id)
            .or_insert_with(|| NodeReputation::new(*node_id))
    }

    /// Get reputation for a node (read-only)
    pub fn get_reputation(&self, node_id: &NodeId) -> Option<&NodeReputation> {
        self.reputations.get(node_id)
    }

    /// Check if a node can create blocks
    pub fn can_create_blocks(&self, node_id: &NodeId) -> bool {
        if let Some(reputation) = self.reputations.get(node_id) {
            reputation.score() >= self.min_reputation
        } else {
            false
        }
    }

    /// Validate that a node has sufficient reputation to create a block
    pub fn validate_block_creator(&self, node_id: &NodeId) -> Result<()> {
        if !self.can_create_blocks(node_id) {
            let score = self
                .reputations
                .get(node_id)
                .map(|r| r.score())
                .unwrap_or(0.0);

            return Err(LedgerError::InsufficientReputation {
                node_id: node_id.to_hex(),
                reputation: score,
                minimum: self.min_reputation,
            });
        }
        Ok(())
    }

    /// Get eligible block creators (nodes with sufficient reputation)
    pub fn get_eligible_creators(&self) -> Vec<NodeId> {
        self.reputations
            .iter()
            .filter(|(_, rep)| rep.score() >= self.min_reputation)
            .map(|(node_id, _)| *node_id)
            .collect()
    }

    /// Select next block creator using round-robin among eligible nodes
    ///
    /// Uses block height as the selection index to ensure deterministic selection
    pub fn select_next_creator(&self, block_height: u64) -> Option<NodeId> {
        let mut eligible = self.get_eligible_creators();
        if eligible.is_empty() {
            return None;
        }

        // Sort for deterministic ordering
        eligible.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));

        let index = (block_height as usize) % eligible.len();
        Some(eligible[index])
    }

    /// Calculate total reputation of a chain
    ///
    /// This is used for fork resolution: longest chain with highest reputation wins
    pub fn calculate_chain_reputation(&self, blocks: &[Block]) -> f64 {
        blocks
            .iter()
            .map(|block| {
                self.reputations
                    .get(&block.header.creator)
                    .map(|r| r.score())
                    .unwrap_or(0.0)
            })
            .sum()
    }

    /// Resolve a fork by selecting the best chain
    ///
    /// Returns the index of the winning chain (0 or 1)
    /// Preference: longer chain, then higher total reputation
    pub fn resolve_fork(&self, chain1: &[Block], chain2: &[Block]) -> usize {
        // Longer chain wins
        if chain1.len() > chain2.len() {
            return 0;
        }
        if chain2.len() > chain1.len() {
            return 1;
        }

        // Same length - compare total reputation
        let rep1 = self.calculate_chain_reputation(chain1);
        let rep2 = self.calculate_chain_reputation(chain2);

        if rep1 >= rep2 {
            0
        } else {
            1
        }
    }

    /// Validate block consensus (2/3 majority of known nodes)
    pub fn validate_consensus(&self, block: &Block) -> Result<()> {
        let total_nodes = self.reputations.len();
        if total_nodes == 0 {
            return Ok(()); // No nodes known yet (genesis case)
        }

        if !block.has_consensus(total_nodes) {
            let required = (total_nodes * 2).div_ceil(3);
            return Err(LedgerError::InsufficientSignatures {
                needed: required,
                actual: block.signature_count(),
            });
        }

        Ok(())
    }

    /// Get the minimum reputation threshold
    pub fn min_reputation(&self) -> f64 {
        self.min_reputation
    }

    /// Set the minimum reputation threshold
    pub fn set_min_reputation(&mut self, min_reputation: f64) {
        self.min_reputation = min_reputation.clamp(0.0, 1.0);
    }

    /// Get total number of tracked nodes
    pub fn node_count(&self) -> usize {
        self.reputations.len()
    }

    /// Get all reputations
    pub fn all_reputations(&self) -> &HashMap<NodeId, NodeReputation> {
        &self.reputations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use myriadmesh_protocol::types::NODE_ID_SIZE;

    fn create_test_node_id(val: u8) -> NodeId {
        NodeId::from_bytes([val; NODE_ID_SIZE])
    }

    #[test]
    fn test_new_reputation() {
        let node_id = create_test_node_id(1);
        let rep = NodeReputation::new(node_id);

        assert_eq!(rep.score(), 0.0);
        assert_eq!(rep.successful_relays, 0);
        assert_eq!(rep.total_relay_attempts, 0);
    }

    #[test]
    fn test_reputation_relay_tracking() {
        let node_id = create_test_node_id(1);
        let mut rep = NodeReputation::new(node_id);

        rep.record_relay_success();
        rep.record_relay_success();
        rep.record_relay_failure();

        assert_eq!(rep.successful_relays, 2);
        assert_eq!(rep.total_relay_attempts, 3);

        // 2/3 success rate * 50% weight = 0.333
        let expected = (2.0 / 3.0) * SUCCESSFUL_RELAYS_WEIGHT;
        assert!((rep.score() - expected).abs() < 0.001);
    }

    #[test]
    fn test_reputation_uptime() {
        let node_id = create_test_node_id(1);
        let mut rep = NodeReputation::new(node_id);

        rep.update_uptime(0.9);

        // 90% uptime * 30% weight = 0.27
        let expected = 0.9 * UPTIME_WEIGHT;
        assert!((rep.score() - expected).abs() < 0.001);
    }

    #[test]
    fn test_reputation_participation() {
        let node_id = create_test_node_id(1);
        let mut rep = NodeReputation::new(node_id);

        rep.record_block_creation();
        rep.record_block_validation();
        rep.record_block_validation();

        // 3 participations / 100 max * 20% weight = 0.006
        let expected = (3.0 / 100.0) * LEDGER_PARTICIPATION_WEIGHT;
        assert!((rep.score() - expected).abs() < 0.001);
    }

    #[test]
    fn test_full_reputation_score() {
        let node_id = create_test_node_id(1);
        let mut rep = NodeReputation::new(node_id);

        // Perfect relay record
        for _ in 0..10 {
            rep.record_relay_success();
        }

        // 100% uptime
        rep.update_uptime(1.0);

        // Max participation
        for _ in 0..100 {
            rep.record_block_creation();
        }

        // Should be close to 1.0 (100% * 0.5 + 100% * 0.3 + 100% * 0.2 = 1.0)
        assert!((rep.score() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_consensus_manager_creation() {
        let manager = ConsensusManager::with_defaults();
        assert_eq!(manager.min_reputation(), DEFAULT_MIN_REPUTATION);
        assert_eq!(manager.node_count(), 0);
    }

    #[test]
    fn test_can_create_blocks() {
        let mut manager = ConsensusManager::new(0.5);
        let node_id = create_test_node_id(1);

        // Initially can't create blocks
        assert!(!manager.can_create_blocks(&node_id));

        // Build up reputation
        let rep = manager.get_reputation_mut(&node_id);
        for _ in 0..10 {
            rep.record_relay_success();
        }
        rep.update_uptime(1.0);

        // Now should be able to create blocks
        assert!(manager.can_create_blocks(&node_id));
    }

    #[test]
    fn test_eligible_creators() {
        let mut manager = ConsensusManager::new(0.5);

        let node1 = create_test_node_id(1);
        let node2 = create_test_node_id(2);
        let node3 = create_test_node_id(3);

        // Build reputation for node1 and node2
        for node_id in [node1, node2] {
            let rep = manager.get_reputation_mut(&node_id);
            for _ in 0..10 {
                rep.record_relay_success();
            }
            rep.update_uptime(1.0);
        }

        // Node3 has low reputation
        manager.get_reputation_mut(&node3);

        let eligible = manager.get_eligible_creators();
        assert_eq!(eligible.len(), 2);
        assert!(eligible.contains(&node1));
        assert!(eligible.contains(&node2));
        assert!(!eligible.contains(&node3));
    }

    #[test]
    fn test_select_next_creator() {
        let mut manager = ConsensusManager::new(0.3);

        // Add 3 eligible nodes
        for i in 1..=3 {
            let node_id = create_test_node_id(i);
            let rep = manager.get_reputation_mut(&node_id);
            for _ in 0..10 {
                rep.record_relay_success();
            }
            rep.update_uptime(0.9);
        }

        // Should rotate through nodes based on height
        let creator0 = manager.select_next_creator(0);
        let creator1 = manager.select_next_creator(1);
        let creator2 = manager.select_next_creator(2);
        let creator3 = manager.select_next_creator(3);

        assert!(creator0.is_some());
        assert!(creator1.is_some());
        assert!(creator2.is_some());
        assert!(creator3.is_some());

        // Should wrap around after 3
        assert_eq!(creator0.unwrap(), creator3.unwrap());
    }

    #[test]
    fn test_chain_reputation_calculation() {
        use crate::block::Block;
        use myriadmesh_crypto::signing::Signature;

        let mut manager = ConsensusManager::new(0.5);

        let node1 = create_test_node_id(1);
        let node2 = create_test_node_id(2);

        // Set reputation scores
        let rep1 = manager.get_reputation_mut(&node1);
        rep1.update_uptime(0.8);
        for _ in 0..10 {
            rep1.record_relay_success();
        }

        let rep2 = manager.get_reputation_mut(&node2);
        rep2.update_uptime(0.6);
        for _ in 0..10 {
            rep2.record_relay_success();
        }

        let sig = Signature::from_bytes([0u8; 64]);

        // Create blocks
        let block1 = Block::genesis(node1, sig).unwrap();
        let block2 = Block::genesis(node2, sig).unwrap();

        let chain = vec![block1, block2];
        let total_rep = manager.calculate_chain_reputation(&chain);

        // Should be sum of both reputations
        let expected = manager.get_reputation(&node1).unwrap().score()
            + manager.get_reputation(&node2).unwrap().score();

        assert!((total_rep - expected).abs() < 0.001);
    }

    #[test]
    fn test_fork_resolution_by_length() {
        use crate::block::Block;
        use myriadmesh_crypto::signing::Signature;

        let manager = ConsensusManager::with_defaults();
        let node_id = create_test_node_id(1);
        let sig = Signature::from_bytes([0u8; 64]);

        let chain1 = vec![Block::genesis(node_id, sig).unwrap()];
        let chain2 = vec![
            Block::genesis(node_id, sig).unwrap(),
            Block::genesis(node_id, sig).unwrap(),
        ];

        // Longer chain should win
        assert_eq!(manager.resolve_fork(&chain1, &chain2), 1);
        assert_eq!(manager.resolve_fork(&chain2, &chain1), 0);
    }
}
