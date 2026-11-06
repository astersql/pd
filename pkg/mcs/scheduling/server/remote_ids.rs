// Copyright 2026 AsterSQL.
//! PD-owned ID allocation. An uncertain RPC outcome is never replayed.
use astersql_pd_mcs_server::DelegateConnections;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{runtime::Handle, sync::Notify};
/// Host-selected PD discovery policy; no backend or deployment is inferred here.
pub type PdLeaderLookup = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Option<String>, String>> + Send>> + Send + Sync,
>;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Status};
pub struct RemoteIds {
    cluster_id: u64,
    target: RwLock<Option<String>>,
    refresh: Arc<Notify>,
    runtime: Handle,
    delegates: Arc<DelegateConnections>,
    cancel: CancellationToken,
}
impl RemoteIds {
    pub fn new(
        cluster_id: u64,
        delegates: Arc<DelegateConnections>,
        cancel: CancellationToken,
        refresh: Arc<Notify>,
        runtime: Handle,
    ) -> Self {
        Self {
            cluster_id,
            target: RwLock::new(None),
            delegates,
            cancel,
            refresh,
            runtime,
        }
    }
    /// The owner polls this on membership notifications or its minute timer.
    /// Failed discovery preserves the previously selected client, matching Go.
    pub async fn refresh(&self, lookup: &PdLeaderLookup) -> Result<(), String> {
        let work = async {
            let Some(target) = lookup().await? else {
                return Ok(());
            };
            self.delegates
                .get(&target)
                .await
                .map_err(|e| e.to_string())?;
            if !self.cancel.is_cancelled() {
                *self.target.write().unwrap() = Some(target);
            }
            Ok(())
        };
        tokio::select! {biased;_=self.cancel.cancelled()=>Ok(()),result=work=>result}
    }
    /// Canonical planners request IDs synchronously on the supplied blocking
    /// executor. Yield its runtime entry before polling the real async RPC.
    pub fn alloc_one_blocking(&self) -> Result<u64, Status> {
        tokio::task::block_in_place(|| self.runtime.block_on(self.alloc_count(1))).map(|r| r.0)
    }
    pub async fn alloc_count(&self, count: u32) -> Result<(u64, u32), Status> {
        let target = match self.target.read().unwrap().clone() {
            Some(target) => target,
            None => {
                self.refresh.notify_one();
                return Err(Status::unknown("PD leader is not found"));
            }
        };
        let work = async {
            let mut client = PdClient::new(self.delegates.get(&target).await?);
            let mut request = Request::new(pdpb::AllocIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: self.cluster_id,
                    ..Default::default()
                }),
                count,
            });
            request.set_timeout(Duration::from_secs(3));
            let response = client.alloc_id(request).await?.into_inner();
            Ok((response.id, response.count))
        };
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>Err(Status::cancelled("context canceled")),
            result=tokio::time::timeout(Duration::from_secs(3),work)=>result.unwrap_or_else(|_|Err(Status::deadline_exceeded("context deadline exceeded"))),
        };
        if result.is_err() {
            self.delegates.invalidate(&target);
            self.refresh.notify_one();
        }
        result
    }
    /// Go supports both a contiguous batch and older PDs returning one ID. The
    /// initial allocation is retained even when compatibility fallback is needed.
    pub async fn split_ids(
        &self,
        peers: usize,
        count: u32,
    ) -> Result<astersql_pd_server::grpc_ask_split::SplitIds, Status> {
        let requested = count.wrapping_mul(1_u32.wrapping_add(peers as u32));
        let (last, actual) = self.alloc_count(requested).await?;
        let mut ids = Vec::with_capacity(count as usize);
        if requested != actual {
            for _ in 0..count {
                let region = self.alloc_count(1).await?.0;
                let mut peer_ids = Vec::with_capacity(peers);
                for _ in 0..peers {
                    peer_ids.push(self.alloc_count(1).await?.0);
                }
                ids.push((region, peer_ids));
            }
        } else {
            let mut next = last.wrapping_sub(u64::from(requested)).wrapping_add(1);
            for _ in 0..count {
                let region = next;
                next = next.wrapping_add(1);
                let mut peer_ids = Vec::with_capacity(peers);
                for _ in 0..peers {
                    peer_ids.push(next);
                    next = next.wrapping_add(1);
                }
                ids.push((region, peer_ids));
            }
        }
        Ok(ids)
    }
}
