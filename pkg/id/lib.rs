// Copyright 2026 AsterSQL.
//! Leader-fenced persistent global ID windows.
mod allocator;
pub use allocator::*;
#[cfg(test)]
extern crate self as astersql_pd_id;
#[cfg(test)]
mod allocate_count_test;
#[cfg(test)]
mod allocator_base_test;
#[cfg(test)]
mod allocator_test;
