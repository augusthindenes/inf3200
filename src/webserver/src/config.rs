// The size of the identifier circle (2^M)
// Meaning we use M-bit identifiers (u64)
pub const M: u32 = 16; // 16 bits = 2^16 identifiers (65536 possible IDs)
pub const HOP_LIMIT: u32 = 16;
pub const IDLE_LIMIT: u64 = 10; // in minutes
pub const MAINTENANCE_INTERVAL_MS: u64 = 500; // 10 seconds