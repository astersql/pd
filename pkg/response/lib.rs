// Copyright 2026 AsterSQL.
//! Protocol-independent HTTP response models.
pub mod peer;
pub use peer::*;
#[cfg(test)]
#[path = "peer_test.rs"]
mod peer_test;
