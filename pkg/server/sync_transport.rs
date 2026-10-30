// Copyright 2026 AsterSQL.
//! Host-owned SyncRegions sessions. Protocol conversion stays at this boundary.
use crate::{grpc::Grpc, sync_codec::*};
use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_proto::pdpb::{self, pd_server::Pd};
use astersql_pd_syncer::{Downstreams, HistoryBuffer, SyncFrame};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::Poll,
    time::Duration,
};
use tokio::{runtime::Handle, sync::mpsc, task::JoinHandle};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

type Output<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + 'static>>;

/// Shared canonical cluster and history. The host publishes accepted changes via
/// `publish`; it must not mutate the shared cluster outside that admission gate.
/// Call `stop` before releasing the host runtime. No private runtime is created.
pub struct RegionSyncer {
    runtime: Handle,
    cluster: Arc<dyn SyncSource>,
    downstreams: Downstreams,
    cancel: CancellationToken,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    timeout: Duration,
}
// A visitor keeps snapshot capture/binding inside the canonical admission gate.
trait SyncSource: Send + Sync {
    fn with_cluster(&self, visit: &mut dyn FnMut(&mut BasicCluster));
}
impl SyncSource for Mutex<BasicCluster> {
    fn with_cluster(&self, visit: &mut dyn FnMut(&mut BasicCluster)) {
        visit(&mut self.lock().unwrap());
    }
}
impl<C: Send> SyncSource for Mutex<astersql_pd_cluster::HeartbeatEffectsState<C>> {
    fn with_cluster(&self, visit: &mut dyn FnMut(&mut BasicCluster)) {
        visit(&mut self.lock().unwrap().cluster);
    }
}
impl RegionSyncer {
    pub fn new(
        runtime: Handle,
        cluster: Arc<Mutex<BasicCluster>>,
        history: HistoryBuffer,
        timeout: Duration,
    ) -> Self {
        Self {
            runtime,
            cluster,
            downstreams: Downstreams::new(history),
            cancel: CancellationToken::new(),
            tasks: Mutex::new(Vec::new()),
            timeout: if timeout.is_zero() {
                Duration::from_secs(10)
            } else {
                timeout
            },
        }
    }
    /// Attach to canonical heartbeat state. Use `downstreams()` in StatePipeline;
    /// its sync runner is the sole history publisher. The host must finish queued
    /// effects before requesting a quiescent full-state checkpoint.
    pub fn for_heartbeat_state<C: Send + 'static>(
        runtime: Handle,
        state: Arc<Mutex<astersql_pd_cluster::HeartbeatEffectsState<C>>>,
        history: HistoryBuffer,
        timeout: Duration,
    ) -> Self {
        Self {
            runtime,
            cluster: state,
            downstreams: Downstreams::new(history),
            cancel: CancellationToken::new(),
            tasks: Mutex::new(Vec::new()),
            timeout: if timeout.is_zero() {
                Duration::from_secs(10)
            } else {
                timeout
            },
        }
    }
    /// Shared history for the heartbeat pipeline; do not publish the same report twice.
    pub fn downstreams(&self) -> Downstreams {
        self.downstreams.clone()
    }
    /// Publish already-admitted domain changes without waiting for network I/O.
    pub fn publish(&self, region: Arc<RegionInfo>) -> Result<(), String> {
        let mut result = Ok(());
        self.cluster.with_cluster(&mut |cluster| {
            result = self
                .downstreams
                .append([region.clone()])
                .map_err(|e| e.to_string());
            if result.is_ok() {
                cluster.regions.put_region(region.clone());
                self.downstreams.broadcast(false);
            }
        });
        result
    }

    pub fn downstream_names(&self) -> Vec<String> {
        self.downstreams.names()
    }
    /// Cancel all streams, including those blocked on sends, then join every
    /// supervisor. Keeping pending handles in the owner preserves ownership if
    /// the caller temporarily drops the shutdown future.
    pub async fn stop(&self) {
        self.cancel.cancel();
        std::future::poll_fn(|cx| {
            let mut tasks = self.tasks.lock().unwrap();
            let mut index = 0;
            while index < tasks.len() {
                if Pin::new(&mut tasks[index]).poll(cx).is_ready() {
                    drop(tasks.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            if tasks.is_empty() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
    fn open(
        &self,
        request: SyncRequest,
        cluster_id: u64,
        input: tonic::Streaming<pdpb::SyncRegionRequest>,
        host_cancel: CancellationToken,
    ) -> Result<Output<pdpb::SyncRegionResponse>, Status> {
        let name = request.member.map(|m| m.name).unwrap_or_default();
        let (tx, rx) = mpsc::channel(1);
        let mut tasks = self.tasks.lock().unwrap();
        if self.cancel.is_cancelled() {
            return Err(Status::unavailable("region syncer stopped"));
        }
        let ready = CancellationToken::new();
        let mut sender = Some(WireSender {
            tx: tx.clone(),
            cluster_id,
            timeout: self.timeout,
            ready: ready.clone(),
        });
        let mut prepared = None;
        self.cluster.with_cluster(&mut |cluster| {
            prepared = Some(
                self.downstreams
                    .prepare_sync(cluster, request.start_index)
                    .map(|(initial, end)| {
                        let worker = self.downstreams.bind(
                            &self.runtime,
                            name.clone(),
                            end,
                            sender.take().unwrap(),
                        );
                        (initial, worker)
                    }),
            );
        });
        let (initial, mut worker) = prepared
            .unwrap()
            .map_err(|e| Status::internal(e.to_string()))?;
        // Registration precedes publication; the gate excludes an append between
        // capture and binding. Live sends wait until all initial frames finish.
        let cancel = self.cancel.child_token();
        let downstreams = self.downstreams.clone();
        let timeout = self.timeout;
        tasks.retain_mut(|task| {
            if task.is_finished() {
                let _ = futures_util::FutureExt::now_or_never(&mut *task);
                false
            } else {
                true
            }
        });
        let closed = CancellationToken::new();
        let session_closed = closed.clone();
        tasks.push(self.runtime.spawn(async move {
            let send_initial = async {
                for response in initial {
                    let response = match response {
                        astersql_pd_syncer::BootstrapFrame::Snapshot((start_index, records)) => {
                            catch_up_to_proto(
                                cluster_id,
                                astersql_pd_syncer::CatchUpFrame::Records {
                                    start_index,
                                    records: &records,
                                },
                            )
                        }
                        astersql_pd_syncer::BootstrapFrame::History(frame) => {
                            history_to_proto(cluster_id, &frame)
                        }
                    };
                    tokio::time::timeout(timeout, tx.send(Ok(response)))
                        .await
                        .map_err(|_| ())?
                        .map_err(|_| ())?;
                }
                Ok::<_, ()>(())
            };
            let mut finished = false;
            let initialized = tokio::select! { biased;
                _ = cancel.cancelled() => false,
                _ = host_cancel.cancelled() => false,
                _ = tx.closed() => false,
                _ = worker.wait() => { finished = true; false },
                result = send_initial => result.is_ok(),
            };
            if initialized {
                ready.cancel();
                downstreams.broadcast(false);
                tokio::select! { biased;
                    _ = cancel.cancelled() => {},
                    _ = host_cancel.cancelled() => {},
                    _ = tx.closed() => {},
                    _ = worker.wait() => { finished = true; },
                }
            }
            // Like Go, only the first request establishes this subscription.
            // Keep its receive half owned until the response closes; request EOF
            // must not terminate a follower's response stream.
            drop(input);
            worker.cancel();
            if !finished {
                let _ = worker.join().await;
            }
            session_closed.cancel();
        }));
        Ok(Box::pin(futures_util::stream::unfold(
            (rx, closed, false),
            |(mut rx, closed, terminal)| async move {
                if terminal {
                    return None;
                }
                let next = tokio::select! { biased;
                    _ = closed.cancelled() => Some(Err(Status::unavailable("region syncer stream closed"))),
                    next = rx.recv() => next,
                };
                let terminal = next.as_ref().is_some_and(Result::is_err);
                next.map(|value| (value, (rx, closed, terminal)))
            },
        )))
    }
}
impl Drop for RegionSyncer {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub(crate) struct WireSender {
    pub(crate) tx: mpsc::Sender<Result<pdpb::SyncRegionResponse, Status>>,
    pub(crate) cluster_id: u64,
    pub(crate) timeout: Duration,
    pub(crate) ready: CancellationToken,
}
impl astersql_pd_syncer::DownstreamSender for WireSender {
    async fn send(&mut self, frame: SyncFrame) -> Result<(), String> {
        self.ready.cancelled().await;
        tokio::time::timeout(
            self.timeout,
            self.tx.send(Ok(history_to_proto(self.cluster_id, &frame))),
        )
        .await
        .map_err(|_| "region syncer send timeout".to_owned())?
        .map_err(|_| "region syncer stream closed".into())
    }
}

/// Delegates the existing PD methods unchanged while extending the generated
/// service's optional SyncRegions method. grpc.rs remains read-only.
pub(crate) struct SyncGrpc(pub Grpc);
#[tonic::async_trait]
impl Pd for SyncGrpc {
    async fn is_snapshot_recovering(
        &self,
        r: Request<pdpb::IsSnapshotRecoveringRequest>,
    ) -> Result<Response<pdpb::IsSnapshotRecoveringResponse>, Status> {
        self.0.is_snapshot_recovering(r).await
    }

    async fn watch_gc_safe_point_v2(
        &self,
        r: Request<pdpb::WatchGcSafePointV2Request>,
    ) -> Result<Response<tonic::codegen::BoxStream<pdpb::WatchGcSafePointV2Response>>, Status> {
        self.0.watch_gc_safe_point_v2(r).await
    }
    async fn watch_gc_states(
        &self,
        r: Request<pdpb::WatchGcStatesRequest>,
    ) -> Result<Response<tonic::codegen::BoxStream<pdpb::WatchGcStatesResponse>>, Status> {
        self.0.watch_gc_states(r).await
    }

    async fn sync_max_ts(
        &self,
        r: Request<pdpb::SyncMaxTsRequest>,
    ) -> Result<Response<pdpb::SyncMaxTsResponse>, Status> {
        self.0.sync_max_ts(r).await
    }
    async fn get_dc_location_info(
        &self,
        r: Request<pdpb::GetDcLocationInfoRequest>,
    ) -> Result<Response<pdpb::GetDcLocationInfoResponse>, Status> {
        self.0.get_dc_location_info(r).await
    }

    async fn set_external_timestamp(
        &self,
        r: Request<pdpb::SetExternalTimestampRequest>,
    ) -> Result<Response<pdpb::SetExternalTimestampResponse>, Status> {
        self.0.set_external_timestamp(r).await
    }

    async fn get_external_timestamp(
        &self,
        r: Request<pdpb::GetExternalTimestampRequest>,
    ) -> Result<Response<pdpb::GetExternalTimestampResponse>, Status> {
        self.0.get_external_timestamp(r).await
    }

    async fn report_min_resolved_ts(
        &self,
        r: Request<pdpb::ReportMinResolvedTsRequest>,
    ) -> Result<Response<pdpb::ReportMinResolvedTsResponse>, Status> {
        self.0.report_min_resolved_ts(r).await
    }

    async fn watch_global_config(
        &self,
        r: Request<pdpb::WatchGlobalConfigRequest>,
    ) -> Result<Response<tonic::codegen::BoxStream<pdpb::WatchGlobalConfigResponse>>, Status> {
        self.0.watch_global_config(r).await
    }

    async fn store_global_config(
        &self,
        r: Request<pdpb::StoreGlobalConfigRequest>,
    ) -> Result<Response<pdpb::StoreGlobalConfigResponse>, Status> {
        self.0.store_global_config(r).await
    }

    async fn load_global_config(
        &self,
        r: Request<pdpb::LoadGlobalConfigRequest>,
    ) -> Result<Response<pdpb::LoadGlobalConfigResponse>, Status> {
        self.0.load_global_config(r).await
    }

    async fn split_and_scatter_regions(
        &self,
        r: Request<pdpb::SplitAndScatterRegionsRequest>,
    ) -> Result<Response<pdpb::SplitAndScatterRegionsResponse>, Status> {
        self.0.split_and_scatter_regions(r).await
    }

    async fn split_regions(
        &self,
        r: Request<pdpb::SplitRegionsRequest>,
    ) -> Result<Response<pdpb::SplitRegionsResponse>, Status> {
        self.0.split_regions(r).await
    }

    async fn put_store(
        &self,
        r: Request<pdpb::PutStoreRequest>,
    ) -> Result<Response<pdpb::PutStoreResponse>, Status> {
        self.0.put_store(r).await
    }
    async fn get_all_stores(
        &self,
        r: Request<pdpb::GetAllStoresRequest>,
    ) -> Result<Response<pdpb::GetAllStoresResponse>, Status> {
        self.0.get_all_stores(r).await
    }
    async fn get_store(
        &self,
        r: Request<pdpb::GetStoreRequest>,
    ) -> Result<Response<pdpb::GetStoreResponse>, Status> {
        self.0.get_store(r).await
    }
    async fn get_region(
        &self,
        r: Request<pdpb::GetRegionRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        self.0.get_region(r).await
    }
    async fn get_prev_region(
        &self,
        r: Request<pdpb::GetRegionRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        self.0.get_prev_region(r).await
    }
    async fn query_region(
        &self,
        r: Request<tonic::Streaming<pdpb::QueryRegionRequest>>,
    ) -> Result<Response<crate::grpc_region_query::QueryStream>, Status> {
        self.0.query_region(r).await
    }
    async fn scan_regions(
        &self,
        r: Request<pdpb::ScanRegionsRequest>,
    ) -> Result<Response<pdpb::ScanRegionsResponse>, Status> {
        self.0.scan_regions(r).await
    }
    async fn batch_scan_regions(
        &self,
        r: Request<pdpb::BatchScanRegionsRequest>,
    ) -> Result<Response<pdpb::BatchScanRegionsResponse>, Status> {
        self.0.batch_scan_regions(r).await
    }
    async fn get_region_by_id(
        &self,
        r: Request<pdpb::GetRegionByIdRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        self.0.get_region_by_id(r).await
    }

    async fn sync_regions(
        &self,
        request: Request<tonic::Streaming<pdpb::SyncRegionRequest>>,
    ) -> Result<Response<Output<pdpb::SyncRegionResponse>>, Status> {
        let owned = self.0.services.region_sync_handle();
        let service = owned
            .as_deref()
            .or_else(|| self.0.services.region_sync_service())
            .ok_or_else(|| Status::unimplemented("region sync service is not configured"))?;
        let mut input = request.into_inner();
        let request = tokio::select! { biased;
            _ = service.cancel.cancelled() => return Err(Status::unavailable("region syncer stopped")),
            _ = self.0.cancel.cancelled() => return Err(Status::unavailable("region syncer stopped")),
            request = input.message() => request?,
        };
        let Some(request) = request else {
            return Ok(Response::new(Box::pin(tokio_stream::empty())));
        };
        let cluster_id = self.0.services.identity().cluster_id;
        if request.header.as_ref().map(|h| h.cluster_id).unwrap_or(0) != cluster_id {
            return Err(Status::failed_precondition(format!(
                "mismatch cluster id, need {cluster_id} but got {}",
                request.header.as_ref().map(|h| h.cluster_id).unwrap_or(0)
            )));
        }
        self.0
            .services
            .healthy()
            .await
            .map_err(Status::unavailable)?;
        Ok(Response::new(service.open(
            request_from_proto(request),
            cluster_id,
            input,
            self.0.cancel.clone(),
        )?))
    }
    async fn report_buckets(
        &self,
        r: Request<tonic::Streaming<pdpb::ReportBucketsRequest>>,
    ) -> Result<Response<pdpb::ReportBucketsResponse>, Status> {
        self.0.report_buckets(r).await
    }
    async fn region_heartbeat(
        &self,
        r: Request<tonic::Streaming<pdpb::RegionHeartbeatRequest>>,
    ) -> Result<Response<Output<pdpb::RegionHeartbeatResponse>>, Status> {
        self.0.region_heartbeat(r).await
    }
    async fn store_heartbeat(
        &self,
        r: Request<pdpb::StoreHeartbeatRequest>,
    ) -> Result<Response<pdpb::StoreHeartbeatResponse>, Status> {
        self.0.store_heartbeat(r).await
    }
    async fn get_cluster_info(
        &self,
        r: Request<pdpb::GetClusterInfoRequest>,
    ) -> Result<Response<pdpb::GetClusterInfoResponse>, Status> {
        self.0.get_cluster_info(r).await
    }
    async fn scatter_region(
        &self,
        r: Request<pdpb::ScatterRegionRequest>,
    ) -> Result<Response<pdpb::ScatterRegionResponse>, Status> {
        self.0.scatter_region(r).await
    }
    async fn get_operator(
        &self,
        r: Request<pdpb::GetOperatorRequest>,
    ) -> Result<Response<pdpb::GetOperatorResponse>, Status> {
        self.0.get_operator(r).await
    }
    async fn get_cluster_config(
        &self,
        r: Request<pdpb::GetClusterConfigRequest>,
    ) -> Result<Response<pdpb::GetClusterConfigResponse>, Status> {
        self.0.get_cluster_config(r).await
    }
    async fn put_cluster_config(
        &self,
        r: Request<pdpb::PutClusterConfigRequest>,
    ) -> Result<Response<pdpb::PutClusterConfigResponse>, Status> {
        self.0.put_cluster_config(r).await
    }
    async fn bootstrap(
        &self,
        r: Request<pdpb::BootstrapRequest>,
    ) -> Result<Response<pdpb::BootstrapResponse>, Status> {
        self.0.bootstrap(r).await
    }
    async fn is_bootstrapped(
        &self,
        r: Request<pdpb::IsBootstrappedRequest>,
    ) -> Result<Response<pdpb::IsBootstrappedResponse>, Status> {
        self.0.is_bootstrapped(r).await
    }
    async fn get_members(
        &self,
        r: Request<pdpb::GetMembersRequest>,
    ) -> Result<Response<pdpb::GetMembersResponse>, Status> {
        self.0.get_members(r).await
    }
    async fn report_split(
        &self,
        r: Request<pdpb::ReportSplitRequest>,
    ) -> Result<Response<pdpb::ReportSplitResponse>, Status> {
        self.0.report_split(r).await
    }
    async fn report_batch_split(
        &self,
        r: Request<pdpb::ReportBatchSplitRequest>,
    ) -> Result<Response<pdpb::ReportBatchSplitResponse>, Status> {
        self.0.report_batch_split(r).await
    }
    async fn ask_split(
        &self,
        r: Request<pdpb::AskSplitRequest>,
    ) -> Result<Response<pdpb::AskSplitResponse>, Status> {
        self.0.ask_split(r).await
    }
    async fn ask_batch_split(
        &self,
        r: Request<pdpb::AskBatchSplitRequest>,
    ) -> Result<Response<pdpb::AskBatchSplitResponse>, Status> {
        self.0.ask_batch_split(r).await
    }
    async fn alloc_id(
        &self,
        r: Request<pdpb::AllocIdRequest>,
    ) -> Result<Response<pdpb::AllocIdResponse>, Status> {
        self.0.alloc_id(r).await
    }
    async fn get_min_ts(
        &self,
        request: Request<pdpb::GetMinTsRequest>,
    ) -> Result<Response<pdpb::GetMinTsResponse>, Status> {
        self.0.get_min_ts(request).await
    }

    async fn tso(
        &self,
        r: Request<tonic::Streaming<pdpb::TsoRequest>>,
    ) -> Result<Response<Output<pdpb::TsoResponse>>, Status> {
        self.0.tso(r).await
    }
}

/// Canonical follower state, shared with the host for readiness and snapshots.
pub struct SyncFollowerState {
    pub cluster: BasicCluster,
    pub history: HistoryBuffer,
    pub apply: astersql_pd_syncer::ClientApply,
    pub storage: astersql_pd_storage::Storage,
    pub paths: astersql_pd_keypath::ClusterPaths,
}
/// One real tonic connection attempt for SyncLifecycle's serialized retry loop.
/// Dropping sync cancels both request and response I/O. Leader replacement uses
/// a new attempt after the host cancels and joins the previous lifecycle.
pub struct SyncFollower {
    pub endpoint: String,
    pub request: SyncRequest,
    pub state: Arc<tokio::sync::Mutex<SyncFollowerState>>,
}
impl astersql_pd_syncer::SyncAttempt for SyncFollower {
    async fn sync(&mut self) -> Result<astersql_pd_syncer::SyncAttemptOutcome, String> {
        let mut client = pdpb::pd_client::PdClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| e.to_string())?;
        let mut request = self.request.clone();
        request.start_index = self.state.lock().await.history.next_index();
        let (tx, rx) = mpsc::channel(1);
        tx.send(request_to_proto(request))
            .await
            .map_err(|e| e.to_string())?;
        let mut stream = client
            .sync_regions(ReceiverStream::new(rx))
            .await
            .map_err(|e| e.to_string())?
            .into_inner();
        self.state.lock().await.apply.reset_stream();
        while let Some(response) = stream.message().await.map_err(|e| e.to_string())? {
            let mut state = self.state.lock().await;
            let SyncFollowerState {
                cluster,
                history,
                apply,
                storage,
                paths,
            } = &mut *state;
            let outcome = apply
                .apply(
                    response_from_proto(response).into_apply(),
                    cluster,
                    storage,
                    paths,
                    history,
                )
                .await
                .map_err(|e| e.to_string())?;
            if !outcome.handled {
                return Ok(astersql_pd_syncer::SyncAttemptOutcome::Retry);
            }
        }
        drop(tx);
        Ok(astersql_pd_syncer::SyncAttemptOutcome::Complete)
    }
}
