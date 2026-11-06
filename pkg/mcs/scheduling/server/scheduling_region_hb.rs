// Copyright 2026 AsterSQL.
use crate::Service;
use astersql_pd_core::NodeState;
use astersql_pd_proto::{pdpb, schedulingpb as p};
use astersql_pd_schedule_hbstream::stream_lifecycle::HeartbeatMessage;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tonic::Status;
impl Service {
    pub(crate) fn heartbeat_stream(
        &self,
        input: tonic::Streaming<p::RegionHeartbeatRequest>,
    ) -> tonic::codegen::BoxStream<p::RegionHeartbeatResponse> {
        let service = self.clone();
        let cancel = self.cancel.child_token();
        let guard = cancel.clone().drop_guard();
        let (send, receive) = tokio::sync::mpsc::channel(1024);
        Box::pin(futures_util::stream::try_unfold(
            (
                input,
                service,
                cancel,
                guard,
                send,
                receive,
                None::<Instant>,
                false,
            ),
            |(mut input, service, cancel, guard, send, mut receive, mut last_bind, terminal)| async move {
                if terminal {
                    return Ok(None);
                }
                loop {
                    let request = tokio::select! {
                        _=cancel.cancelled()=>return Err(Status::unavailable("server closed")),
                        message=receive.recv()=>{
                            let response=match message {
                                Some(HeartbeatMessage::Command(command))=>astersql_pd_server::schedule_wire::to_scheduling(&command).map_err(|e|Status::internal(e.to_string()))?,
                                Some(HeartbeatMessage::KeepAlive {cluster_id})=>astersql_pd_server::schedule_wire::keepalive_scheduling(cluster_id),
                                None=>return Ok(None),
                            };
                            return Ok(Some((response,(input,service,cancel,guard,send,receive,last_bind,false))));
                        }
                        request=input.message()=>request?,
                    };
                    // Go heartbeatServer.recv wraps io.EOF, so the handler's direct
                    // equality check returns this wire status instead of normal EOF.
                    let Some(r) = request else {
                        return Err(Status::unknown("EOF"));
                    };
                    let Some(cluster) = service.host.cluster() else {
                        let response = p::RegionHeartbeatResponse {
                            header: Some(service.unavailable_cluster()),
                            ..Default::default()
                        };
                        return Ok(Some((
                            response,
                            (
                                input, service, cancel, guard, send, receive, last_bind, true,
                            ),
                        )));
                    };
                    let domain = cluster.region_heartbeat_service().ok_or_else(|| {
                        Status::unimplemented("region heartbeat state service is not configured")
                    })?;
                    let id = r.leader.as_ref().map_or(0, |p| p.store_id);
                    match domain.region_store_state(id) {
                        None => {
                            return Err(Status::unknown(format!(
                                "invalid store ID {id}, not found"
                            )));
                        }
                        Some(NodeState::Removed) => continue,
                        _ => {}
                    }
                    let work = async {
                        if last_bind.is_none_or(|at| at.elapsed() > Duration::from_secs(60)) {
                            domain.bind_heartbeat(id,Arc::new(astersql_pd_server::heartbeat_stream_adapter::CommandStream {sender:send.clone(),cancellation:cancel.clone()})).await.map_err(|e|Status::unavailable(e.0))?;
                            last_bind = Some(Instant::now());
                        }
                        let region =
                            astersql_pd_grpcutil::core_region_adapter::region_from_heartbeat(
                                pdpb::RegionHeartbeatRequest {
                                    region: r.region,
                                    leader: r.leader,
                                    term: r.term,
                                    down_peers: r.down_peers,
                                    pending_peers: r.pending_peers,
                                    bytes_written: r.bytes_written,
                                    bytes_read: r.bytes_read,
                                    keys_written: r.keys_written,
                                    keys_read: r.keys_read,
                                    approximate_size: r.approximate_size,
                                    approximate_keys: r.approximate_keys,
                                    query_stats: r.query_stats,
                                    interval: r.interval,
                                    bucket_meta: r.bucket_meta,
                                    ..Default::default()
                                },
                                0,
                            );
                        if let Ok(region) = region {
                            let _ = domain.region_heartbeat(region).await;
                        }
                        Ok::<(), Status>(())
                    };
                    tokio::select! {biased;_=cancel.cancelled()=>return Err(Status::unavailable("server closed")),result=work=>result?}
                }
            },
        ))
    }
    pub(crate) fn buckets_stream(
        &self,
        input: tonic::Streaming<p::RegionBucketsRequest>,
    ) -> tonic::codegen::BoxStream<p::RegionBucketsResponse> {
        let service = self.clone();
        let cancel = self.cancel.child_token();
        let guard = cancel.clone().drop_guard();
        Box::pin(futures_util::stream::try_unfold(
            (input, service, cancel, guard, false),
            |(mut input, service, cancel, guard, terminal)| async move {
                if terminal {
                    return Ok(None);
                }
                loop {
                    let request = tokio::select! {biased;_=cancel.cancelled()=>return Err(Status::unavailable("server closed")),request=input.message()=>request?};
                    let Some(r) = request else { return Ok(None) };
                    let Some(cluster) = service.host.cluster() else {
                        let response = p::RegionBucketsResponse {
                            header: Some(service.unavailable_cluster()),
                        };
                        return Ok(Some((response, (input, service, cancel, guard, true))));
                    };
                    let Some(buckets) = r.buckets.filter(|b| !b.keys.is_empty()) else {
                        continue;
                    };
                    let work = async {
                        if let Some(view) = cluster.cluster_read_service() {
                            if let Ok(Some(region)) = view.region(buckets.region_id).await {
                                if let Some(leader) = region.leader() {
                                    if view
                                        .store(leader.store_id)
                                        .await
                                        .ok()
                                        .flatten()
                                        .is_some_and(|store| store.is_removed())
                                    {
                                        return Ok(None);
                                    }
                                }
                            }
                        }
                        let domain = cluster.bucket_heartbeat_service().ok_or_else(|| {
                            Status::unimplemented(
                                "bucket heartbeat state service is not configured",
                            )
                        })?;
                        let _ = domain
                            .bucket_heartbeat(
                                astersql_pd_server::bucket_heartbeat_adapter::bucket_from_proto(
                                    buckets,
                                ),
                            )
                            .await;
                        Ok::<_, Status>(Some(p::RegionBucketsResponse {
                            header: Some(service.header()),
                        }))
                    };
                    let response = tokio::select! {biased;_=cancel.cancelled()=>return Err(Status::unavailable("server closed")),result=work=>result?};
                    if let Some(response) = response {
                        return Ok(Some((response, (input, service, cancel, guard, false))));
                    }
                }
            },
        ))
    }
}
