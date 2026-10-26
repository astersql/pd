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
