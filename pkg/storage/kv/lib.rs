// Copyright 2026 AsterSQL.
//! Backend-neutral PD metadata semantics. Etcd is a transitional adapter.
#[cfg(test)]
extern crate self as astersql_pd_storage_kv;
mod base_kv;
pub use base_kv::Base;
mod etcd_store;
mod memory_store;
mod meta_store;
pub use etcd_store::EtcdMetaStore;
pub use memory_store::MemoryStore;
pub use meta_store::*;
#[cfg(test)]
mod meta_store_test;

#[cfg(test)]
mod bounded_range_test;

#[cfg(test)]
mod base_kv_test;

#[cfg(test)]
#[allow(dead_code)]
#[path = "etcd_fixture_test.rs"]
mod fixture;
