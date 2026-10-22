// Copyright 2026 AsterSQL.
//! Protocol-independent core domain models.

pub mod peer;
pub use peer::*;

#[cfg(test)]
#[path = "peer_test.rs"]
mod peer_test;

pub mod region_meta;
pub use region_meta::*;

#[cfg(test)]
#[path = "region_meta_test.rs"]
mod region_meta_test;

pub mod store_meta;
pub use store_meta::*;

#[cfg(test)]
#[path = "store_meta_test.rs"]
mod store_meta_test;

#[cfg(test)]
#[path = "store_state_test.rs"]
mod store_state_test;

pub mod store_state;
pub use store_state::*;
