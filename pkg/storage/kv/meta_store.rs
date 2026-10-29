// Copyright 2026 AsterSQL.
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub type Revision = i64;
pub type LeaseId = i64;
pub type Result<T> = std::result::Result<T, StoreError>;

/// Stable error categories; transport and protobuf types stay in adapters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("metadata backend unavailable; mutation outcome may be unknown")]
    Unavailable,
    #[error("revision {requested} compacted (boundary {compact_revision:?}); resnapshot required")]
    Compacted {
        requested: Revision,
        compact_revision: Option<Revision>,
    },
    #[error("revision is newer than the store")]
    FutureRevision,
    #[error("lease does not exist")]
    LeaseNotFound,
    #[error("metadata request is too large")]
    RequestTooLarge,
    #[error("invalid metadata request")]
    InvalidArgument,
    #[error("permission denied")]
    PermissionDenied,
    #[error("unexpected metadata backend response")]
    Backend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consistency {
    Linearizable,
    Serializable,
}

/// Byte-ordered prefix or bounded read, optionally pinned to a revision. None means latest.
/// `start` is inclusive within the prefix; `limit == 0` is unlimited.
#[derive(Debug, Clone)]
pub struct Read {
    pub prefix: Vec<u8>,
    pub start: Option<Vec<u8>>,
    /// Exclusive upper bound for a bounded read; empty means a point read at start.
    /// Bounded reads require an empty prefix and a nonempty start.
    /// Use `Read::range` to translate Go Base's NUL prefix sentinel.
    pub end: Option<Vec<u8>>,
    pub limit: usize,
    pub revision: Option<Revision>,
    pub consistency: Consistency,
}
impl Read {
    /// Read Go Base's `[start, end)` selection in byte order.
    /// An end of exactly one NUL byte selects the start prefix (empty start: all keys).
    /// An empty end selects exactly start, matching Go's WithRange(empty) behavior.
    pub fn range(start: &[u8], end: &[u8], consistency: Consistency) -> Self {
        if end == [0] {
            return Self::prefix(start, consistency);
        }
        Self {
            start: Some(start.to_vec()),
            end: Some(end.to_vec()),
            ..Self::prefix(b"", consistency)
        }
    }

    pub fn prefix(prefix: &[u8], consistency: Consistency) -> Self {
        Self {
            prefix: prefix.to_vec(),
            start: None,
            end: None,
            limit: 0,
            revision: None,
            consistency,
        }
    }
    #[doc(hidden)]
    pub fn validate(&self) -> Result<()> {
        if self.revision.is_some_and(|r| r <= 0)
            || self
                .start
                .as_ref()
                .is_some_and(|s| !s.starts_with(&self.prefix))
            || (self.end.is_some()
                && (!self.prefix.is_empty() || self.start.as_ref().is_none_or(|s| s.is_empty())))
            || self.limit > i64::MAX as usize
        {
            return Err(StoreError::InvalidArgument);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub create_revision: Revision,
    pub mod_revision: Revision,
    pub lease: Option<LeaseId>,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Store header revision, including for historical reads (etcd semantics).
    pub revision: Revision,
    pub entries: Vec<Entry>,
    pub more: bool,
}
#[derive(Debug, Clone)]
pub enum Compare {
    CreateRevision(Vec<u8>, Revision),
    ModRevision(Vec<u8>, Revision),
    /// Value comparisons are false for missing keys, including empty operands.
    Value(Vec<u8>, Vec<u8>),
    ValueNotEqual(Vec<u8>, Vec<u8>),
    ValueLess(Vec<u8>, Vec<u8>),
    ValueGreater(Vec<u8>, Vec<u8>),
    Exists(Vec<u8>),
    NotExists(Vec<u8>),
    Lease(Vec<u8>, LeaseId),
}
#[derive(Debug, Clone)]
pub enum Mutation {
    Put {
        key: Vec<u8>,
        value: Vec<u8>,
        lease: Option<LeaseId>,
    },
    Delete {
        key: Vec<u8>,
    },
}
impl Mutation {
    pub fn put(key: &[u8], value: &[u8]) -> Self {
        Self::Put {
            key: key.to_vec(),
            value: value.to_vec(),
            lease: None,
        }
    }
    pub(crate) fn key(&self) -> &[u8] {
        match self {
            Self::Put { key, .. } | Self::Delete { key } => key,
        }
    }
}
pub(crate) fn validate_txn(compares: &[Compare], writes: &[Mutation]) -> Result<()> {
    if compares.len() + writes.len() > 128
        || writes.iter().enumerate().any(|(i, w)| {
            w.key().is_empty()
                || writes[..i].iter().any(|other| other.key() == w.key())
                || matches!(w, Mutation::Put { lease: Some(id), .. } if *id <= 0)
        })
        || compares.iter().any(|c| match c {
            Compare::CreateRevision(k, _)
            | Compare::ModRevision(k, _)
            | Compare::Value(k, _)
            | Compare::ValueNotEqual(k, _)
            | Compare::ValueLess(k, _)
            | Compare::ValueGreater(k, _)
            | Compare::Exists(k)
            | Compare::NotExists(k)
            | Compare::Lease(k, _) => k.is_empty(),
        })
    {
        return Err(StoreError::InvalidArgument);
    }
    Ok(())
}
#[derive(Debug, Clone, Copy)]
pub struct Commit {
    pub succeeded: bool,
    pub revision: Revision,
}
#[derive(Debug, Clone, Copy)]
pub struct Lease {
    pub id: LeaseId,
    pub ttl_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Put(Entry),
    Delete { key: Vec<u8>, revision: Revision },
}
impl Event {
    pub fn revision(&self) -> Revision {
        match self {
            Self::Put(e) => e.mod_revision,
            Self::Delete { revision, .. } => *revision,
        }
    }
    pub fn key(&self) -> &[u8] {
        match self {
            Self::Put(e) => &e.key,
            Self::Delete { key, .. } => key,
        }
    }
}
#[derive(Debug, Clone)]
pub struct WatchBatch {
    pub events: Vec<Event>,
    pub revision: Revision,
}

/// Pull-based watch: no detached worker. Drop closes resources; cancellation is terminal.
/// Events are ordered, inclusive from `start_revision`. Resume at batch.revision + 1.
/// Unavailable is retryable via next(); Compacted requires a fresh snapshot + watch.
#[async_trait]
pub trait MetaWatch: Send {
    /// Previous bytes for a delete in the most recently returned batch, if retained.
    /// Call before next(): advancing the watch may release this batch's history.
    fn previous_value(&self, _event: &Event) -> Option<Vec<u8>> {
        None
    }

    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch>;
}

/// Minimal PD metadata contract. Futures may be dropped to cancel requests.
/// Writes are atomic; compare failure is not an error. A selected write branch
/// consumes one revision if it changes state; empty branches consume none.
/// Never blindly retry Unavailable mutations: the commit may already have happened.
/// Lease-attached create-if-absent plus Value/Lease comparisons fences leadership.
#[async_trait]
pub trait MetaStore: Send + Sync {
    /// Backend consensus cluster identity for validating an explicit diagnostic binding.
    async fn consensus_cluster_id(&self) -> Result<Option<u64>> {
        Ok(None)
    }
    /// Current consensus leader identity, when exposed by this backend.
    /// The ID is backend-local and must never be assumed to identify a PD node.
    async fn consensus_leader_id(&self) -> Result<Option<u64>> {
        Ok(None)
    }

    async fn read(&self, request: Read) -> Result<Snapshot>;
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit>;
    /// Evaluate comparisons and commit exactly one write branch atomically.
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> Result<Commit>;
    /// Execute reads and writes in one backend transaction, preserving operation order.
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<crate::RawTxnOp>,
        else_ops: Vec<crate::RawTxnOp>,
    ) -> Result<crate::RawTxnResponse>;
    async fn watch(&self, prefix: Vec<u8>, start_revision: Revision) -> Result<Box<dyn MetaWatch>>;
    /// Opt in to retained delete values for the most recently delivered batch.
    /// Backends without separate watch options may retain history for every watch.
    async fn watch_with_previous(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> Result<Box<dyn MetaWatch>> {
        self.watch(prefix, start_revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> Result<Lease>;
    async fn keep_alive(&self, lease: LeaseId) -> Result<Lease>;
    async fn revoke(&self, lease: LeaseId) -> Result<()>;
    async fn ttl(&self, lease: LeaseId) -> Result<i64>;
    /// Administrative retention operation used by recovery/contract tests.
    async fn compact(&self, revision: Revision) -> Result<()>;
}
