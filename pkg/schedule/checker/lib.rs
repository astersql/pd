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
