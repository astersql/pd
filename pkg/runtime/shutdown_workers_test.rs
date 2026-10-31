// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{
    Commit, Compare, Consistency, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp,
    RawTxnResponse, Read, Revision, Snapshot,
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_095_behavior() {
    scenario(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_095_host_cancel_and_interrupted_shutdown() {
    scenario(true).await;
}

async fn scenario(host_cancel: bool) {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 950).await;
    let registry = Arc::new(prometheus::Registry::new());
    let parent = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 950,
                member_id: 95,
                name: "shutdown".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();
    let syncer = host.region_sync_handle().unwrap();
    let mut client = PdClient::connect(format!("http://{address}"))
        .await
        .unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 950,
        ..Default::default()
    });
    let (tso_tx, tso_rx) = tokio::sync::mpsc::channel(1);
    tso_tx
        .send(pdpb::TsoRequest {
            header: header.clone(),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut tso = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(tso_rx))
        .await
        .unwrap()
        .into_inner();
    assert!(tso.message().await.unwrap().unwrap().timestamp.is_some());
    let (hb_tx, hb_rx) = tokio::sync::mpsc::channel::<pdpb::RegionHeartbeatRequest>(1);
    let mut heartbeat = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(hb_rx))
        .await
        .unwrap()
        .into_inner();
    let (sync_tx, sync_rx) = tokio::sync::mpsc::channel(1);
    sync_tx
        .send(pdpb::SyncRegionRequest {
            header,
            member: Some(pdpb::Member {
                name: "shutdown-peer".into(),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut sync = client
        .sync_regions(tokio_stream::wrappers::ReceiverStream::new(sync_rx))
        .await
        .unwrap()
        .into_inner();
    assert!(sync.message().await.unwrap().is_some());
    // The watch sends headers on its first event, so establish it with a durable write.
    let mut watch_client = client.clone();
    let watching = tokio::spawn(async move {
        watch_client
            .watch_global_config(pdpb::WatchGlobalConfigRequest {
                config_path: "/global/config/".into(),
                revision: 1,
            })
            .await
            .unwrap()
            .into_inner()
    });
    store
        .transact(
            vec![],
            vec![astersql_pd_storage::Mutation::put(
                b"/global/config/key",
                b"value",
            )],
        )
        .await
        .unwrap();
    let mut watch = watching.await.unwrap();
    assert!(watch.message().await.unwrap().is_some());
    let health = std::process::Command::new("curl")
        .args(["-fsS", &format!("http://{address}/pd/api/v1/health")])
        .output()
        .unwrap();
    assert!(health.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&health.stdout).unwrap()[0]["health"],
        true
    );
    if host_cancel {
        // Drop a pending shutdown wait; the owned worker must remain joinable.
        let unrelated = parent.child_token();
        parent.cancel();
        assert!(unrelated.is_cancelled());
        let mut closing = Box::pin(node.shutdown());
        std::future::poll_fn(|cx| {
            let _ = std::future::Future::poll(closing.as_mut(), cx);
            std::task::Poll::Ready(())
        })
        .await;
        drop(closing);
    }
    // Keep request producers and response streams alive throughout shutdown.
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .expect("shutdown must join with idle bidirectional clients alive")
        .unwrap();
    node.shutdown().await.unwrap();
    assert_eq!(parent.is_cancelled(), host_cancel);
    let saved = store
        .read(Read::prefix(b"/global/config/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(saved.entries.iter().any(|entry| entry.value == b"value"));
    assert!(host.snapshot().await.is_err());
    assert!(syncer.downstream_names().is_empty());
    assert!(registry.gather().is_empty());
    assert!(
        store
            .read(Read::range(
                b"/pd/950/leader",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    let rebound = tokio::net::TcpListener::bind(address).await.unwrap();
    drop(rebound);
    assert!(tso.message().await.unwrap().is_none());
    assert!(heartbeat.message().await.unwrap().is_none());
    assert!(watch.message().await.unwrap().is_none());
    assert_eq!(
        sync.message().await.unwrap_err().code(),
        tonic::Code::Unavailable
    );
    drop((
        tso_tx, hb_tx, sync_tx, tso, heartbeat, sync, watch, client, syncer,
    ));
    assert_eq!(Arc::strong_count(&store), 1);
}

// The process oracle has no host Future or Registry API. Compare the observable
// contract: live streams terminate and admission closes with clients still alive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_095_readonly_shutdown_differential() {
    use astersql_pd_compat_tests::harness::Server;
    use astersql_pd_proto::metapb;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = Server::start_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29995".into(),
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                region_epoch: Some(metapb::RegionEpoch {
                    version: 1,
                    conf_ver: 1,
                }),
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    let (tso_tx, tso_rx) = tokio::sync::mpsc::channel(1);
    tso_tx
        .send(pdpb::TsoRequest {
            header: header.clone(),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut tso = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(tso_rx))
        .await
        .unwrap()
        .into_inner();
    assert!(tso.message().await.unwrap().unwrap().timestamp.is_some());
    let (hb_tx, hb_rx) = tokio::sync::mpsc::channel::<pdpb::RegionHeartbeatRequest>(1);
    // grpc-go publishes response headers with its first message. A recoverable
    // invalid-region report establishes the stream before it becomes idle.
    hb_tx
        .send(pdpb::RegionHeartbeatRequest {
            header: header.clone(),
            region: Some(metapb::Region::default()),
            leader: Some(metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut heartbeat = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(hb_rx))
        .await
        .unwrap_or_else(|error| panic!("heartbeat admission: {error}; {}", go.logs()))
        .into_inner();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), heartbeat.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .header
            .unwrap()
            .error
            .is_some()
    );
    let (sync_tx, sync_rx) = tokio::sync::mpsc::channel(1);
    sync_tx
        .send(pdpb::SyncRegionRequest {
            header,
            member: Some(pdpb::Member {
                name: "shutdown-peer".into(),
                client_urls: vec!["http://127.0.0.1:29994".into()],
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut sync = client
        .sync_regions(tokio_stream::wrappers::ReceiverStream::new(sync_rx))
        .await
        .unwrap_or_else(|error| panic!("sync admission: {error}; {}", go.logs()))
        .into_inner();
    // Drain the full snapshot and its completion before initiating shutdown.
    loop {
        if tokio::time::timeout(Duration::from_secs(5), sync.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .regions
            .is_empty()
        {
            break;
        }
    }
    let backend = astersql_pd_storage_kv::EtcdMetaStore::connect(
        std::slice::from_ref(&go.url),
        Duration::from_secs(3),
    )
    .await
    .unwrap();
    let revision = backend
        .transact(
            vec![],
            vec![astersql_pd_storage::Mutation::put(
                b"/global/config/task095",
                b"value",
            )],
        )
        .await
        .unwrap()
        .revision;
    let mut watch = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: "/global/config/task095".into(),
            revision,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        watch.message().await.unwrap().unwrap().changes[0].payload,
        b"value"
    );
    assert_eq!(go.get("/pd/api/v1/health").unwrap().0, 200);
    go.stop().unwrap();
    async fn terminal<T: prost::Message + Default>(stream: &mut tonic::Streaming<T>) -> String {
        // A response queued before shutdown may still be readable after the
        // process exits. Require finite draining and termination, not an empty
        // transport buffer at the instant of shutdown.
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut buffered = 0;
            loop {
                let status = match stream.message().await {
                    Ok(None) => "EOF".into(),
                    Err(error) => format!("{:?}:{}", error.code(), error.message()),
                    Ok(Some(_)) => {
                        buffered += 1;
                        continue;
                    }
                };
                return format!("{status} (buffered={buffered})");
            }
        })
        .await
        .expect("Go shutdown must terminate live streams")
    }
    println!(
        "task095 Go shutdown terminals: tso={}, heartbeat={}, watch={}, sync={}",
        terminal(&mut tso).await,
        terminal(&mut heartbeat).await,
        terminal(&mut watch).await,
        terminal(&mut sync).await
    );
    let address = go.url.strip_prefix("http://").unwrap();
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    drop((
        tso_tx, hb_tx, sync_tx, tso, heartbeat, sync, watch, client, backend,
    ));
    scenario(false).await;
    println!(
        "task095 Rust shutdown terminals: tso=EOF, heartbeat=EOF, watch=EOF, sync=Unavailable; both close admission and all four streams with live clients"
    );
}

struct RetirementStore {
    inner: Arc<dyn MetaStore>,
    gate: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
}
#[async_trait::async_trait]
impl MetaStore for RetirementStore {
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> astersql_pd_storage::Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> astersql_pd_storage::Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> astersql_pd_storage::Result<Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> astersql_pd_storage::Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> astersql_pd_storage::Result<()> {
        if self.gate.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.acquire().await.unwrap().forget();
        }
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage::Result<()> {
        self.inner.compact(revision).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_095_stops_admission_before_lease_release() {
    let fixture = fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let store = Arc::new(RetirementStore {
        inner: inner.clone(),
        gate: false.into(),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Semaphore::new(0),
    });
    let registry = Arc::new(prometheus::Registry::new());
    let parent = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 951,
                member_id: 95,
                name: "retirement".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://peer".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    store.gate.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut closing = Box::pin(node.shutdown());
    tokio::select! {
        result = &mut closing => panic!("shutdown returned before the lease gate: {result:?}"),
        _ = store.entered.notified() => {}
    }
    // The real lease still exists; shutdown has not finished backend retirement.
    assert!(
        !inner
            .read(Read::range(
                b"/pd/951/leader",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if tokio::net::TcpStream::connect(address).await.is_err() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("listener must stop admission before backend retirement completes");
    // Cancelling a host wait must not detach the worker or release its ownership.
    drop(closing);
    store.release.add_permits(1);
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(
        inner
            .read(Read::range(
                b"/pd/951/leader",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(registry.gather().is_empty());
    assert!(!parent.is_cancelled());
}
