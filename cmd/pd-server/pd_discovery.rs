// Copyright 2026 AsterSQL.
//! Explicit deployment policy; etcd membership and PD election are distinct.
use astersql_pd_storage::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
pub(crate) fn lookup(
    policy: &str,
    backend: EtcdMetaStore,
    cluster: u64,
    cancel: CancellationToken,
) -> astersql_pd_runtime::PdLeaderLookup {
    let primary = policy == "primary-key";
    let key = astersql_pd_keypath::ClusterPaths::new(cluster).election_path(None);
    Arc::new(move || {
        let backend = backend.clone();
        let key = key.clone();
        let cancel = cancel.clone();
        Box::pin(async move {
            if !primary {
                return backend
                    .consensus_leader_client_url()
                    .await
                    .map_err(|e| e.to_string());
            }
            let read = async {
                let snapshot = backend
                    .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                    .await
                    .map_err(|e| e.to_string())?;
                let Some(entry) = snapshot.entries.first() else {
                    return Ok(None);
                };
                let member = astersql_pd_proto::pdpb::Member::decode(entry.value.as_slice())
                    .map_err(|e| e.to_string())?;
                Ok(member.client_urls.into_iter().next())
            };
            tokio::select! {biased;
                _=cancel.cancelled()=>Err("PD discovery cancelled".into()),
                result=tokio::time::timeout(Duration::from_secs(2),read)=>result.map_err(|_|"PD discovery timed out".to_owned())?,
            }
        })
    })
}
