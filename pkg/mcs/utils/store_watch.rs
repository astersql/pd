// Copyright 2026 AsterSQL.
//! Shared pull-based observation of PD-owned store metadata into canonical stores.
use astersql_pd_core::{StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_grpcutil::core_store_adapter::store_from_proto;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::metapb;
use astersql_pd_storage_domain::{Event, MetaStore, StoreError, WatchBatch};
use prost::Message;
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
/// The embedding host drives IO and retries. Metadata replacement preserves
/// local heartbeat statistics; compaction recovery reconciles deleted stores.
pub struct StoreWatcher {
    backend: Arc<dyn MetaStore>,
    paths: ClusterPaths,
    stores: StoresInfo,
    watch: ReconciledWatch,
}
impl StoreWatcher {
    pub async fn load(
        backend: Arc<dyn MetaStore>,
        paths: ClusterPaths,
        stores: StoresInfo,
        cancel: &CancellationToken,
    ) -> Result<Self, StoreError> {
        let (snapshot, watch) = ReconciledWatch::load(
            backend.clone(),
            paths.store_path_prefix().into_bytes(),
            true,
            128,
            cancel,
        )
        .await?;
        let changes = snapshot
            .entries
            .into_iter()
            .map(Event::Put)
            .collect::<Vec<_>>();
        let mut decoded = decode(&WatchBatch {
            events: changes,
            revision: snapshot.revision,
        })?;
        let present: BTreeSet<_> = decoded
            .iter()
            .filter_map(|(id, meta)| meta.as_ref().map(|_| *id))
            .collect();
        decoded.extend(
            stores
                .get_store_ids()
                .into_iter()
                .filter(|id| !present.contains(id))
                .map(|id| (id, None)),
        );
        publish(&stores, decoded);
        Ok(Self {
            backend,
            paths,
            stores,
            watch,
        })
    }
    pub async fn advance(&mut self, cancel: &CancellationToken) -> Result<(), StoreError> {
        let stores = &self.stores;
        self.watch
            .apply_next(cancel, |batch| {
                let changes = decode(batch)?;
                publish(stores, changes);
                Ok(())
            })
            .await
            .map(|_| ())
    }
    pub async fn reload(&mut self, cancel: &CancellationToken) -> Result<(), StoreError> {
        let replacement = Self::load(
            self.backend.clone(),
            self.paths,
            self.stores.clone(),
            cancel,
        )
        .await?;
        *self = replacement;
        Ok(())
    }
}
fn decode(batch: &WatchBatch) -> Result<Vec<(u64, Option<StoreMeta>)>, StoreError> {
    batch
        .events
        .iter()
        .map(|event| match event {
            Event::Put(row) => metapb::Store::decode(row.value.as_slice())
                .map(|store| (store.id, store_from_proto(Some(store))))
                .map_err(|_| StoreError::Backend),
            Event::Delete { key, .. } => std::str::from_utf8(key)
                .ok()
                .and_then(|s| s.rsplit('/').next())
                .and_then(|id| id.parse().ok())
                .map(|id| (id, None))
                .ok_or(StoreError::Backend),
        })
        .collect()
}
fn publish(stores: &StoresInfo, changes: Vec<(u64, Option<StoreMeta>)>) {
    for (id, meta) in changes {
        if let Some(meta) = meta {
            if !stores.patch_store(id, |store| store.set_store_meta(&meta)) {
                stores.put_store(Arc::new(StoreInfo::new(meta)));
            }
        } else {
            stores.delete_store(id);
        }
    }
}
