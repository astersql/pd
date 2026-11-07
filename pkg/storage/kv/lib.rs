// Copyright 2026 AsterSQL.
//! Compatibility facade over neutral contracts and the transitional Etcd adapter.
pub use astersql_pd_storage_domain::kv::*;
#[cfg(test)]
extern crate self as astersql_pd_storage_kv;
mod etcd_store;
pub use etcd_store::EtcdMetaStore;
#[cfg(test)]
mod meta_store_test;

#[cfg(test)]
mod bounded_range_test;
#[cfg(test)]
mod metadata_range_test;

#[cfg(test)]
mod base_kv_test;

#[cfg(test)]
mod callback_txn_test;

#[cfg(test)]
#[allow(dead_code)]
#[path = "etcd_fixture_test.rs"]
mod fixture;

#[cfg(test)]
mod revision_txn_test;

#[cfg(test)]
mod raw_compare_test;

#[cfg(test)]
mod raw_branches_test;

#[cfg(test)]
mod raw_get_test;

#[cfg(test)]
mod raw_range_test;

#[cfg(test)]
mod txn_cancel_test;

#[cfg(test)]
mod candidate;
#[cfg(all(test, unix))]
mod candidate_bench_test;
#[cfg(test)]
mod candidate_commit_test;
#[cfg(test)]
mod candidate_open_test;
#[cfg(test)]
mod candidate_power_vfs_test;
#[cfg(test)]
mod candidate_raft;
#[cfg(test)]
mod candidate_raft_test;

pub use astersql_pd_storage_domain::{
    HistoryCipher, HistoryError, HistoryHotRegion, HistoryQuery, HotHistory,
};
