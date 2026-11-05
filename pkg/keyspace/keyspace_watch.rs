// Copyright 2026 AsterSQL.
//! Revision-pinned metadata observation over the existing recoverable watch.
use crate::{KeyspaceError, KeyspaceManager};
use astersql_pd_etcdutil::ReconciledWatch;
use astersql_pd_storage_domain::{Event, Keyspace, KeyspaceStorage, StoreError};
use tokio_util::sync::CancellationToken;
pub struct KeyspaceWatch {
    storage: KeyspaceStorage,
    watch: ReconciledWatch,
}
impl KeyspaceManager {
    pub async fn watch_metadata(
        &self,
        cancel: &CancellationToken,
    ) -> Result<(Vec<Keyspace>, KeyspaceWatch), KeyspaceError> {
        let storage = self.metadata_storage();
        let (snapshot, watch) = ReconciledWatch::load(
            storage.store.clone(),
            storage.paths.keyspace_meta_prefix().into_bytes(),
            true,
            100,
            cancel,
        )
        .await?;
        let metas = snapshot
            .entries
            .iter()
            .map(|e| {
                storage
                    .decode_persisted(&e.value)
                    .map_err(KeyspaceError::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((metas, KeyspaceWatch { storage, watch }))
    }
}
impl KeyspaceWatch {
    pub async fn next(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<Vec<Keyspace>, KeyspaceError> {
        let mut metas = vec![];
        self.watch
            .apply_next(cancel, |batch| {
                let decoded = batch
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let Event::Put(row) = e {
                            Some(
                                self.storage
                                    .decode_persisted(&row.value)
                                    .map_err(|_| StoreError::Backend),
                            )
                        } else {
                            None
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                metas = decoded;
                Ok(())
            })
            .await?;
        Ok(metas)
    }
}
