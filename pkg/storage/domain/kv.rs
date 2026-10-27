// Copyright 2026 AsterSQL.
//! Canonical backend-neutral KV contracts; transport adapters depend on this module.
#[path = "../kv/base_kv.rs"]
mod base_kv;
#[path = "../kv/callback_txn.rs"]
mod callback_txn;
#[path = "../kv/memory_store.rs"]
mod memory_store;
#[path = "../kv/meta_store.rs"]
mod meta_store;
#[path = "../kv/raw_branches.rs"]
mod raw_branches;
#[path = "../kv/raw_compare.rs"]
mod raw_compare;
#[path = "../kv/raw_get.rs"]
pub mod raw_get;
#[path = "../kv/raw_range.rs"]
pub mod raw_range;
#[path = "../kv/revision_txn.rs"]
mod revision_txn;
#[path = "../kv/txn_cancel.rs"]
pub mod txn_cancel;

pub use base_kv::Base;
pub use callback_txn::{CallbackTxn, CallbackTxnError};
pub use memory_store::MemoryStore;
pub use meta_store::*;
pub use raw_branches::RawTxn;
pub use raw_compare::{RawTxnCmpType, RawTxnCondition};
pub use raw_get::{KeyValuePair, RawTxnOp, RawTxnResponse, RawTxnResponseItem};
