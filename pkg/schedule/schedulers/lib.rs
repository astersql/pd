// Copyright 2026 AsterSQL.
//! Built-in scheduler contracts; concrete implementations register only when ported.
mod base;
pub use base::*;
#[cfg(test)]
mod task_065_test;

mod config;
pub use config::*;
#[cfg(test)]
mod task_066_test;

mod balance_solver;
pub use balance_solver::*;
#[cfg(test)]
mod task_067_test;

#[cfg(test)]
mod task_068_test;

mod range_view;
pub use range_view::*;

mod load_compare;
pub use load_compare::*;
#[cfg(test)]
mod task_069_test;

mod balance_leader;
pub use balance_leader::*;
#[cfg(test)]
mod task_070_test;

mod balance_leader_config;
pub use balance_leader_config::*;
#[cfg(test)]
mod task_071_test;

#[cfg(test)]
mod task_072_test;

mod balance_region;
pub use balance_region::*;

mod balance_witness;
pub use balance_witness::*;
#[cfg(test)]
mod task_073_test;

#[cfg(test)]
mod task_074_test;

mod balance_witness_config;

mod evict_leader;
pub use evict_leader::*;
#[cfg(test)]
mod task_075_test;

mod evict_leader_config;
pub use evict_leader_config::*;
#[cfg(test)]
mod task_076_test;

#[cfg(test)]
mod task_077_test;

mod grant_leader;
pub use grant_leader::*;

#[cfg(test)]
mod task_078_test;

mod grant_leader_config;
pub use grant_leader_config::*;

#[cfg(test)]
mod task_079_test;

mod evict_disk;
pub use evict_disk::*;

#[cfg(test)]
mod task_080_test;

mod evict_network_detect;
pub use evict_network_detect::*;

#[cfg(test)]
mod task_081_test;

mod evict_network;
pub use evict_network::*;

#[cfg(test)]
mod task_082_test;

mod evict_stopping;
pub use evict_stopping::*;

mod slow_trend_candidate;
pub use slow_trend_candidate::*;
#[cfg(test)]
mod task_083_test;

#[cfg(test)]
mod task_084_test;

mod slow_trend_recover;
pub use slow_trend_recover::*;

#[cfg(test)]
mod task_085_test;

mod hot_config;
pub use hot_config::*;

mod hot_solution;
pub use hot_solution::*;
#[cfg(test)]
mod task_086_test;

#[cfg(test)]
mod task_087_test;

mod hot_rank_v1;
pub use hot_rank_v1::*;

#[cfg(test)]
mod task_088_test;

mod hot_rank_v2;
pub use hot_rank_v2::*;

mod hot_source;
pub use hot_source::*;
#[cfg(test)]
mod task_089_test;

mod hot_destination;
pub use hot_destination::*;
#[cfg(test)]
mod task_090_test;

mod hot_placement;
pub use hot_placement::*;
#[cfg(test)]
mod task_091_test;

mod hot_solve;
pub use hot_solve::*;
#[cfg(test)]
mod task_092_test;

#[cfg(test)]
mod task_093_test;

mod hot_split;
pub use hot_split::*;

mod hot_schedule;
pub use hot_schedule::*;
#[cfg(test)]
mod task_094_test;

#[cfg(test)]
mod task_095_test;

mod hot_pending;
pub use hot_pending::*;

mod grant_hot;
pub use grant_hot::*;
#[cfg(test)]
mod task_096_test;

mod label;
pub use label::*;
#[cfg(test)]
mod task_097_test;

#[cfg(test)]
mod task_098_test;

mod random_merge;
pub use random_merge::*;

mod scatter_range;
pub use scatter_range::*;
#[cfg(test)]
mod task_099_test;

mod shuffle_hot;
pub use shuffle_hot::*;
#[cfg(test)]
mod task_100_test;

#[cfg(test)]
mod task_101_test;

mod shuffle_leader;
pub use shuffle_leader::*;

#[cfg(test)]
mod task_102_test;

mod shuffle_region;
pub use shuffle_region::*;

#[cfg(test)]
mod task_103_test;

mod transfer_witness;
pub use transfer_witness::*;

mod balance_range_job;
pub use balance_range_job::*;
#[cfg(test)]
mod task_104_test;

mod balance_range_schedule;
#[cfg(test)]
mod task_105_test;
pub use balance_range_schedule::*;

#[cfg(test)]
mod task_106_test;

mod registry;
pub use registry::*;

mod scheduler_control;
pub use scheduler_control::*;
#[cfg(test)]
mod task_107_test;

mod scheduler_loop;
pub use scheduler_loop::*;
#[cfg(test)]
mod task_108_test;

mod diagnostic_record;
pub use diagnostic_record::*;
#[cfg(test)]
mod task_109_test;
