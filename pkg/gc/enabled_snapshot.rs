// Copyright 2026 AsterSQL.
use astersql_pd_storage_domain::{
    Consistency, KeyValuePair, KeyspaceStorage, RawTxn, RawTxnOp, Read, StoreError,
};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnabledKeyspace {
    pub id: u32,
    pub gc_management_type: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnabledSnapshot {
    pub entries: BTreeMap<u32, EnabledKeyspace>,
    pub revision: i64,
}
#[derive(Debug, thiserror::Error)]
pub enum EnabledSnapshotError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Invalid(String),
}
/// Loads one private revision-pinned attempt using host-provided storage and codec.
#[derive(Clone)]
pub struct EnabledSnapshotLoader {
    pub storage: KeyspaceStorage,
    pub concurrency: usize,
}
impl EnabledSnapshotLoader {
    pub async fn load(
        &self,
        cancel: &CancellationToken,
    ) -> Result<EnabledSnapshot, EnabledSnapshotError> {
        self.load_with_pages(cancel, Arc::new(|_| {})).await
    }
    /// The page callback owns validated values; a failed attempt never returns a partial snapshot.
    pub async fn load_with_pages(
        &self,
        cancel: &CancellationToken,
        on_page: Arc<dyn Fn(Vec<EnabledKeyspace>) + Send + Sync>,
    ) -> Result<EnabledSnapshot, EnabledSnapshotError> {
        if self.concurrency == 0 {
            return Err(EnabledSnapshotError::Invalid(
                "keyspace load concurrency must be positive".into(),
            ));
        }
        let prefix = self.storage.paths.keyspace_meta_prefix();
        let end = prefix_end(prefix.as_bytes());
        let response = request(
            cancel,
            RawTxn::new(self.storage.store.as_ref())
                .then_ops(vec![
                    RawTxnOp::GetRange {
                        key: prefix.as_bytes().to_vec(),
                        end: end.clone(),
                        limit: 256,
                    },
                    RawTxnOp::Get(self.storage.paths.keyspace_alloc_id_path().into_bytes()),
                ])
                .commit(),
        )
        .await?;
        if response.revision <= 0 || response.responses.len() != 2 {
            return Err(EnabledSnapshotError::Invalid(
                "invalid initial keyspace metadata transaction response".into(),
            ));
        }
        let first = &response.responses[0];
        let revision = response.revision;
        let (page, last) = self.decode_page(&first.key_value_pairs, first.more, revision)?;
        let entries = Arc::new(Mutex::new(BTreeMap::new()));
        consume(&entries, page, &on_page);
        if cancel.is_cancelled() {
            return Err(StoreError::Cancelled.into());
        }
        if !first.more {
            return Ok(EnabledSnapshot {
                entries: Arc::try_unwrap(entries).unwrap().into_inner().unwrap(),
                revision,
            });
        }
        let watermark = response.responses[1]
            .key_value_pairs
            .first()
            .filter(|_| response.responses[1].key_value_pairs.len() == 1)
            .and_then(|kv| kv.value.as_slice().try_into().ok())
            .map(u64::from_be_bytes)
            .filter(|v| *v <= 0xffffff)
            .unwrap_or(0);
        let lo = u64::from(last) + 1;
        let span = watermark.saturating_sub(lo);
        let budget = (self.concurrency as u64).saturating_mul(32);
        let size = 4096.max(span.div_ceil(budget));
        let attempt = cancel.child_token();
        let mut cursor = first.key_value_pairs.last().unwrap().key.clone();
        cursor.push(0);
        let mut ranges = Vec::new();
        let mut boundary = lo + size;
        loop {
            let task_end = if boundary < watermark {
                format!("{prefix}{boundary:08}").into_bytes()
            } else {
                end.clone()
            };
            ranges.push((cursor, task_end.clone()));
            if task_end == end {
                break;
            }
            cursor = task_end;
            boundary += size;
        }
        // Futures are scoped to this invocation. Dropping it cancels every in-flight request.
        let queue = Arc::new(Mutex::new(ranges.into_iter()));
        let mut workers = tokio::task::JoinSet::new();
        for _ in 0..self.concurrency.min(queue.lock().unwrap().len()) {
            let loader = self.clone();
            let queue = queue.clone();
            let entries = entries.clone();
            let on_page = on_page.clone();
            let attempt = attempt.clone();
            workers.spawn(async move {
                loop {
                    let next = queue.lock().unwrap().next();
                    let Some((mut start, end)) = next else {
                        return Ok::<_, EnabledSnapshotError>(());
                    };
                    loop {
                        let mut read = Read::range(&start, &end, Consistency::Linearizable);
                        read.limit = 256;
                        read.revision = Some(revision);
                        let page = request(&attempt, loader.storage.store.read(read)).await?;
                        let pairs: Vec<_> = page
                            .entries
                            .into_iter()
                            .map(|e| KeyValuePair {
                                key: e.key,
                                value: e.value,
                            })
                            .collect();
                        let (values, _) = loader.decode_page(&pairs, page.more, revision)?;
                        if attempt.is_cancelled() {
                            return Err(StoreError::Cancelled.into());
                        }
                        consume(&entries, values, &on_page);
                        if !page.more {
                            break;
                        }
                        start = pairs.last().unwrap().key.clone();
                        start.push(0);
                    }
                }
            });
        }
        let mut failure = None;
        while let Some(result) = workers.join_next().await {
            let result = result.unwrap_or_else(|e| {
                Err(EnabledSnapshotError::Invalid(format!(
                    "keyspace load worker failed: {e}"
                )))
            });
            if let Err(error) = result {
                if failure.is_none() {
                    failure = Some(error);
                    attempt.cancel();
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        if cancel.is_cancelled() {
            return Err(StoreError::Cancelled.into());
        }
        Ok(EnabledSnapshot {
            entries: Arc::try_unwrap(entries).unwrap().into_inner().unwrap(),
            revision,
        })
    }
    pub(crate) fn decode_page(
        &self,
        pairs: &[KeyValuePair],
        more: bool,
        revision: i64,
    ) -> Result<(Vec<EnabledKeyspace>, u32), EnabledSnapshotError> {
        if more && pairs.is_empty() {
            return Err(EnabledSnapshotError::Invalid(format!(
                "empty keyspace metadata page at revision {revision}"
            )));
        }
        let prefix = self.storage.paths.keyspace_meta_prefix();
        let mut page = Vec::with_capacity(pairs.len());
        let mut last = 0;
        for pair in pairs {
            let raw = std::str::from_utf8(&pair.key)
                .map_err(|e| EnabledSnapshotError::Invalid(e.to_string()))?;
            let suffix = raw.strip_prefix(&prefix).ok_or_else(|| {
                EnabledSnapshotError::Invalid(format!(
                    "keyspace metadata key {raw:?} is outside prefix"
                ))
            })?;
            let id = suffix
                .strip_prefix('+')
                .unwrap_or(suffix)
                .parse::<u32>()
                .map_err(|e| {
                    EnabledSnapshotError::Invalid(format!(
                        "invalid keyspace metadata key {raw:?}: {e}"
                    ))
                })?;
            let meta = self.storage.decode_persisted(&pair.value).map_err(|e| {
                EnabledSnapshotError::Invalid(format!("decode keyspace metadata {raw:?}: {e}"))
            })?;
            if meta.id() != id {
                return Err(EnabledSnapshotError::Invalid(format!(
                    "keyspace metadata {raw:?} contains ID {}",
                    meta.id()
                )));
            }
            last = id;
            if meta.state == 0 {
                page.push(EnabledKeyspace {
                    id,
                    gc_management_type: meta
                        .config
                        .get("gc_management_type")
                        .cloned()
                        .unwrap_or_default(),
                });
            }
        }
        Ok((page, last))
    }
}
fn consume(
    entries: &Mutex<BTreeMap<u32, EnabledKeyspace>>,
    page: Vec<EnabledKeyspace>,
    callback: &Arc<dyn Fn(Vec<EnabledKeyspace>) + Send + Sync>,
) {
    {
        let mut entries = entries.lock().unwrap();
        for entry in &page {
            entries.insert(entry.id, entry.clone());
        }
    }
    callback(page);
}
async fn request<T>(
    cancel: &CancellationToken,
    f: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, EnabledSnapshotError> {
    tokio::select! {biased;_=cancel.cancelled()=>Err(StoreError::Cancelled.into()),r=tokio::time::timeout(Duration::from_secs(5),f)=>r.map_err(|_|StoreError::Unavailable)?.map_err(Into::into)}
}
pub(crate) fn prefix_end(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.last_mut() {
        if *last < 255 {
            *last += 1;
            return end;
        }
        end.pop();
    }
    vec![0]
}
