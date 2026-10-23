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
