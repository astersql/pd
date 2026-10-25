// Copyright 2026 AsterSQL.
#![forbid(unsafe_code)]
mod role_checkers;
pub use role_checkers::*;
#[cfg(test)]
mod task_039_test;

mod replica_strategy;
pub use replica_strategy::*;
#[cfg(test)]
mod task_040_test;

mod replica_fault;
pub use replica_fault::*;
#[cfg(test)]
mod task_041_test;

mod replica_shape;
pub use replica_shape::*;
#[cfg(test)]
mod task_042_test;

mod rule_missing;
pub use rule_missing::*;
#[cfg(test)]
mod task_043_test;

mod rule_fault;
pub use rule_fault::*;
#[cfg(test)]
mod task_044_test;

mod rule_location;
pub use rule_location::*;
#[cfg(test)]
mod task_045_test;

mod merge_guard;
pub use merge_guard::*;
#[cfg(test)]
mod task_046_test;

mod merge_decision;
pub use merge_decision::*;
#[cfg(test)]
mod task_047_test;

mod split_checker;
pub use split_checker::*;
#[cfg(test)]
mod task_048_test;

mod affinity_peer;
pub use affinity_peer::*;
#[cfg(test)]
mod task_049_test;

mod affinity_merge;
#[cfg(test)]
mod task_050_test;

mod priority_inspector;
pub use priority_inspector::*;
#[cfg(test)]
mod task_051_test;

mod split_scatter_group;
pub use split_scatter_group::*;
#[cfg(test)]
mod task_052_test;

mod split_scatter_observe;
pub use split_scatter_observe::*;
#[cfg(test)]
mod task_053_test;

mod split_scatter_retry;
pub use split_scatter_retry::*;
#[cfg(test)]
mod task_054_test;

mod checker_dispatch;
pub use checker_dispatch::*;
#[cfg(test)]
mod task_055_test;

mod patrol;
pub use patrol::*;
#[cfg(test)]
mod task_056_test;

mod suspect_ranges;
pub use suspect_ranges::*;
#[cfg(test)]
mod task_057_test;
