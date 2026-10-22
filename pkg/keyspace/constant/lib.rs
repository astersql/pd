// Copyright 2026 AsterSQL.

/// DEFAULT_KEYSPACE_NAME is reserved for the default keyspace.
pub const DEFAULT_KEYSPACE_NAME: &str = "DEFAULT";

/// START_KEYSPACE_ID is the first valid keyspace ID.
pub const START_KEYSPACE_ID: u32 = 0;

/// DEFAULT_KEYSPACE_ID identifies the default keyspace.
pub const DEFAULT_KEYSPACE_ID: u32 = 0;

/// MAX_VALID_KEYSPACE_ID is the largest ID representable by the 24-bit
/// keyspace field.
pub const MAX_VALID_KEYSPACE_ID: u32 = 0x00ff_ffff;

/// REGION_LABEL_ID_PREFIX prefixes keyspace region label rule IDs.
pub const REGION_LABEL_ID_PREFIX: &str = "keyspaces/";

/// REGION_LABEL_KEY stores the keyspace ID in a region label.
pub const REGION_LABEL_KEY: &str = "id";

/// VALID_KEYSPACE_ID_MASK contains every valid keyspace ID bit.
pub const VALID_KEYSPACE_ID_MASK: u32 = 0x00ff_ffff;

/// NULL_KEYSPACE_ID identifies keyspace-agnostic and legacy API paths.
pub const NULL_KEYSPACE_ID: u32 = u32::MAX;

/// DEFAULT_KEYSPACE_GROUP_ID identifies the default keyspace group.
pub const DEFAULT_KEYSPACE_GROUP_ID: u32 = 0;

/// RESERVED_KEYSPACE_ID_COUNT is the number of IDs reserved for next-gen
/// keyspaces.
pub const RESERVED_KEYSPACE_ID_COUNT: u64 = 1024;

/// RESERVED_KEYSPACE_ID_START is the inclusive start of the reserved ID
/// range.
pub const RESERVED_KEYSPACE_ID_START: u64 =
    MAX_VALID_KEYSPACE_ID as u64 - RESERVED_KEYSPACE_ID_COUNT;

/// SYSTEM_KEYSPACE_ID identifies the system keyspace.
pub const SYSTEM_KEYSPACE_ID: u32 = MAX_VALID_KEYSPACE_ID - 1;

/// SYSTEM_KEYSPACE_NAME is reserved for the system keyspace.
pub const SYSTEM_KEYSPACE_NAME: &str = "SYSTEM";

#[cfg(test)]
mod constant_test;
