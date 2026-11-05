// Copyright 2026 AsterSQL.
mod keyspace_bounds;
pub use keyspace_bounds::*;
#[cfg(test)]
#[path = "keyspace_bounds_test.rs"]
mod keyspace_bounds_test;

mod keyspace_cache;
pub use keyspace_cache::*;
#[cfg(test)]
#[path = "keyspace_cache_test.rs"]
mod keyspace_cache_test;

mod meta_group_assign;
pub use meta_group_assign::*;
#[cfg(test)]
#[path = "meta_group_assign_test.rs"]
mod meta_group_assign_test;

mod meta_group_update;
pub use meta_group_update::*;
#[cfg(test)]
#[path = "meta_group_update_test.rs"]
mod meta_group_update_test;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;
