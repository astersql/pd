// Copyright 2026 AsterSQL.

/// START_KEYSPACE_ID is the first valid keyspace ID.
pub const START_KEYSPACE_ID: u32 = 0;

/// DEFAULT_KEYSPACE_ID identifies the default keyspace.
pub const DEFAULT_KEYSPACE_ID: u32 = 0;

/// MAX_VALID_KEYSPACE_ID is the largest ID representable by the 24-bit
/// keyspace field.
pub const MAX_VALID_KEYSPACE_ID: u32 = 0x00ff_ffff;

/// VALID_KEYSPACE_ID_MASK contains every valid keyspace ID bit.
pub const VALID_KEYSPACE_ID_MASK: u32 = 0x00ff_ffff;

/// NULL_KEYSPACE_ID identifies keyspace-agnostic and legacy API paths.
pub const NULL_KEYSPACE_ID: u32 = u32::MAX;

/// DEFAULT_KEYSPACE_GROUP_ID identifies the default keyspace group.
pub const DEFAULT_KEYSPACE_GROUP_ID: u32 = 0;

#[cfg(test)]
mod constant_test;
