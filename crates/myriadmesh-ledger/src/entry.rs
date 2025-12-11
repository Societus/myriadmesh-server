//! Ledger entry types
//!
//! Defines the four primary entry types:
//! - DISCOVERY: Node joins network
//! - TEST: Network performance testing results
//! - MESSAGE: Message delivery confirmations
//! - KEY_EXCHANGE: Cryptographic key exchanges

use chrono::{DateTime, Utc};
use myriadmesh_crypto::signing::Signature;
use myriadmesh_protocol::{types::AdapterType, NodeId};
use serde::{Deserialize, Serialize};

/// A ledger entry that records a network event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LedgerEntry {
    /// Entry type and data
    pub entry_type: EntryType,
    /// Timestamp when the entry was created
    pub timestamp: DateTime<Utc>,
    /// Signature of the reporting node
    pub signature: Signature,
}

impl LedgerEntry {
    /// Create a new ledger entry
    pub fn new(entry_type: EntryType, signature: Signature) -> Self {
        Self {
            entry_type,
            timestamp: Utc::now(),
            signature,
        }
    }

    /// Get the byte representation for hashing
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Get the node ID associated with this entry
    pub fn node_id(&self) -> NodeId {
        match &self.entry_type {
            EntryType::Discovery(d) => d.node_id,
            EntryType::Test(t) => t.source_node,
            EntryType::Message(m) => m.source_node,
            EntryType::KeyExchange(k) => k.node1,
        }
    }
}

/// Different types of ledger entries
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntryType {
    /// Node discovery entry
    Discovery(DiscoveryEntry),
    /// Network test result entry
    Test(TestEntry),
    /// Message delivery confirmation entry
    Message(MessageEntry),
    /// Key exchange entry
    KeyExchange(KeyExchangeEntry),
}

/// Discovery entry - records when a node joins the network
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryEntry {
    /// Node identifier
    pub node_id: NodeId,
    /// Ed25519 public key (32 bytes)
    pub public_key: [u8; 32],
    /// Available network adapters
    pub adapters: Vec<AdapterType>,
    /// Node ID of the discovering node
    pub discovered_by: NodeId,
}

impl DiscoveryEntry {
    /// Create a new discovery entry
    pub fn new(
        node_id: NodeId,
        public_key: [u8; 32],
        adapters: Vec<AdapterType>,
        discovered_by: NodeId,
    ) -> Self {
        Self {
            node_id,
            public_key,
            adapters,
            discovered_by,
        }
    }
}

/// Test entry - records network performance test results
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestEntry {
    /// Source node performing the test
    pub source_node: NodeId,
    /// Destination node being tested
    pub dest_node: NodeId,
    /// Adapter type used for the test
    pub adapter: AdapterType,
    /// Latency in milliseconds
    pub latency_ms: f64,
    /// Bandwidth in bytes per second
    pub bandwidth_bps: u64,
    /// Whether the test succeeded
    pub success: bool,
}

impl TestEntry {
    /// Create a new test entry
    pub fn new(
        source_node: NodeId,
        dest_node: NodeId,
        adapter: AdapterType,
        latency_ms: f64,
        bandwidth_bps: u64,
        success: bool,
    ) -> Self {
        Self {
            source_node,
            dest_node,
            adapter,
            latency_ms,
            bandwidth_bps,
            success,
        }
    }
}

/// Message entry - records message delivery confirmation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageEntry {
    /// Message identifier (16 bytes)
    pub message_id: [u8; 16],
    /// Source node that sent the message
    pub source_node: NodeId,
    /// Destination node that received the message
    pub dest_node: NodeId,
    /// Adapter used for delivery
    pub adapter: AdapterType,
    /// Whether delivery was successful
    pub delivered: bool,
}

impl MessageEntry {
    /// Create a new message entry
    pub fn new(
        message_id: [u8; 16],
        source_node: NodeId,
        dest_node: NodeId,
        adapter: AdapterType,
        delivered: bool,
    ) -> Self {
        Self {
            message_id,
            source_node,
            dest_node,
            adapter,
            delivered,
        }
    }
}

/// Key exchange entry - records cryptographic key exchanges
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyExchangeEntry {
    /// First node in the exchange
    pub node1: NodeId,
    /// Second node in the exchange
    pub node2: NodeId,
    /// BLAKE2b hash of the exchanged key (32 bytes)
    pub key_hash: [u8; 32],
}

impl KeyExchangeEntry {
    /// Create a new key exchange entry
    pub fn new(node1: NodeId, node2: NodeId, key_hash: [u8; 32]) -> Self {
        Self {
            node1,
            node2,
            key_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use myriadmesh_protocol::types::NODE_ID_SIZE;

    #[test]
    fn test_discovery_entry() {
        let node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let discovered_by = NodeId::from_bytes([2u8; NODE_ID_SIZE]);
        let public_key = [3u8; 32];
        let adapters = vec![AdapterType::Ethernet, AdapterType::Bluetooth];

        let entry = DiscoveryEntry::new(node_id, public_key, adapters.clone(), discovered_by);

        assert_eq!(entry.node_id, node_id);
        assert_eq!(entry.public_key, public_key);
        assert_eq!(entry.adapters, adapters);
        assert_eq!(entry.discovered_by, discovered_by);
    }

    #[test]
    fn test_test_entry() {
        let source = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let dest = NodeId::from_bytes([2u8; NODE_ID_SIZE]);

        let entry = TestEntry::new(source, dest, AdapterType::Ethernet, 10.5, 1_000_000, true);

        assert_eq!(entry.source_node, source);
        assert_eq!(entry.dest_node, dest);
        assert_eq!(entry.adapter, AdapterType::Ethernet);
        assert_eq!(entry.latency_ms, 10.5);
        assert_eq!(entry.bandwidth_bps, 1_000_000);
        assert!(entry.success);
    }

    #[test]
    fn test_message_entry() {
        let source = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let dest = NodeId::from_bytes([2u8; NODE_ID_SIZE]);
        let message_id = [42u8; 16];

        let entry = MessageEntry::new(message_id, source, dest, AdapterType::Bluetooth, true);

        assert_eq!(entry.message_id, message_id);
        assert_eq!(entry.source_node, source);
        assert_eq!(entry.dest_node, dest);
        assert_eq!(entry.adapter, AdapterType::Bluetooth);
        assert!(entry.delivered);
    }

    #[test]
    fn test_key_exchange_entry() {
        let node1 = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let node2 = NodeId::from_bytes([2u8; NODE_ID_SIZE]);
        let key_hash = [99u8; 32];

        let entry = KeyExchangeEntry::new(node1, node2, key_hash);

        assert_eq!(entry.node1, node1);
        assert_eq!(entry.node2, node2);
        assert_eq!(entry.key_hash, key_hash);
    }

    #[test]
    fn test_ledger_entry_serialization() {
        let node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let discovered_by = NodeId::from_bytes([2u8; NODE_ID_SIZE]);
        let public_key = [3u8; 32];
        let adapters = vec![AdapterType::Ethernet];

        let discovery = DiscoveryEntry::new(node_id, public_key, adapters, discovered_by);
        let signature = Signature::from_bytes([0u8; 64]);
        let entry = LedgerEntry::new(EntryType::Discovery(discovery), signature);

        let bytes = entry.to_bytes().unwrap();
        let deserialized: LedgerEntry = bincode::deserialize(&bytes).unwrap();

        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_entry_node_id() {
        let node_id = NodeId::from_bytes([1u8; NODE_ID_SIZE]);
        let discovered_by = NodeId::from_bytes([2u8; NODE_ID_SIZE]);

        let discovery = DiscoveryEntry::new(node_id, [0u8; 32], vec![], discovered_by);
        let signature = Signature::from_bytes([0u8; 64]);
        let entry = LedgerEntry::new(EntryType::Discovery(discovery), signature);

        assert_eq!(entry.node_id(), node_id);
    }
}
