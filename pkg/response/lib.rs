// Copyright 2026 AsterSQL.
//! Protocol-independent HTTP response models.
pub mod peer;
pub use peer::*;
#[cfg(test)]
#[path = "peer_test.rs"]
mod peer_test;
pub mod region;
pub use region::*;
#[cfg(test)]
#[path = "region_test.rs"]
mod region_test;
