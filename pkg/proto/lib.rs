// Copyright 2026 AsterSQL.

//! Wire adapters generated from the exact Go kvproto baseline.
//! Domain crates must not depend on these transport types.

include!(concat!(env!("OUT_DIR"), "/kvproto.rs"));

/// Complete descriptor set for compatibility checks and reflection adapters.
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kvproto.bin"));
/// Source commit shared with both Go modules.
pub const SOURCE_COMMIT: &str = "f8ad079c8e9812ed5900b0c12b5f1d97171464c4";

#[cfg(test)]
#[path = "proto_compat_test.rs"]
mod proto_compat_test;
