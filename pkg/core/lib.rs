// Copyright 2026 AsterSQL.
//! Protocol-independent core domain models.

pub mod peer;
pub use peer::*;

#[cfg(test)]
#[path = "peer_test.rs"]
mod peer_test;
