// Copyright 2026 AsterSQL.
//! Stable participant identities from the original memberutil package.
use sha2::{Digest, Sha256};
/// Use Go GenerateUniqueID's first eight SHA-256 bytes in little-endian order.
pub fn generate_unique_id(seed: &str) -> u64 {
    let digest = Sha256::digest(seed.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().unwrap())
}
#[cfg(test)]
mod member_test;
