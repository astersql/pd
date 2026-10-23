// Copyright 2026 AsterSQL.
//! Leader-fenced persistent global and keyspace ID windows.
mod allocator;
pub use allocator::*;
mod allocator_namespace;
pub use allocator_namespace::*;
#[cfg(test)]
extern crate self as astersql_pd_id;
#[cfg(test)]
mod allocate_count_test;
#[cfg(test)]
mod allocator_base_test;
#[cfg(test)]
mod allocator_namespace_test;
#[cfg(test)]
mod allocator_test;
