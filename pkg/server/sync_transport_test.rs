// Copyright 2026 AsterSQL.
use super::sync_transport::*;
use astersql_pd_core::{BasicCluster, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_proto::pdpb;
use astersql_pd_syncer::HistoryBuffer;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

struct Host {
    identity: crate::Identity,
    syncer: RegionSyncer,
}
#[async_trait::async_trait]
impl crate::PdServices for Host {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    fn region_sync_service(&self) -> Option<&RegionSyncer> {
        Some(&self.syncer)
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("unused".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("unused".into())
    }
}
fn region(version: u64) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,

            epoch: Some(RegionEpoch {
                version,
                conf_ver: 1,
            }),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
async fn setup() -> (
    Arc<Host>,
    pdpb::pd_client::PdClient<tonic::transport::Channel>,
    CancellationToken,
    JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let host = Arc::new(Host {
        identity: crate::Identity {
            cluster_id: 42,
            member_id: 1,
            name: "source".into(),
            client_url: url.clone(),
            peer_url: String::new(),
        },
        syncer: RegionSyncer::new(
            Handle::current(),
            Arc::new(Mutex::new(BasicCluster::new())),
            HistoryBuffer::new(10000).unwrap(),
            Duration::from_secs(2),
        ),
    });
    host.syncer.publish(region(1)).unwrap();
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let router = crate::router(host.clone(), cancel.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    let client = pdpb::pd_client::PdClient::connect(url).await.unwrap();
    (host, client, cancel, task)
}
async fn subscribe(
    client: &mut pdpb::pd_client::PdClient<tonic::transport::Channel>,
    name: &str,
    start: u64,
) -> (
    mpsc::Sender<pdpb::SyncRegionRequest>,
    tonic::Streaming<pdpb::SyncRegionResponse>,
) {
    let (tx, rx) = mpsc::channel(1);
    tx.send(pdpb::SyncRegionRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 42,
            ..Default::default()
        }),
        member: Some(pdpb::Member {
            name: name.into(),
            ..Default::default()
        }),
        start_index: start,
    })
    .await
    .unwrap();
    let stream = client
        .sync_regions(ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    (tx, stream)
}
async fn next(stream: &mut tonic::Streaming<pdpb::SyncRegionResponse>) -> pdpb::SyncRegionResponse {
    tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
async fn shutdown(host: &Host, cancel: CancellationToken, server: JoinHandle<()>) {
    host.syncer.stop().await;
    assert!(host.syncer.downstream_names().is_empty());
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn state_task_51_sync_transport_full_and_incremental() {
    let (host, mut client, cancel, server) = setup().await;
    let (tx, mut stream) = subscribe(&mut client, "follower", 0).await;
    let first = next(&mut stream).await;
    let completion = if first.regions.is_empty() {
        first.clone()
    } else {
        next(&mut stream).await
    };
    drop(tx);
    drop(stream);
    shutdown(&host, cancel, server).await;
    assert_eq!(
        first.regions.len(),
        1,
        "full snapshot must contain canonical region"
    );
    assert_eq!(first.start_index, 0);
    assert_eq!(completion.start_index, 1);
}

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::{MemoryStore, Storage};
use astersql_pd_syncer::{ClientApply, HistoryIndex as I};
fn follower_state() -> SyncFollowerState {
    SyncFollowerState {
        cluster: BasicCluster::new(),
        history: HistoryBuffer::new(10000).unwrap(),
        apply: ClientApply::default(),
        storage: Storage::new(Arc::new(MemoryStore::default())),
        paths: ClusterPaths::new(42),
    }
}
async fn apply(state: &mut SyncFollowerState, response: pdpb::SyncRegionResponse) {
    let SyncFollowerState {
        cluster,
        history,
        apply,
        storage,
        paths,
    } = state;
    let outcome = apply
        .apply(
            crate::sync_codec::response_from_proto(response).into_apply(),
            cluster,
            storage,
            paths,
            history,
        )
        .await
        .unwrap();
    assert!(outcome.handled);
    assert_eq!(outcome.save_errors, 0);
}
async fn names(host: &Host, expected: &[&str]) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if host.syncer.downstream_names() == expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn state_task_51_sync_transport_independent_slow_reader_and_final_state() {
    let (host, mut client, cancel, server) = setup().await;
    let (slow_tx, mut slow) = subscribe(&mut client, "slow", 0).await;
    let (fast_tx, mut fast) = subscribe(&mut client, "fast", 1).await;
    let mut follower = follower_state();
    apply(&mut follower, next(&mut fast).await).await;
    for version in 2..=8 {
        let mut meta = region(version).meta().clone();
        meta.key_range.start_key = vec![7; 512 * 1024];
        host.syncer
            .publish(Arc::new(RegionInfo::new(meta, None, vec![], vec![])))
            .unwrap();
        let response = next(&mut fast).await;
        apply(&mut follower, response).await;
        assert_eq!(follower.history.next_index(), I(version));
    }
    names(&host, &["fast", "slow"]).await;
    // Several MiB exceed HTTP/2 flow control while the slow reader stays idle.
    assert_eq!(
        follower
            .cluster
            .regions
            .get_region(1)
            .unwrap()
            .meta()
            .epoch
            .unwrap()
            .version,
        8
    );
    let mut slow_follower = follower_state();
    while slow_follower.history.next_index() != I(8) || !slow_follower.apply.is_running() {
        apply(&mut slow_follower, next(&mut slow).await).await;
    }
    assert_eq!(
        slow_follower.cluster.regions.get_region(1).unwrap().meta(),
        follower.cluster.regions.get_region(1).unwrap().meta()
    );
    drop((slow_tx, slow, fast_tx, fast));
    names(&host, &[]).await;
    shutdown(&host, cancel, server).await;
}
#[tokio::test]
async fn state_task_51_sync_transport_replacement_and_reconnect() {
    let (host, mut client, cancel, server) = setup().await;
    let (old_tx, mut old) = subscribe(&mut client, "same", 0).await;
    let mut state = follower_state();
    apply(&mut state, next(&mut old).await).await;
    apply(&mut state, next(&mut old).await).await;
    let (new_tx, mut new) = subscribe(&mut client, "same", state.history.next_index().0).await;
    apply(&mut state, next(&mut new).await).await;
    // Old registration must finish without deleting its successor.
    let old_end = tokio::time::timeout(Duration::from_secs(5), old.message())
        .await
        .unwrap();
    assert!(matches!(old_end, Ok(None) | Err(_)));
    names(&host, &["same"]).await;
    host.syncer.publish(region(2)).unwrap();
    apply(&mut state, next(&mut new).await).await;
    drop((old_tx, old, new_tx, new));
    names(&host, &[]).await;
    host.syncer.publish(region(3)).unwrap();
    let (tx, mut stream) = subscribe(&mut client, "same", state.history.next_index().0).await;
    state.apply.reset_stream();
    apply(&mut state, next(&mut stream).await).await;
    assert_eq!(state.history.next_index(), I(3));
    assert_eq!(
        state
            .cluster
            .regions
            .get_region(1)
            .unwrap()
            .meta()
            .epoch
            .unwrap()
            .version,
        3
    );
    drop((tx, stream));
    shutdown(&host, cancel, server).await;
}
#[tokio::test]
async fn state_task_51_sync_transport_send_failure_cancellation_and_join() {
    use astersql_pd_syncer::DownstreamSender;
    // Real bounded transport channel: dropped receiver returns an actual send error.
    let (tx, rx) = mpsc::channel(1);
    let ready = CancellationToken::new();
    ready.cancel();
    let mut sender = super::sync_transport::WireSender {
        tx,
        cluster_id: 42,
        timeout: Duration::from_millis(10),
        ready: ready.clone(),
    };
    drop(rx);
    assert!(
        sender
            .send(astersql_pd_syncer::SyncFrame::keepalive(I(9)))
            .await
            .unwrap_err()
            .contains("closed")
    );
    // A full channel models transport backpressure without mocking index results.
    let (tx, _rx) = mpsc::channel(1);
    tx.send(Ok(pdpb::SyncRegionResponse::default()))
        .await
        .unwrap();
    let downstreams = astersql_pd_syncer::Downstreams::new(HistoryBuffer::new(10000).unwrap());
    downstreams.append([region(1)]).unwrap();
    let mut worker = downstreams.bind(
        &Handle::current(),
        "blocked".into(),
        I(0),
        super::sync_transport::WireSender {
            tx,
            cluster_id: 42,
            timeout: Duration::from_millis(10),
            ready,
        },
    );
    downstreams.broadcast(false);
    assert!(worker.wait().await.unwrap().is_err());
    assert_eq!(worker.send_index(), I(0));
    assert!(downstreams.names().is_empty());
    drop(worker);
    let (host, mut client, cancel, server) = setup().await;
    let (tx, mut stream) = subscribe(&mut client, "cancel", 1).await;
    next(&mut stream).await;
    host.syncer.stop().await;
    names(&host, &[]).await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await
            .unwrap()
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    drop((tx, stream));
    shutdown(&host, cancel, server).await;
}
#[tokio::test]
async fn state_task_51_sync_transport_follower_leader_reconnect() {
    use astersql_pd_syncer::{Downstreams, SyncLifecycle, SyncMetrics};
    let (host1, client1, cancel1, server1) = setup().await;
    let (host2, client2, cancel2, server2) = setup().await;
    host2.syncer.publish(region(2)).unwrap();
    let state = Arc::new(tokio::sync::Mutex::new(follower_state()));
    let request = crate::sync_codec::SyncRequest {
        header: Some(crate::sync_codec::SyncRequestHeader {
            cluster_id: 42,
            ..Default::default()
        }),
        member: Some(crate::sync_codec::SyncMember {
            name: "follower".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    for (version, host) in [(1, &host1), (2, &host2)] {
        let follower = SyncFollower {
            endpoint: host.identity.client_url.clone(),
            request: request.clone(),
            state: state.clone(),
        };
        let metrics = SyncMetrics::register(&prometheus::Registry::new()).unwrap();
        let mut lifecycle = SyncLifecycle::new(
            Handle::current(),
            Downstreams::new(HistoryBuffer::new(10000).unwrap()),
            metrics,
        );
        lifecycle
            .start_client(follower, Duration::from_millis(10))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state = state.lock().await;
                if state.apply.is_running() && state.history.next_index() == I(version) {
                    break;
                }
                drop(state);
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(lifecycle.stop().await.is_empty());
        names(host, &[]).await;
    }
    assert_eq!(
        state
            .lock()
            .await
            .cluster
            .regions
            .get_region(1)
            .unwrap()
            .meta()
            .epoch
            .unwrap()
            .version,
        2
    );
    drop((client1, client2));
    shutdown(&host1, cancel1, server1).await;
    shutdown(&host2, cancel2, server2).await;
}

#[tokio::test]
async fn state_task_51_sync_transport_snapshot_batches_and_publish_during_scan() {
    use astersql_pd_core::KeyRange;
    let (host, mut client, cancel, server) = setup().await;
    let ranged = |id, version| {
        Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                epoch: Some(RegionEpoch {
                    version,
                    conf_ver: 1,
                }),
                key_range: KeyRange {
                    start_key: if id == 1 {
                        vec![]
                    } else {
                        id.to_be_bytes().to_vec()
                    },
                    end_key: if id == 1001 {
                        vec![]
                    } else {
                        (id + 1).to_be_bytes().to_vec()
                    },
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        ))
    };
    for id in 1..=1001 {
        host.syncer.publish(ranged(id, 1)).unwrap();
    }
    let (tx, mut stream) = subscribe(&mut client, "scan", 0).await;
    let mut state = follower_state();
    let first = next(&mut stream).await;
    assert_eq!(first.regions.len(), 1000);
    apply(&mut state, first).await;
    assert!(!state.apply.is_running());
    host.syncer.publish(ranged(1001, 2)).unwrap();
    let second = next(&mut stream).await;
    assert_eq!(second.start_index, 1000);
    apply(&mut state, second).await;
    apply(&mut state, next(&mut stream).await).await;
    assert!(state.apply.is_running());
    apply(&mut state, next(&mut stream).await).await;
    assert_eq!(state.history.next_index(), I(1003));
    assert_eq!(state.cluster.regions.get_regions().len(), 1001);
    assert_eq!(
        state
            .cluster
            .regions
            .get_region(1001)
            .unwrap()
            .meta()
            .epoch
            .unwrap()
            .version,
        2
    );
    drop((tx, stream));
    shutdown(&host, cancel, server).await;
}
#[tokio::test]
async fn state_task_51_sync_transport_stop_before_first_request() {
    let (host, mut client, cancel, server) = setup().await;
    let (tx, rx) = mpsc::channel::<pdpb::SyncRegionRequest>(1);
    let pending = tokio::spawn(async move { client.sync_regions(ReceiverStream::new(rx)).await });
    tokio::task::yield_now().await;
    host.syncer.stop().await;
    let result = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap_err().code(), tonic::Code::Unavailable);
    drop(tx);
    shutdown(&host, cancel, server).await;
}

#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;
#[tokio::test]
async fn state_task_51_sync_transport_etcd_follower_persistence() {
    let fixture = etcd_fixture::Fixture::start();
    let (host, mut client, cancel, server) = setup().await;
    let (tx, mut stream) = subscribe(&mut client, "etcd", 0).await;
    let mut state = follower_state();
    state.storage = Storage::new(Arc::new(fixture.store().await));
    apply(&mut state, next(&mut stream).await).await;
    apply(&mut state, next(&mut stream).await).await;
    host.syncer.publish(region(2)).unwrap();
    apply(&mut state, next(&mut stream).await).await;
    assert_eq!(state.history.next_index(), I(2));
    assert!(state.apply.is_running());
    assert_eq!(
        state.storage.load_region(&state.paths, 1).await.unwrap(),
        Some(region(2).meta().clone())
    );
    assert_eq!(
        state.cluster.regions.get_region(1).unwrap().meta(),
        region(2).meta()
    );
    drop((tx, stream));
    shutdown(&host, cancel, server).await;
    drop(state);
    drop(fixture); // EOF closes and waits for the real etcd process.
}

struct TerminalPd(bool);
#[tonic::async_trait]
impl pdpb::pd_server::Pd for TerminalPd {
    async fn sync_regions(
        &self,
        _request: tonic::Request<tonic::Streaming<pdpb::SyncRegionRequest>>,
    ) -> Result<
        tonic::Response<
            std::pin::Pin<
                Box<
                    dyn tokio_stream::Stream<Item = Result<pdpb::SyncRegionResponse, tonic::Status>>
                        + Send,
                >,
            >,
        >,
        tonic::Status,
    > {
        // Only the transport error/EOF boundary is controlled. State application
        // below still uses real ClientApply, cluster, history and storage.
        let responses = if self.0 {
            vec![Ok(pdpb::SyncRegionResponse {
                header: Some(pdpb::ResponseHeader {
                    cluster_id: 42,
                    error: Some(pdpb::Error {
                        r#type: 0,
                        message: "leader unavailable".into(),
                    }),
                }),
                start_index: 999,
                ..Default::default()
            })]
        } else {
            vec![]
        };
        let output: std::pin::Pin<
            Box<
                dyn tokio_stream::Stream<Item = Result<pdpb::SyncRegionResponse, tonic::Status>>
                    + Send,
            >,
        > = if self.0 {
            Box::pin(futures_util::StreamExt::chain(
                tokio_stream::iter(responses),
                futures_util::stream::pending(),
            ))
        } else {
            Box::pin(tokio_stream::empty())
        };
        Ok(tonic::Response::new(output))
    }
}
#[tokio::test]
async fn state_task_51_sync_transport_follower_error_and_eof() {
    use astersql_pd_syncer::{SyncAttempt, SyncAttemptOutcome};
    for error in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let router = tonic::service::Routes::new(pdpb::pd_server::PdServer::new(TerminalPd(error)))
            .into_axum_router();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
                .unwrap();
        });
        let mut state = follower_state();
        state.history.reset_with_index(I(9));
        state.cluster.regions.put_region(region(1));
        let state = Arc::new(tokio::sync::Mutex::new(state));
        let mut follower = SyncFollower {
            endpoint,
            request: Default::default(),
            state: state.clone(),
        };
        let outcome = tokio::time::timeout(Duration::from_secs(5), follower.sync())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            (error, outcome),
            (true, SyncAttemptOutcome::Retry) | (false, SyncAttemptOutcome::Complete)
        ));
        let state = state.lock().await;
        assert_eq!(state.history.next_index(), I(9));
        assert_eq!(
            state.cluster.regions.get_region(1).unwrap().meta(),
            region(1).meta()
        );
        assert!(!state.apply.is_running());
        drop(state);
        cancel.cancel();
        server.await.unwrap();
    }
}
