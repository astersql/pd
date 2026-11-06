// Copyright 2026 AsterSQL.
//! Host-owned observation of a service primary, independent of wire encoding.
use astersql_pd_election::LeaderWatch;
use astersql_pd_storage_domain::{Consistency, Event, MetaStore, Read};
use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Cached primary survives backend outages until a successful snapshot or delete
/// proves it absent. The caller provides the service's canonical wire decoder.
pub struct PrimaryObserver {
    value: Arc<RwLock<Option<String>>>,
    cancel: CancellationToken,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl PrimaryObserver {
    pub fn start(
        runtime: &Handle,
        parent: &CancellationToken,
        store: Arc<dyn MetaStore>,
        key: Vec<u8>,
        decode: fn(&[u8]) -> Option<String>,
    ) -> Self {
        let cancel = parent.child_token();
        let value = Arc::new(RwLock::new(None));
        let task_value = value.clone();
        let task_cancel = cancel.clone();
        let worker = runtime.spawn(async move {
            observe(store, key, decode, task_value, task_cancel).await;
        });
        Self {
            value,
            cancel,
            worker: Mutex::new(Some(worker)),
        }
    }
    pub fn current(&self) -> Option<String> {
        self.value.read().unwrap().clone()
    }
    pub async fn shutdown(&self) {
        self.cancel.cancel();
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.await;
        }
    }
}
impl Drop for PrimaryObserver {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.get_mut().unwrap().take() {
            worker.abort();
        }
    }
}
async fn observe(
    store: Arc<dyn MetaStore>,
    key: Vec<u8>,
    decode: fn(&[u8]) -> Option<String>,
    value: Arc<RwLock<Option<String>>>,
    cancel: CancellationToken,
) {
    loop {
        let attempt = async {
            let snapshot = store
                .read(Read::range(&key, b"", Consistency::Linearizable))
                .await?;
            match snapshot.entries.first() {
                Some(row) => {
                    if let Some(target) = decode(&row.value) {
                        *value.write().unwrap() = Some(target);
                    }
                }
                None => *value.write().unwrap() = None,
            }
            let mut watch = LeaderWatch::new(
                store.clone(),
                key.clone(),
                snapshot.revision.saturating_add(1),
            )
            .map_err(|_| astersql_pd_storage_domain::StoreError::Backend)?;
            loop {
                match watch
                    .next(&cancel)
                    .await
                    .map_err(|_| astersql_pd_storage_domain::StoreError::Backend)?
                {
                    Event::Put(row) => {
                        if let Some(target) = decode(&row.value) {
                            *value.write().unwrap() = Some(target);
                        }
                    }
                    Event::Delete { .. } => {
                        *value.write().unwrap() = None;
                        break;
                    }
                }
            }
            Ok::<(), astersql_pd_storage_domain::StoreError>(())
        };
        tokio::select! { biased; _=cancel.cancelled()=>return, _=attempt=>{} }
        tokio::select! { biased; _=cancel.cancelled()=>return, _=tokio::time::sleep(Duration::from_millis(100))=>{} }
    }
}
