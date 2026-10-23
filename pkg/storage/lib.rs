// Copyright 2026 AsterSQL.
//! Compatibility facade for the canonical metadata implementation in storage-kv.
pub use astersql_pd_storage_kv::*;
#[cfg(test)]
#[path = "kv/meta_store_test.rs"]
mod meta_store_test;
