// Copyright 2026 AsterSQL.

use astersql_pd_keypath::{CLUSTER_ID_PATH, ClusterPaths};
use etcd_client::{Client, Compare, CompareOp, DeleteOptions, Txn, TxnOp};

use crate::{Result, metadata_u64, rejected, unchanged};

const ID_GUARD: u64 = 100_000_000;

/// Advance the allocator guard and clear only the old cluster's store prefix.
pub async fn recover_old(client: &mut Client) -> Result<()> {
    let identity = client.get(CLUSTER_ID_PATH, None).await?;
    let id = metadata_u64(&identity)?
        .filter(|&id| id != 0)
        .ok_or_else(|| rejected("missing cluster identity"))?;
    let paths = ClusterPaths::new(id);
    let alloc = paths.alloc_id_path();
    let allocator = client.get(alloc.as_str(), None).await?;
    let next = metadata_u64(&allocator)?
        .unwrap_or_default()
        .checked_add(ID_GUARD)
        .ok_or_else(|| rejected("allocator guard would overflow"))?;
    let response = client
        .txn(
            Txn::new()
                .when([
                    Compare::create_revision(paths.cluster_path(), CompareOp::Greater, 0),
                    unchanged(CLUSTER_ID_PATH, &identity),
                    unchanged(&alloc, &allocator),
                ])
                .and_then([
                    TxnOp::put(alloc, next.to_be_bytes(), None),
                    TxnOp::delete(
                        paths.store_path_prefix(),
                        Some(DeleteOptions::new().with_prefix()),
                    ),
                ]),
        )
        .await?;
    if !response.succeeded() {
        return Err(rejected("cluster not bootstrapped or metadata changed"));
    }
    Ok(())
}
