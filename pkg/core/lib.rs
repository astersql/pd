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

pub mod store_location;
pub use store_location::*;
#[cfg(test)]
#[path = "store_location_test.rs"]
mod store_location_test;

#[cfg(test)]
#[path = "store_topology_test.rs"]
mod store_topology_test;

pub mod store_topology;
pub use store_topology::*;

pub mod stores;
pub use stores::*;
#[cfg(test)]
#[path = "stores_test.rs"]
mod stores_test;

pub mod stores_control;
pub use stores_control::*;
#[cfg(test)]
#[path = "stores_control_test.rs"]
mod stores_control_test;

pub mod region;
pub use region::*;
#[cfg(test)]
#[path = "region_test.rs"]
mod region_test;

#[cfg(test)]
#[path = "region_clone_test.rs"]
mod region_clone_test;

pub mod region_clone;
pub use region_clone::*;

pub mod region_flow;
pub use region_flow::*;
#[cfg(test)]
#[path = "region_flow_test.rs"]
mod region_flow_test;

pub mod region_query;
pub use region_query::*;
#[cfg(test)]
#[path = "region_query_test.rs"]
mod region_query_test;

#[cfg(test)]
#[path = "region_merge_policy_test.rs"]
mod region_merge_policy_test;

pub mod region_merge_policy;
pub use region_merge_policy::*;

pub mod region_inherit;
pub use region_inherit::*;
#[cfg(test)]
#[path = "region_inherit_test.rs"]
mod region_inherit_test;

#[cfg(test)]
#[path = "region_buckets_test.rs"]
mod region_buckets_test;

pub mod region_buckets;

#[cfg(test)]
#[path = "region_guide_test.rs"]
mod region_guide_test;

pub mod region_guide;
pub use region_guide::*;

#[cfg(test)]
#[path = "region_format_test.rs"]
mod region_format_test;

pub mod region_format;
pub use region_format::*;

pub mod region_split;
pub use region_split::*;
#[cfg(test)]
#[path = "region_split_test.rs"]
mod region_split_test;

pub mod region_merge;
pub use region_merge::*;
#[cfg(test)]
#[path = "region_merge_test.rs"]
mod region_merge_test;

pub mod region_tree;
pub use region_tree::*;
#[cfg(test)]
#[path = "region_tree_test.rs"]
mod region_tree_test;

mod region_tree_update;
#[cfg(test)]
#[path = "region_tree_update_test.rs"]
mod region_tree_update_test;

mod region_tree_stats;
#[cfg(test)]
#[path = "region_tree_stats_test.rs"]
mod region_tree_stats_test;

mod region_tree_scan;
#[cfg(test)]
#[path = "region_tree_scan_test.rs"]
mod region_tree_scan_test;
