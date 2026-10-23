// Copyright 2026 AsterSQL.
//! Backend-neutral PD metadata semantics. Etcd is a transitional adapter.
#[cfg(test)]
extern crate self as astersql_pd_storage_kv;
mod base_kv;
pub use base_kv::Base;
mod callback_txn;
pub use callback_txn::{CallbackTxn, CallbackTxnError};
mod etcd_store;
mod memory_store;
mod meta_store;
mod revision_txn;
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
mod callback_txn_test;

#[cfg(test)]
#[allow(dead_code)]
#[path = "etcd_fixture_test.rs"]
mod fixture;

#[cfg(test)]
mod revision_txn_test;

mod raw_compare;
pub use raw_compare::{RawTxnCmpType, RawTxnCondition};
#[cfg(test)]
mod raw_compare_test;

mod raw_branches;
pub use raw_branches::RawTxn;
#[cfg(test)]
mod raw_branches_test;

mod raw_get;
pub use raw_get::{KeyValuePair, RawTxnOp, RawTxnResponse, RawTxnResponseItem};
#[cfg(test)]
mod raw_get_test;

mod raw_range;
#[cfg(test)]
mod raw_range_test;

mod txn_cancel;
#[cfg(test)]
mod txn_cancel_test;
