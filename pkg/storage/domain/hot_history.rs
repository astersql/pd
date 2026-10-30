// Copyright 2026 AsterSQL.
//! Hot-region history over the canonical backend-neutral metadata contract.
//! The host supplies a fenced store and owns sampling, retention and cancellation.
use crate::{Consistency, MetaStore, Mutation, Read, StoreError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Persisted and HTTP-visible plaintext history. Encrypted inputs fail closed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryHotRegion {
    pub update_time: i64,
    pub region_id: u64,
    pub peer_id: u64,
    pub store_id: u64,
    pub is_leader: bool,
    pub is_learner: bool,
    pub hot_region_type: String,
    pub hot_degree: i64,
    pub flow_bytes: f64,
    pub flow_cpu: f64,
    pub key_rate: f64,
    pub query_rate: f64,
    pub start_key: String,
    pub end_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption_meta: Option<serde_json::Value>,
}
/// Go history filters. Empty boolean sets intentionally select nothing.
#[derive(Clone, Debug, Default)]
pub struct HistoryQuery {
    pub start_time: i64,
    pub end_time: i64,
    pub region_ids: Vec<u64>,
    pub store_ids: Vec<u64>,
    pub peer_ids: Vec<u64>,
    pub is_learners: Vec<bool>,
    pub is_leaders: Vec<bool>,
    pub hot_region_types: Vec<String>,
}
impl HistoryQuery {
    fn matches(&self, r: &HistoryHotRegion) -> bool {
        (self.region_ids.is_empty() || self.region_ids.contains(&r.region_id))
            && (self.store_ids.is_empty() || self.store_ids.contains(&r.store_id))
            && (self.peer_ids.is_empty() || self.peer_ids.contains(&r.peer_id))
            && self.is_learners.contains(&r.is_learner)
            && self.is_leaders.contains(&r.is_leader)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("history region encryption key manager is not configured")]
    EncryptionUnavailable,
    #[error("json: unsupported value: non-finite hotspot load")]
    NonFinite,
}
/// Separate per-member namespace preserves Go's local historical database boundary.
#[derive(Clone)]
pub struct HotHistory {
    store: Arc<dyn MetaStore>,
    root: String,
    writes: Arc<tokio::sync::Mutex<()>>,
}
impl HotHistory {
    pub fn new(store: Arc<dyn MetaStore>, cluster_id: u64, member_id: u64) -> Self {
        Self {
            store,
            writes: Arc::new(tokio::sync::Mutex::new(())),
            root: format!("/pd/{cluster_id}/rust/hot-history/{member_id}"),
        }
    }
    fn key(&self, kind: &str, time: i64, region: u64) -> Vec<u8> {
        // path.Join cleans slashes and dot components in caller-supplied types.
        let raw = format!("schedule/hot_region/{kind}/{time:020}/{region:020}");
        let mut parts = Vec::new();
        for p in raw.split('/') {
            match p {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(p),
            }
        }
        format!("{}/rows/{}", self.root, parts.join("/")).into_bytes()
    }
    /// Go uses (type, timestamp, region), so the last peer for that key wins.
    pub fn encode(&self, rows: &[HistoryHotRegion]) -> Result<Vec<Mutation>, HistoryError> {
        let mut batch = std::collections::BTreeMap::new();
        for row in rows {
            if row.encryption_meta.is_some() {
                return Err(HistoryError::EncryptionUnavailable);
            }
            if [row.flow_bytes, row.flow_cpu, row.key_rate, row.query_rate]
                .iter()
                .any(|n| !n.is_finite())
            {
                return Err(HistoryError::NonFinite);
            }
            batch.insert(
                self.key(&row.hot_region_type, row.update_time, row.region_id),
                serde_json::to_vec(row)?,
            );
        }
        Ok(batch
            .into_iter()
            .map(|(key, value)| Mutation::Put {
                key,
                value,
                lease: None,
            })
            .collect())
    }
    fn published(&self, version: &str) -> Vec<u8> {
        format!("{}/published/{version}", self.root).into_bytes()
    }
    async fn read(
        &self,
        request: Read,
        cancel: &CancellationToken,
    ) -> Result<crate::Snapshot, HistoryError> {
        tokio::select! {biased;_=cancel.cancelled()=>Err(StoreError::Cancelled.into()),v=self.store.read(request)=>Ok(v?)}
    }
    async fn commit(
        &self,
        compares: Vec<crate::Compare>,
        writes: Vec<Mutation>,
        cancel: &CancellationToken,
    ) -> Result<crate::Commit, HistoryError> {
        let commit = tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled.into()),v=self.store.transact(compares,writes)=>v?};
        if !commit.succeeded {
            return Err(StoreError::Unavailable.into());
        }
        Ok(commit)
    }
    /// Stage bounded transactions, then atomically publish the complete flush.
    /// Failed staging is invisible; retrying a sample cannot expose partial rows.
    /// The host serializes writes and supplies a leadership-fenced store.
    pub async fn append(
        &self,
        rows: &[HistoryHotRegion],
        cancel: &CancellationToken,
    ) -> Result<(), HistoryError> {
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled.into()),v=self.writes.lock()=>v};
        let mut batch = self.encode(rows)?;
        if batch.is_empty() {
            return Ok(());
        }
        let sequence = format!("{}/sequence", self.root);
        let id = self
            .commit(
                vec![],
                vec![Mutation::put(sequence.as_bytes(), b"")],
                cancel,
            )
            .await?
            .revision;
        let version = format!("{id:020}");
        for mutation in &mut batch {
            let Mutation::Put { key, .. } = mutation else {
                unreachable!()
            };
            key.extend_from_slice(format!("/{version}").as_bytes());
        }
        for chunk in batch.chunks(64) {
            self.commit(vec![], chunk.to_vec(), cancel).await?;
        }
        self.commit(
            vec![],
            vec![Mutation::put(
                &self.published(&version),
                batch.len().to_string().as_bytes(),
            )],
            cancel,
        )
        .await?;
        Ok(())
    }
    /// Paged backend scan pinned to one revision, including publication markers.
    /// Public HTTP has no page/limit fields. Unpublished stages never become results.
    pub async fn query(
        &self,
        q: &HistoryQuery,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<HistoryHotRegion>>, HistoryError> {
        let types = if q.hot_region_types.is_empty() {
            vec!["read".into(), "write".into()]
        } else {
            q.hot_region_types.clone()
        };
        let mut rows = Vec::new();
        let mut revision = None;
        let mut publications = std::collections::HashMap::new();
        for kind in types {
            let start = self.key(&kind.to_lowercase(), q.start_time, 0);
            let end = self.key(&kind.to_lowercase(), q.end_time, u64::MAX);
            if start >= end {
                continue;
            }
            let mut request = Read::range(&start, &end, Consistency::Linearizable);
            request.limit = 64;
            request.revision = revision;
            let mut latest = std::collections::BTreeMap::new();
            loop {
                let page = self.read(request.clone(), cancel).await?;
                if revision.is_none() {
                    revision = Some(page.revision);
                }
                request.revision = revision;
                for entry in &page.entries {
                    let split = entry
                        .key
                        .iter()
                        .rposition(|b| *b == b'/')
                        .ok_or(StoreError::Backend)?;
                    let version = std::str::from_utf8(&entry.key[split + 1..])
                        .map_err(|_| StoreError::Backend)?;
                    let published = if let Some(value) = publications.get(version) {
                        *value
                    } else {
                        let mut read =
                            Read::range(&self.published(version), b"", Consistency::Linearizable);
                        read.revision = revision;
                        let value = !self.read(read, cancel).await?.entries.is_empty();
                        publications.insert(version.to_owned(), value);
                        value
                    };
                    if published {
                        latest.insert(entry.key[..split].to_vec(), entry.value.clone());
                    }
                }
                if !page.more {
                    break;
                }
                let Some(last) = page.entries.last() else {
                    return Err(StoreError::Backend.into());
                };
                let mut next = last.key.clone();
                next.push(0);
                request.start = Some(next);
            }
            for bytes in latest.values() {
                let row: HistoryHotRegion = serde_json::from_slice(bytes)?;
                if row.encryption_meta.is_some() {
                    return Err(HistoryError::EncryptionUnavailable);
                }
                if q.matches(&row) {
                    rows.push(row);
                }
            }
        }
        Ok((!rows.is_empty()).then_some(rows))
    }
    /// Delete expired versions in bounded transactions, releasing their markers
    /// with the final row. The host supplies the Go calendar-day cutoff.
    /// A clone shares the writer lock, so retention cannot delete an active stage.
    pub async fn purge_before(
        &self,
        cutoff: i64,
        cancel: &CancellationToken,
    ) -> Result<(), HistoryError> {
        let _guard = tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled.into()),v=self.writes.lock()=>v};
        for kind in ["read", "write"] {
            let start = self.key(kind, 0, 0);
            let end = self.key(kind, cutoff, i64::MAX as u64);
            if start >= end {
                continue;
            }
            loop {
                let mut read = Read::range(&start, &end, Consistency::Linearizable);
                read.limit = 64;
                let page = self.read(read, cancel).await?;
                if page.entries.is_empty() {
                    break;
                }
                let mut groups = std::collections::BTreeMap::<String, Vec<Mutation>>::new();
                for entry in page.entries {
                    let split = entry
                        .key
                        .iter()
                        .rposition(|b| *b == b'/')
                        .ok_or(StoreError::Backend)?;
                    let version = std::str::from_utf8(&entry.key[split + 1..])
                        .map_err(|_| StoreError::Backend)?
                        .to_owned();
                    groups
                        .entry(version)
                        .or_default()
                        .push(Mutation::Delete { key: entry.key });
                }
                for (version, mut mutations) in groups {
                    let key = self.published(&version);
                    let marker = self
                        .read(Read::range(&key, b"", Consistency::Linearizable), cancel)
                        .await?
                        .entries
                        .into_iter()
                        .next();
                    let mut compares = Vec::new();
                    if let Some(marker) = marker {
                        let remaining = std::str::from_utf8(&marker.value)
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok())
                            .and_then(|n| n.checked_sub(mutations.len()))
                            .ok_or(StoreError::Backend)?;
                        compares.push(crate::Compare::ModRevision(
                            key.clone(),
                            marker.mod_revision,
                        ));
                        mutations.push(if remaining == 0 {
                            Mutation::Delete { key }
                        } else {
                            Mutation::put(&key, remaining.to_string().as_bytes())
                        });
                    }
                    self.commit(compares, mutations, cancel).await?;
                }
            }
        }
        Ok(())
    }
}
