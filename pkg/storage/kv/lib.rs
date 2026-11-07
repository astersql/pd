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
mod candidate_profile_test;
mod candidate_raft;
mod native;
mod native_backup;
#[cfg(test)]
mod native_contract_test;
#[cfg(test)]
mod native_failover_test;
#[cfg(test)]
mod native_open_test;
#[cfg(all(test, unix))]
mod native_process_test;
mod native_state;
mod native_watch;
pub use native::{NativeCommand, NativeMetaStore, NativeRaft, NativeReply};
pub use native_backup::{RecoveryImage, open_isolated, restore_native_backup};
#[cfg(test)]
mod candidate_raft_test;

pub use astersql_pd_storage_domain::{
    HistoryCipher, HistoryError, HistoryHotRegion, HistoryQuery, HotHistory,
};
