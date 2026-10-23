// Copyright 2026 AsterSQL.
//! Default storage facade over canonical KV and endpoint implementations.
pub use astersql_pd_storage_kv::*;
#[cfg(test)]
#[path = "kv/meta_store_test.rs"]
mod meta_store_test;

mod facade;
pub use facade::Storage;
#[cfg(test)]
mod facade_test;

mod region_switch;
#[cfg(test)]
mod region_switch_test;

#[cfg(test)]
#[path = "kv/etcd_fixture_test.rs"]
#[allow(dead_code)]
mod etcd_fixture;

mod region_load_once;
#[cfg(test)]
mod region_load_once_test;

mod region_lifecycle;
pub use region_lifecycle::{RegionLifecycle, RegionLifecycleError};
#[cfg(test)]
mod region_lifecycle_test;
