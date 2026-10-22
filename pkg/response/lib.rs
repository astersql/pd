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
pub mod regions;
pub use regions::*;
#[cfg(test)]
#[path = "regions_test.rs"]
mod regions_test;

pub mod store;
pub use store::*;
#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
