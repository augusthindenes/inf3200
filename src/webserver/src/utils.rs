use sha1::{Digest, Sha1};
use crate::config::M;

// Function to hash a key using SHA-1 and return a u64 identifier
pub fn hash_key(key: &str) -> u64 {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    let result = hasher.finalize();
    // Use the first M / 8 bytes of the hash as the identifier
    let n = M as usize / 8;
    let mut id_bytes = [0u8; 8];
    id_bytes[8 - n..].copy_from_slice(&result[..n]);
    u64::from_be_bytes(id_bytes)
}

// Check if id is in the (start, end) interval on the identifier circle
pub fn in_interval_open_open(id: u64, start: u64, end: u64) -> bool {
    if start < end {
        id > start && id < end
    } else if start > end {
        id > start || id < end
    } else {
        false
    }
}

// Check if id is in the (start, end] interval on the identifier circle
pub fn in_interval_open_closed(id: u64, start: u64, end: u64) -> bool {
    if start < end {
        id > start && id <= end
    } else if start > end {
        id > start || id <= end
    } else {
        // start == end means the whole circle (only true with 1 node); treat as owned
        true
    }
}

// Unit tests for utility functions
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hash_key() {
        // Test that hash_key produces a u64 within the identifier space
        let key = "example_key";
        let id = hash_key(key);
        if M < 64 {
            assert!(id < (1u64 << M));
        }
        // When M == 64, any u64 value is valid
    }
    #[test]
    fn test_in_interval_open_open() {
        assert!(in_interval_open_open(5, 3, 7)); // 5 is between 3 and 7
        assert!(!in_interval_open_open(3, 3, 7)); // 3 is not in (3,7)
        assert!(!in_interval_open_open(7, 3, 7)); // 7 is not in (3,7)
        assert!(in_interval_open_open(1, 7, 3)); // Wrap around case
        assert!(!in_interval_open_open(7, 7, 3)); // 7 is not in (7,3)
    }
    #[test]
    fn test_in_interval_open_closed() {
        assert!(in_interval_open_closed(5, 3, 7)); // 5 is between 3 and 7
        assert!(!in_interval_open_closed(3, 3, 7)); // 3 is not in (3,7]
        assert!(in_interval_open_closed(7, 3, 7)); // 7 is in (3,7]
        assert!(in_interval_open_closed(1, 7, 3)); // Wrap around case
        assert!(!in_interval_open_closed(7, 7, 3)); // 7 is not in (7,3]
    }
}