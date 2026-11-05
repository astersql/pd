// Copyright 2026 AsterSQL.
//! GC state management over host-supplied metadata and keyspace resources.
mod gc_read;
pub use gc_read::*;
#[cfg(test)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;
#[cfg(test)]
mod gc_read_test;

#[cfg(test)]
#[path = "../storage/kv/response_gate_test.rs"]
mod response_gate_test;

mod txn_safe_point;
pub use txn_safe_point::*;
#[cfg(test)]
mod txn_safe_point_test;

mod gc_barrier;
pub use gc_barrier::*;
#[cfg(test)]
mod gc_barrier_test;

mod gc_global_barrier;
pub use gc_global_barrier::*;
#[cfg(test)]
mod gc_global_barrier_test;
