// Copyright 2026 AsterSQL.
use crate::{
    EnabledIndexHooks, EnabledKeyspaceIndex, EnabledSnapshotLoader, GCReadError, GCStateManager,
    GCWarmup,
};
use std::sync::{Arc, atomic::Ordering};
impl GCStateManager {
    /// Opt into discovery on the current host-owned generation; classic passive readers stay passive.
    /// The returned index can receive validated load-concurrency changes between attempts.
    pub async fn start_metadata_discovery(
        self: &Arc<Self>,
        load_concurrency: usize,
        warmup_concurrency: usize,
    ) -> Result<Arc<EnabledKeyspaceIndex>, GCReadError> {
        for (name, value) in [
            ("gc-keyspace-index-load-concurrency", load_concurrency),
            ("gc-state-cache-warmup-concurrency", warmup_concurrency),
        ] {
            if value == 0 {
                return Err(GCReadError::InvalidArgument {
                    argument: name,
                    value: value.to_string(),
                });
            }
        }
        let _reset = self.lifecycle.lock().await;
        let generation = self.generation().ok_or(GCReadError::NotLeader)?;
        self.cached(&generation, astersql_pd_keypath::NULL_KEYSPACE_ID)?;
        if generation.discovery.lock().unwrap().is_some() {
            return Err(GCReadError::InvalidArgument {
                argument: "metadata discovery",
                value: "already started in this generation".into(),
            });
        }
        let index = Arc::new(EnabledKeyspaceIndex::new(EnabledSnapshotLoader {
            storage: self.keyspaces.metadata_storage(),
            concurrency: load_concurrency,
        }));
        let warmup = Arc::new(GCWarmup::new(
            self.clone(),
            Some(index.clone()),
            warmup_concurrency,
        )?);
        let pages = warmup.clone();
        let initial = warmup.clone();
        let hooks = EnabledIndexHooks {
            on_page: Some(Arc::new(move |page| pages.on_page(page))),
            on_initial_snapshot: Some(Arc::new(move |snapshot| {
                initial.on_initial_snapshot(snapshot)
            })),
        };
        // Retirement takes this same claim lock before closing the tracker. No task can register late.
        let _claims = generation.flights.lock().unwrap();
        if generation.cancel.is_cancelled()
            || self.shared.active.load(Ordering::Acquire) != generation.id
        {
            return Err(GCReadError::NotLeader);
        }
        *generation.discovery.lock().unwrap() = Some(index.clone());
        let owner = generation.clone();
        let run_index = index.clone();
        let token = generation.cancel.child_token();
        generation.work.spawn_on(
            async move {
                let mut workers = tokio::task::JoinSet::new();
                let index_cancel = token.clone();
                workers.spawn(async move {
                    run_index.run_with_hooks(&index_cancel, hooks).await;
                    (true, Ok::<_, GCReadError>(()))
                });
                let warmup_cancel = token.clone();
                workers.spawn(async move { (false, warmup.run(&warmup_cancel).await) });
                while let Some(result) = workers.join_next().await {
                    match result {
                        Ok((false, Ok(()))) => {}
                        Ok((true, Ok(()))) => token.cancel(),
                        Ok((_, Err(_))) | Err(_) => {
                            owner.cancel.cancel();
                            token.cancel();
                        }
                    }
                }
            },
            &self.host,
        );
        Ok(index)
    }
    /// Current-generation index only; retained old handles stop serving snapshots after retirement.
    pub fn enabled_keyspace_index(&self) -> Option<Arc<EnabledKeyspaceIndex>> {
        let generation = self.generation()?;
        if generation.cancel.is_cancelled() {
            return None;
        }
        generation.discovery.lock().unwrap().clone()
    }
}
