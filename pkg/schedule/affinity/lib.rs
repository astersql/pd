// Copyright 2026 AsterSQL.
#[cfg(test)]
#[path = "affinity_create_delete_test.rs"]
mod affinity_create_delete_test;
#[cfg(test)]
#[path = "affinity_ranges_test.rs"]
mod affinity_ranges_test;
#[cfg(test)]
#[path = "affinity_restore_test.rs"]
mod affinity_restore_test;
#[cfg(test)]
#[path = "affinity_values_test.rs"]
mod affinity_values_test;
pub mod group;
pub mod manager;
pub mod parse;
pub mod ranges;
pub mod txn;
