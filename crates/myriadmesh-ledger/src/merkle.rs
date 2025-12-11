//! Merkle tree implementation for ledger entries
//!
//! Provides efficient verification that entries haven't been tampered with.

use blake2::{Blake2b512, Digest};

/// Hash size for BLAKE2b-512 (64 bytes)
pub const HASH_SIZE: usize = 64;

/// A Merkle tree root hash
pub type MerkleRoot = [u8; 32]; // We use 32 bytes for the root

/// Calculate the Merkle root of a list of entries
///
/// Uses BLAKE2b-512 for hashing, then truncates to 32 bytes for the root.
/// If the list is empty, returns a hash of an empty byte array.
pub fn calculate_merkle_root(entries: &[Vec<u8>]) -> MerkleRoot {
    if entries.is_empty() {
        // Empty tree - hash of empty bytes
        let mut hasher = Blake2b512::new();
        hasher.update(b"");
        let hash = hasher.finalize();
        let mut root = [0u8; 32];
        root.copy_from_slice(&hash[..32]);
        return root;
    }

    // Hash all entries
    let mut hashes: Vec<[u8; HASH_SIZE]> = entries
        .iter()
        .map(|entry| {
            let mut hasher = Blake2b512::new();
            hasher.update(entry);
            let hash = hasher.finalize();
            let mut result = [0u8; HASH_SIZE];
            result.copy_from_slice(&hash);
            result
        })
        .collect();

    // Build the tree bottom-up
    while hashes.len() > 1 {
        let mut next_level = Vec::new();

        // Process pairs of hashes
        for chunk in hashes.chunks(2) {
            let combined_hash = if chunk.len() == 2 {
                // Combine two hashes
                let mut hasher = Blake2b512::new();
                hasher.update(chunk[0]);
                hasher.update(chunk[1]);
                hasher.finalize()
            } else {
                // Odd number - hash with itself
                let mut hasher = Blake2b512::new();
                hasher.update(chunk[0]);
                hasher.update(chunk[0]);
                hasher.finalize()
            };

            let mut hash = [0u8; HASH_SIZE];
            hash.copy_from_slice(&combined_hash);
            next_level.push(hash);
        }

        hashes = next_level;
    }

    // Truncate final hash to 32 bytes for the root
    let mut root = [0u8; 32];
    root.copy_from_slice(&hashes[0][..32]);
    root
}

/// Verify that an entry is included in a Merkle tree
///
/// This is a simple implementation that recalculates the entire root.
/// For production, you'd want to implement Merkle proofs for efficiency.
pub fn verify_entry_inclusion(
    entries: &[Vec<u8>],
    entry: &[u8],
    expected_root: &MerkleRoot,
) -> bool {
    // Check if entry exists in the list
    if !entries.iter().any(|e| e == entry) {
        return false;
    }

    // Verify the root matches
    let calculated_root = calculate_merkle_root(entries);
    calculated_root == *expected_root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_merkle_tree() {
        let root = calculate_merkle_root(&[]);
        assert_eq!(root.len(), 32);
        // Should be deterministic
        let root2 = calculate_merkle_root(&[]);
        assert_eq!(root, root2);
    }

    #[test]
    fn test_single_entry() {
        let entries = vec![b"entry1".to_vec()];
        let root = calculate_merkle_root(&entries);
        assert_eq!(root.len(), 32);

        // Different entry should produce different root
        let entries2 = vec![b"entry2".to_vec()];
        let root2 = calculate_merkle_root(&entries2);
        assert_ne!(root, root2);
    }

    #[test]
    fn test_multiple_entries() {
        let entries = vec![
            b"entry1".to_vec(),
            b"entry2".to_vec(),
            b"entry3".to_vec(),
            b"entry4".to_vec(),
        ];
        let root = calculate_merkle_root(&entries);
        assert_eq!(root.len(), 32);

        // Reordering should change the root
        let entries2 = vec![
            b"entry2".to_vec(),
            b"entry1".to_vec(),
            b"entry3".to_vec(),
            b"entry4".to_vec(),
        ];
        let root2 = calculate_merkle_root(&entries2);
        assert_ne!(root, root2);
    }

    #[test]
    fn test_odd_number_of_entries() {
        let entries = vec![b"entry1".to_vec(), b"entry2".to_vec(), b"entry3".to_vec()];
        let root = calculate_merkle_root(&entries);
        assert_eq!(root.len(), 32);
    }

    #[test]
    fn test_deterministic() {
        let entries = vec![b"entry1".to_vec(), b"entry2".to_vec(), b"entry3".to_vec()];

        let root1 = calculate_merkle_root(&entries);
        let root2 = calculate_merkle_root(&entries);
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_verify_inclusion() {
        let entries = vec![b"entry1".to_vec(), b"entry2".to_vec(), b"entry3".to_vec()];
        let root = calculate_merkle_root(&entries);

        // Valid entry
        assert!(verify_entry_inclusion(&entries, b"entry2", &root));

        // Invalid entry
        assert!(!verify_entry_inclusion(&entries, b"entry4", &root));
    }

    #[test]
    fn test_verify_with_wrong_root() {
        let entries = vec![b"entry1".to_vec(), b"entry2".to_vec()];
        let _root = calculate_merkle_root(&entries);

        let wrong_root = [0u8; 32];
        assert!(!verify_entry_inclusion(&entries, b"entry1", &wrong_root));
    }

    #[test]
    fn test_single_bit_change_affects_root() {
        let entries1 = vec![b"entry1".to_vec()];
        let entries2 = vec![b"entry2".to_vec()]; // One bit different

        let root1 = calculate_merkle_root(&entries1);
        let root2 = calculate_merkle_root(&entries2);

        assert_ne!(root1, root2);
    }
}
