// Copyright 2026 AsterSQL.
//! Host-owned key watch/rotation task with resnapshot cursors and joined shutdown.
use crate::{DictionaryStorage, ENCRYPTION_KEYS_PATH, KeyRotation};
use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::*};
use astersql_pd_storage_domain::{Consistency, Event, Read, StoreError};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyWatchPhase {
    Snapshot,
    Watch,
    Compaction,
    Decode,
    Rotation,
}
#[derive(Clone, Debug)]
pub struct KeyWatchFailure {
    pub phase: KeyWatchPhase,
    pub message: String,
}
type Failures = Arc<Mutex<Option<KeyWatchFailure>>>;
fn record(failures: &Failures, phase: KeyWatchPhase, error: &TypedError) {
    *failures.lock().expect("key watch failure poisoned") = Some(KeyWatchFailure {
        phase,
        message: error.to_string(),
    });
}
fn watcher_error(error: impl std::error::Error + Send + Sync + 'static) -> TypedError {
    ERR_ENCRYPTION_KEYS_WATCHER.with_source([] as [String; 0], error)
}
async fn cancellable<T>(cancel: &CancellationToken, work: impl Future<Output = T>) -> Option<T> {
    tokio::select! {biased;_ =cancel.cancelled()=>None,result=work=>Some(result)}
}
/// A fresh point-read header anchors reconnect even if the current record is corrupt
/// or was last modified before compaction. Accepted dictionary revision is separate.
async fn refresh(
    storage: &DictionaryStorage,
    initial: bool,
    failures: &Failures,
) -> Result<i64, TypedError> {
    let snapshot = storage
        .backend()
        .read(Read::range(
            ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .map_err(|e| ERR_ETCD_KV_GET.with_source([] as [String; 0], e))?;
    let applied = if let Some(entry) = snapshot.entries.first() {
        storage.load_entry(entry).await
    } else {
        storage.load().await
    };
    match applied {
        Ok(loaded) if initial && loaded.keys.is_some() => Ok(loaded.revision.max(1)),
        Ok(_) => Ok(snapshot.revision.saturating_add(1).max(1)),
        Err(e) if initial => Err(e),
        Err(e) => {
            record(failures, KeyWatchPhase::Decode, &e);
            Ok(snapshot.revision.saturating_add(1).max(1))
        }
    }
}
pub struct KeyWatch {
    cancel: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
    failures: Failures,
}
impl KeyWatch {
    /// Load before readiness, then spawn only on the caller's runtime.
    pub async fn start(
        handle: &tokio::runtime::Handle,
        rotation: Arc<KeyRotation>,
        parent: &CancellationToken,
    ) -> Result<Self, TypedError> {
        let nanos = rotation.period_nanos();
        if nanos <= 0 {
            return Err(ErrorTemplate::new(
                "PD:encryption:ErrEncryptionInvalidConfig",
                "non-positive data key rotation period",
            )
            .instantiate([] as [String; 0]));
        }
        let period = Duration::from_nanos(nanos.min(600_000_000_000) as u64);
        let cancel = parent.child_token();
        let failures = Arc::new(Mutex::new(None));
        let storage = rotation.storage();
        let cursor = cancellable(&cancel, refresh(&storage, true, &failures))
            .await
            .ok_or_else(|| watcher_error(StoreError::Cancelled))??;
        if cancel.is_cancelled() {
            return Err(watcher_error(StoreError::Cancelled));
        }
        let task = handle.spawn(run(
            rotation,
            storage,
            cancel.clone(),
            failures.clone(),
            period,
            cursor,
        ));
        Ok(Self {
            cancel,
            task: Some(task),
            failures,
        })
    }
    pub fn last_failure(&self) -> Option<KeyWatchFailure> {
        self.failures
            .lock()
            .expect("key watch failure poisoned")
            .clone()
    }
    pub async fn shutdown(mut self) -> Result<(), TypedError> {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.await.map_err(watcher_error)?;
        }
        Ok(())
    }
}
impl Drop for KeyWatch {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
async fn run(
    rotation: Arc<KeyRotation>,
    storage: Arc<DictionaryStorage>,
    cancel: CancellationToken,
    failures: Failures,
    period: Duration,
    mut cursor: i64,
) {
    let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let Some(opened) = cancellable(
            &cancel,
            storage
                .backend()
                .watch(ENCRYPTION_KEYS_PATH.to_vec(), cursor),
        )
        .await
        else {
            return;
        };
        match opened {
            Err(error) => record_store_error(&failures, error),
            Ok(mut watch) => loop {
                let next = tokio::select! {biased;
                    _=cancel.cancelled()=>return,
                    result=watch.next(&cancel)=>Some(result),
                    _=ticker.tick()=>None,
                };
                match next {
                    None => {
                        let Some(result) = cancellable(&cancel, rotation.tick()).await else {
                            return;
                        };
                        if let Err(error) = result {
                            record(&failures, KeyWatchPhase::Rotation, &error);
                        }
                    }
                    Some(Err(error)) => {
                        record_store_error(&failures, error);
                        break;
                    }
                    Some(Ok(batch)) => {
                        for event in batch.events {
                            if event.key() != ENCRYPTION_KEYS_PATH {
                                continue;
                            }
                            match event {
                                Event::Put(entry) => {
                                    let Some(result) =
                                        cancellable(&cancel, storage.load_entry(&entry)).await
                                    else {
                                        return;
                                    };
                                    if let Err(error) = result {
                                        record(&failures, KeyWatchPhase::Decode, &error);
                                    }
                                }
                                Event::Delete { .. } => record(
                                    &failures,
                                    KeyWatchPhase::Decode,
                                    &ErrorTemplate::new(
                                        ERR_ENCRYPTION_LOAD_KEYS.code(),
                                        "encryption keys is deleted unexpectedly",
                                    )
                                    .instantiate([] as [String; 0]),
                                ),
                            }
                        }
                    }
                }
            },
        }
        // Avoid a hot loop on unavailable storage or a compacted/corrupt record.
        loop {
            if cancellable(&cancel, tokio::time::sleep(Duration::from_millis(100)))
                .await
                .is_none()
            {
                return;
            }
            let Some(result) = cancellable(&cancel, refresh(&storage, false, &failures)).await
            else {
                return;
            };
            match result {
                Ok(next) => {
                    cursor = next;
                    break;
                }
                Err(error) => record(&failures, KeyWatchPhase::Snapshot, &error),
            }
        }
    }
}
fn record_store_error(failures: &Failures, error: StoreError) {
    let phase = if matches!(error, StoreError::Compacted { .. }) {
        KeyWatchPhase::Compaction
    } else {
        KeyWatchPhase::Watch
    };
    record(failures, phase, &watcher_error(error));
}
