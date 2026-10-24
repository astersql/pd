// Copyright 2026 AsterSQL.
//! Heartbeat state composition; scheduling and transport belong to their owners.
#![forbid(unsafe_code)]

mod store_heartbeat;
pub use store_heartbeat::{StoreNotFound, handle_store_heartbeat};

#[cfg(test)]
mod store_heartbeat_test;

mod removed_store;
#[cfg(test)]
mod removed_store_test;

mod region_heartbeat;
pub use region_heartbeat::{
    PreparedRegionHeartbeat, RegionHeartbeatResult, handle_region_heartbeat,
    prepare_region_heartbeat,
};
#[cfg(test)]
mod region_heartbeat_test;

mod region_overlap;
pub use region_overlap::handle_region_heartbeat_with_overlap_cleanup;
#[cfg(test)]
mod region_overlap_test;
