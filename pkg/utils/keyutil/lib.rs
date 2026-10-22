// Copyright 2026 AsterSQL.
//! Byte-key utilities preserving Go keyutil contracts.
mod key_range;
pub use key_range::KeyRange;
mod util;
pub use util::*;

#[cfg(test)]
#[path = "key_range_test.rs"]
mod key_range_test;
