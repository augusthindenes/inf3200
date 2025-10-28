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