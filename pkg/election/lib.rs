// Copyright 2026 AsterSQL.
//! Immutable leadership terms and fenced metadata transactions.
mod leadership;
pub use leadership::*;
#[cfg(test)]
mod campaign_key_test;
#[cfg(test)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;
#[cfg(test)]
mod leadership_test;

mod lease_expiry;
pub use lease_expiry::*;
#[cfg(test)]
mod lease_expiry_test;
