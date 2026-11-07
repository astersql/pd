// Copyright 2026 AsterSQL.
//! OpenRaft metadata adapter with SQLite quorum logs and a replayed MVCC view.

use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use openraft::{Config, Raft, RaftNetworkFactory, ServerState};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::candidate::Candidate;
use super::candidate_raft::{ReplayMachine, SqliteRaftLog};
use super::native_state::{Application, State};
pub use super::native_state::{NativeCommand, NativeReply};
use super::native_watch::NativeWatch;
use crate::*;

openraft::declare_raft_types!(pub NativeRaft: D = NativeCommand, R = NativeReply);

type Machine = ReplayMachine<NativeRaft, Application>;
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
const LEASE_TICK: Duration = Duration::from_millis(100);

/// One node of the native store. The host owns peer transport and membership.
pub struct NativeMetaStore {
    inner: Arc<Node>,
    lease_worker: JoinHandle<()>,
}

pub(super) struct Node {
    pub(super) raft: Raft<NativeRaft>,
    pub(super) machine: Machine,
    pub(super) cancelled: CancellationToken,
    cluster: u64,
}

impl NativeMetaStore {
    /// Open a node with the supplied cluster and node identities, using the caller's runtime.
    /// The supplied network must route only authenticated peers of this cluster.
    pub async fn open(
        directory: &Path,
        cluster: u64,
        node: u64,
        config: Arc<Config>,
        network: impl RaftNetworkFactory<NativeRaft>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let candidate = Candidate::open_native(directory, cluster, node)?;
        let (disk, machine) = Machine::open(SqliteRaftLog::new(candidate))?;
        let raft = Raft::new(node, config, network, disk, machine.clone()).await?;
        let cancelled = CancellationToken::new();
        let inner = Arc::new(Node {
            raft,
            machine,
            cancelled: cancelled.clone(),
            cluster,
        });
        let lease_worker = tokio::spawn(expire_leases(Arc::downgrade(&inner), cancelled));
        Ok(Self {
            inner,
            lease_worker,
        })
    }

    /// Access the consensus RPC and membership entry points for this node's host.
    pub fn consensus(&self) -> &Raft<NativeRaft> {
        &self.inner.raft
    }

    /// Capture a checksummed backup at a linearizable applied revision.
    pub async fn backup(
        &self,
    ) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        self.inner.barrier().await?;
        let image = self
            .inner
            .inspect(|state| Ok(state.backup(self.inner.cluster)))?;
        super::native_backup::encode(image)
    }

    /// Publish the current backup to a new file after synchronizing the archive.
    pub async fn backup_to(
        &self,
        path: &Path,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let bytes = self.backup().await?;
        super::native_backup::publish(&bytes, path)
    }

    /// Stop accepting requests, stop lease expiry, and join consensus shutdown.
    pub async fn close(
        mut self,
    ) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.inner.cancelled.cancel();
        let worker = (&mut self.lease_worker).await;
        self.inner.raft.shutdown().await?;
        worker?;
        Ok(())
    }
}

impl Drop for NativeMetaStore {
    fn drop(&mut self) {
        self.inner.cancelled.cancel();
    }
}

impl Node {
    async fn request<T, E>(
        &self,
        operation: impl Future<Output = std::result::Result<T, E>>,
    ) -> Result<T> {
        tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => Err(StoreError::Unavailable),
            reply = tokio::time::timeout(RPC_TIMEOUT, operation) =>
                reply.ok().and_then(std::result::Result::ok).ok_or(StoreError::Unavailable),
        }
    }

    async fn commit(&self, command: NativeCommand) -> Result<NativeReply> {
        Ok(self.request(self.raft.client_write(command)).await?.data)
    }

    pub(super) async fn barrier(&self) -> Result<()> {
        self.request(self.raft.ensure_linearizable())
            .await
            .map(|_| ())
    }

    pub(super) fn inspect<T>(&self, read: impl FnOnce(&State) -> Result<T>) -> Result<T> {
        if self.cancelled.is_cancelled() {
            return Err(StoreError::Unavailable);
        }
        self.machine
            .inspect(|app| read(app.state()))
            .map_err(|_| StoreError::Unavailable)?
    }
}

async fn expire_leases(node: Weak<Node>, cancelled: CancellationToken) {
    let mut tick = tokio::time::interval(LEASE_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut anchor = None;
    loop {
        tokio::select! {
            biased;
            _ = cancelled.cancelled() => return,
            _ = tick.tick() => {},
        }
        let Some(node) = node.upgrade() else { return };
        let term = {
            let rx = node.raft.metrics();
            let metrics = rx.borrow();
            (metrics.state == ServerState::Leader).then_some(metrics.current_term)
        };
        let clock = node.inspect(|state| Ok(state.clock())).ok().flatten();
        if let (Some(term), Some(clock)) = (term, clock) {
            let now = Instant::now();
            if let Some((old_term, previous)) = anchor.replace((term, now)) {
                if old_term == term {
                    let elapsed = now.duration_since(previous).min(LEASE_TICK).as_millis() as u64;
                    let _ = node
                        .commit(NativeCommand::AdvanceClock(clock.saturating_add(elapsed)))
                        .await;
                }
            }
        } else {
            anchor = None;
        }
    }
}

#[async_trait]
impl MetaStore for NativeMetaStore {
    async fn consensus_cluster_id(&self) -> Result<Option<u64>> {
        self.inner.inspect(|_| Ok(Some(self.inner.cluster)))
    }

    async fn consensus_leader_id(&self) -> Result<Option<u64>> {
        self.inner.barrier().await?;
        Ok(self.inner.raft.current_leader().await)
    }

    async fn read(&self, request: Read) -> Result<Snapshot> {
        request.validate()?;
        if request.consistency == Consistency::Linearizable {
            self.inner.barrier().await?;
        }
        self.inner.inspect(|state| state.read(&request))
    }

    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        self.transact_branches(compares, writes, Vec::new()).await
    }

    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> Result<Commit> {
        let reply = self
            .transact_raw(
                compares,
                then_writes.into_iter().map(Into::into).collect(),
                else_writes.into_iter().map(Into::into).collect(),
            )
            .await?;
        Ok(Commit {
            succeeded: reply.succeeded,
            revision: reply.revision,
        })
    }

    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        raw_get::validate_raw(&compares, &then_ops)?;
        raw_get::validate_raw(&compares, &else_ops)?;
        match self
            .inner
            .commit(NativeCommand::Transaction {
                compares,
                then_ops,
                else_ops,
            })
            .await?
        {
            NativeReply::Transaction(result) => result,
            _ => Err(StoreError::Backend),
        }
    }

    async fn watch(&self, prefix: Vec<u8>, start_revision: Revision) -> Result<Box<dyn MetaWatch>> {
        self.watch_with_previous(prefix, start_revision).await
    }

    async fn watch_with_previous(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> Result<Box<dyn MetaWatch>> {
        if start_revision <= 0 {
            return Err(StoreError::InvalidArgument);
        }
        Ok(Box::new(NativeWatch::new(
            Arc::downgrade(&self.inner),
            prefix,
            start_revision,
        )))
    }

    async fn grant(&self, ttl_seconds: i64) -> Result<Lease> {
        if !(1..=i32::MAX as i64).contains(&ttl_seconds) {
            return Err(StoreError::InvalidArgument);
        }
        match self.inner.commit(NativeCommand::Grant(ttl_seconds)).await? {
            NativeReply::Lease(result) => result,
            _ => Err(StoreError::Backend),
        }
    }

    async fn keep_alive(&self, id: LeaseId) -> Result<Lease> {
        match self.inner.commit(NativeCommand::Renew(id)).await? {
            NativeReply::Lease(result) => result,
            _ => Err(StoreError::Backend),
        }
    }

    async fn revoke(&self, id: LeaseId) -> Result<()> {
        match self.inner.commit(NativeCommand::Revoke(id)).await? {
            NativeReply::Administrative(result) => result,
            _ => Err(StoreError::Backend),
        }
    }

    async fn ttl(&self, id: LeaseId) -> Result<i64> {
        self.inner.barrier().await?;
        self.inner.inspect(|state| state.ttl(id))
    }

    async fn compact(&self, revision: Revision) -> Result<()> {
        match self.inner.commit(NativeCommand::Compact(revision)).await? {
            NativeReply::Administrative(result) => result,
            _ => Err(StoreError::Backend),
        }
    }
}
