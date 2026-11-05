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

mod group_catalog;
pub use group_catalog::*;
#[cfg(test)]
#[path = "group_catalog_test.rs"]
mod group_catalog_test;

mod group_membership;
pub use group_membership::*;
#[cfg(test)]
#[path = "group_membership_test.rs"]
mod group_membership_test;

mod group_split;
pub use group_split::*;
#[cfg(test)]
#[path = "group_split_test.rs"]
mod group_split_test;

mod group_merge;
pub use group_merge::*;
#[cfg(test)]
#[path = "group_merge_test.rs"]
mod group_merge_test;

mod group_nodes;
pub use group_nodes::*;
#[cfg(test)]
#[path = "group_nodes_test.rs"]
mod group_nodes_test;

mod group_reconcile;
pub use group_reconcile::*;
#[cfg(test)]
#[path = "group_reconcile_test.rs"]
mod group_reconcile_test;

mod group_patrol;
pub use group_patrol::*;
#[cfg(test)]
#[path = "group_patrol_test.rs"]
mod group_patrol_test;

mod keyspace_split;
pub use keyspace_split::*;
#[cfg(test)]
#[path = "keyspace_split_test.rs"]
mod keyspace_split_test;

mod keyspace_create;
pub use keyspace_create::*;
#[cfg(test)]
#[path = "keyspace_create_test.rs"]
mod keyspace_create_test;

mod keyspace_config;
pub use keyspace_config::*;
#[cfg(test)]
#[path = "keyspace_config_test.rs"]
mod keyspace_config_test;

mod keyspace_state;
pub use keyspace_state::*;
#[cfg(test)]
#[path = "keyspace_state_test.rs"]
mod keyspace_state_test;
