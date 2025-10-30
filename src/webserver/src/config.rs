// The size of the identifier circle (2^M)
// Meaning we use M-bit identifiers (u64)
pub const M: u32 = 64; // 64 bits = 2^64 identifiers
pub const HOP_LIMIT: u32 = 64;
pub const IDLE_LIMIT: u64 = 10; // in minutes