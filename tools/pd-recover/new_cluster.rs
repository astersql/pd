// Copyright 2026 AsterSQL.

use std::time::{SystemTime, UNIX_EPOCH};

use astersql_pd_keypath::{CLUSTER_ID_PATH, ClusterPaths};
use astersql_pd_storage_endpoint::Cluster;
use etcd_client::{Client, Compare, CompareOp, Txn, TxnOp};

use crate::{Result, metadata_u64, rejected, unchanged};

/// Bootstrap the supplied cluster identity and ID boundary in a single transaction.
pub async fn recover_new(client: &mut Client, id: u64, alloc_id: u64) -> Result<()> {
    recover_new_with_timestamp(client, id, alloc_id, None).await
}

/// Recover a new cluster and optionally preserve the backup's default TSO boundary.
pub async fn recover_new_with_timestamp(
    client: &mut Client,
    id: u64,
    alloc_id: u64,
    timestamp: Option<u64>,
) -> Result<()> {
    if id == 0 || alloc_id == 0 {
        return Err(rejected("cluster and allocator IDs must be nonzero"));
    }
    if timestamp.is_some_and(|n| n == 0 || n > i64::MAX as u64) {
        return Err(rejected("invalid timestamp boundary"));
    }
    let paths = ClusterPaths::new(id);
    let cluster = paths.cluster_path();
    let alloc = paths.alloc_id_path();
    let identity = client.get(CLUSTER_ID_PATH, None).await?;
    let allocator = client.get(alloc.as_str(), None).await?;
    if metadata_u64(&identity)?.is_some_and(|existing| existing != id) {
        return Err(rejected("existing cluster identity differs"));
    }
    if metadata_u64(&allocator)?.is_some_and(|existing| existing > alloc_id) {
        return Err(rejected("allocator boundary would move backwards"));
    }
    let bootstrap = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos())?;
    let mut compares = vec![
        Compare::create_revision(cluster.as_str(), CompareOp::Equal, 0),
        unchanged(CLUSTER_ID_PATH, &identity),
        unchanged(&alloc, &allocator),
    ];
    let mut writes = vec![
        TxnOp::put(CLUSTER_ID_PATH, id.to_be_bytes(), None),
        TxnOp::put(alloc, alloc_id.to_be_bytes(), None),
        TxnOp::put(
            cluster,
            Cluster {
                id,
                ..Default::default()
            }
            .encode(),
            None,
        ),
        TxnOp::put(
            paths.cluster_bootstrap_time_path(),
            bootstrap.to_be_bytes(),
            None,
        ),
    ];
    if let Some(timestamp) = timestamp {
        let key = paths.timestamp_path(0);
        let existing = client.get(key.as_str(), None).await?;
        if metadata_u64(&existing)?.is_some_and(|n| n > timestamp) {
            return Err(rejected("timestamp boundary would move backwards"));
        }
        compares.push(unchanged(&key, &existing));
        writes.push(TxnOp::put(key, timestamp.to_be_bytes(), None));
    }
    let response = client
        .txn(Txn::new().when(compares).and_then(writes))
        .await?;
    if !response.succeeded() {
        return Err(rejected("cluster already bootstrapped or metadata changed"));
    }
    Ok(())
}
