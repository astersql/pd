// Copyright 2026 AsterSQL.

use std::io;

use astersql_pd_keypath::{CLUSTER_ID_PATH, ClusterPaths};
use etcd_client::{Client, Compare, CompareOp, GetResponse, Txn, TxnOp, TxnOpResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Go-compatible metadata backup. Region and store data are outside this format.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Existing cluster identity.
    #[serde(rename = "clusterID")]
    pub cluster_id: u64,
    /// Persisted allocator boundary.
    #[serde(rename = "allocIDMax")]
    pub alloc_id_max: u64,
    /// Persisted timestamp boundary for the default group.
    #[serde(rename = "allocTimestampMax")]
    pub alloc_timestamp_max: u64,
    /// Configuration captured by the caller from PD's config API.
    pub config: Value,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn number(response: &GetResponse) -> Result<Option<u64>> {
    response
        .kvs()
        .first()
        .map(|kv| Ok(u64::from_be_bytes(kv.value().try_into()?)))
        .transpose()
}

/// Capture ID and timestamp boundaries from one linearizable metadata transaction.
pub async fn collect(client: &mut Client, config: Value) -> Result<BackupInfo> {
    let identity = client.get(CLUSTER_ID_PATH, None).await?;
    let cluster_id = number(&identity)?
        .filter(|id| *id != 0)
        .ok_or_else(|| io::Error::other("missing cluster identity"))?;
    let paths = ClusterPaths::new(cluster_id);
    let result = client
        .txn(
            Txn::new()
                .when([Compare::mod_revision(
                    CLUSTER_ID_PATH,
                    CompareOp::Equal,
                    identity.kvs()[0].mod_revision(),
                )])
                .and_then([
                    TxnOp::get(paths.alloc_id_path(), None),
                    TxnOp::get(paths.timestamp_path(0), None),
                ]),
        )
        .await?;
    if !result.succeeded() {
        return Err(io::Error::other("cluster identity changed while collecting backup").into());
    }
    let ops = result.op_responses();
    let [TxnOpResponse::Get(alloc), TxnOpResponse::Get(timestamp)] = ops.as_slice() else {
        return Err(io::Error::other("unexpected backup transaction response").into());
    };
    Ok(BackupInfo {
        cluster_id,
        alloc_id_max: number(alloc)?.unwrap_or_default(),
        alloc_timestamp_max: number(timestamp)?
            .ok_or_else(|| io::Error::other("missing persisted timestamp"))?,
        config,
    })
}
