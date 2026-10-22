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

pub mod store_labels;
pub use store_labels::*;
#[cfg(test)]
#[path = "store_labels_test.rs"]
mod store_labels_test;

pub mod store_stats;
pub use store_stats::*;
#[cfg(test)]
#[path = "store_stats_test.rs"]
mod store_stats_test;

pub mod store_stats_average;
#[cfg(test)]
#[path = "store_stats_average_test.rs"]
mod store_stats_average_test;

#[cfg(test)]
#[path = "store_clone_test.rs"]
mod store_clone_test;

pub mod store_clone;

#[cfg(test)]
#[path = "store_time_test.rs"]
mod store_time_test;

pub mod store_time;

#[cfg(test)]
#[path = "store_control_test.rs"]
mod store_control_test;

pub mod store_control;

pub mod store_limit;
pub use store_limit::*;
#[cfg(test)]
#[path = "store_limit_test.rs"]
mod store_limit_test;

pub mod store_resource;
#[cfg(test)]
#[path = "store_resource_test.rs"]
mod store_resource_test;

#[cfg(test)]
#[path = "store_score_v1_test.rs"]
mod store_score_v1_test;

pub mod store_score_v1;

#[cfg(test)]
#[path = "store_score_v2_test.rs"]
mod store_score_v2_test;

pub mod store_score_v2;

#[cfg(test)]
#[path = "store_health_test.rs"]
mod store_health_test;

pub mod store_health;
