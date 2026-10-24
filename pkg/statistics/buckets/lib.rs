// Copyright 2026 AsterSQL.
//! Owned bucket report values, independent of heartbeat protocols.
mod bucket_values;
pub use bucket_values::*;

#[cfg(test)]
#[path = "bucket_values_test.rs"]
mod bucket_values_test;
