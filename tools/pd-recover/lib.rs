// Copyright 2026 AsterSQL.
//! Offline recovery operations for PD metadata stored in etcd.

mod new_cluster;
mod old_member;
mod preflight;

use std::io;

use etcd_client::{Compare, CompareOp, GetResponse};

pub use new_cluster::{recover_new, recover_new_with_timestamp};
pub use old_member::recover_old;
pub use preflight::{Action, USAGE, parse};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn metadata_u64(response: &GetResponse) -> Result<Option<u64>> {
    response
        .kvs()
        .first()
        .map(|kv| Ok(u64::from_be_bytes(kv.value().try_into()?)))
        .transpose()
}

fn unchanged(key: &str, response: &GetResponse) -> Compare {
    match response.kvs().first() {
        Some(kv) => Compare::mod_revision(key, CompareOp::Equal, kv.mod_revision()),
        None => Compare::create_revision(key, CompareOp::Equal, 0),
    }
}

fn rejected(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::other(message).into()
}
