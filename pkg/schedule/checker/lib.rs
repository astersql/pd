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
