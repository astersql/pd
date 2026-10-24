// Copyright 2026 AsterSQL.
use super::bucket_heartbeat_adapter::*;
use astersql_pd_core::*;
use astersql_pd_proto::{metapb, pdpb};
use astersql_pd_statistics_buckets::*;
use prost::Message;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;

struct Host {
    identity: crate::Identity,
    boot: AtomicBool,
    enabled: AtomicBool,
    block: AtomicBool,
    active: AtomicUsize,
    cluster: Mutex<BasicCluster>,
    worker: BucketWorkerClient,
    reports: Mutex<
        Vec<(
            BucketReport,
            Result<bool, astersql_pd_cluster::BucketRegionNotFound>,
        )>,
    >,
}
#[async_trait::async_trait]
impl BucketHeartbeatService for Host {
    fn is_bootstrapped(&self) -> bool {
        self.boot.load(Ordering::SeqCst)
    }
    async fn bucket_heartbeat(
        &self,
        report: BucketReport,
    ) -> Result<bool, astersql_pd_cluster::BucketRegionNotFound> {
        if self.block.load(Ordering::SeqCst) {
            struct Active<'a>(&'a AtomicUsize);
            impl Drop for Active<'_> {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
            self.active.fetch_add(1, Ordering::SeqCst);
            let _active = Active(&self.active);
            std::future::pending::<()>().await;
        }
        let result = astersql_pd_cluster::handle_bucket_heartbeat(
            &mut self.cluster.lock().unwrap(),
            Some(&self.worker),
            report.clone(),
        );
        self.reports.lock().unwrap().push((report, result));
        result
    }
}
#[async_trait::async_trait]
impl crate::PdServices for Host {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    fn bucket_heartbeat_service(&self) -> Option<&dyn BucketHeartbeatService> {
        self.enabled.load(Ordering::SeqCst).then_some(self)
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
fn request(version: u64) -> pdpb::ReportBucketsRequest {
    pdpb::ReportBucketsRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 42,
            ..Default::default()
        }),
        buckets: Some(metapb::Buckets {
            region_id: 1,
            version,
            keys: vec![vec![], b"m".to_vec(), vec![]],
            period_in_ms: 2000,
            stats: Some(metapb::BucketStats {
                read_bytes: vec![200000 * version, 400000 * version],
                read_keys: vec![4, 8],
                read_qps: vec![6, 12],
                write_bytes: vec![800000, 1600000],
                write_keys: vec![10, 20],
                write_qps: vec![12, 24],
            }),
        }),
        // The local Go route does not gate bucket versions with this epoch.
        region_epoch: Some(metapb::RegionEpoch {
            version: 999,
            conf_ver: 999,
        }),
    }
}
type Client = pdpb::pd_client::PdClient<tonic::transport::Channel>;
struct Fixture {
    host: Arc<Host>,
    client: Client,
    cancel: CancellationToken,
    server: tokio::task::JoinHandle<()>,
    worker: BucketWorker,
}
impl Fixture {
    async fn new() -> Self {
        let mut cluster = BasicCluster::new();
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 1,
            node_state: NodeState::Serving,
            ..Default::default()
        })));
        let peer = Peer {
            id: 2,
            store_id: 1,
            ..Default::default()
        };
        astersql_pd_cluster::handle_region_heartbeat(
            &mut cluster,
            RegionInfo::new(
                RegionMeta {
                    id: 1,
                    peers: vec![peer.clone()],
                    epoch: Some(RegionEpoch {
                        version: 2,
                        conf_ver: 2,
                    }),
                    ..Default::default()
                },
                Some(peer),
                vec![],
                vec![],
            ),
            true,
        )
        .unwrap();
        let cancel = CancellationToken::new();
        let worker = BucketWorker::start(
            &tokio::runtime::Handle::current(),
            &cancel,
            &prometheus::Registry::new(),
            BucketRangeIndex::new(),
        )
        .unwrap();
        let host = Arc::new(Host {
            identity: crate::Identity {
                cluster_id: 42,
                member_id: 1,
                name: "state49".into(),
                client_url: String::new(),
                peer_url: String::new(),
            },
            boot: AtomicBool::new(true),
            enabled: AtomicBool::new(true),
            block: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            cluster: Mutex::new(cluster),
            worker: worker.client(),
            reports: Mutex::new(vec![]),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = cancel.clone();
        let router = crate::router(host.clone(), cancel.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
                .unwrap();
        });
        let client = Client::connect(url).await.unwrap();
        Self {
            host,
            client,
            cancel,
            server,
            worker,
        }
    }
    async fn close(mut self) {
        drop(self.client);
        self.cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(3), self.server)
            .await
            .unwrap()
            .unwrap();
        self.worker.shutdown().await.unwrap();
    }
    async fn stats(&self) -> BucketStats {
        self.host
            .worker
            .collect(-20, vec![])
            .unwrap()
            .wait(&CancellationToken::new())
            .await
            .unwrap()
    }
    fn version(&self) -> u64 {
        self.host
            .cluster
            .lock()
            .unwrap()
            .regions
            .get_region(1)
            .unwrap()
            .report_buckets()
            .map_or(0, |b| b.version)
    }
}
async fn exchange(
    client: &mut Client,
    requests: Vec<pdpb::ReportBucketsRequest>,
) -> Result<pdpb::ReportBucketsResponse, tonic::Status> {
    client
        .report_buckets(tokio_stream::iter(requests))
        .await
        .map(|r| r.into_inner())
}
fn eof(result: Result<pdpb::ReportBucketsResponse, tonic::Status>) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert_eq!(error.message(), "EOF");
}
#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_wire_versions_and_loads() {
    let mut f = Fixture::new().await;
    let r = request(3);
    let wire = r.encode_to_vec();
    let decoded = pdpb::ReportBucketsRequest::decode(wire.as_slice()).unwrap();
    let outcome = exchange(&mut f.client, vec![decoded]).await;
    // Always retire listener and worker before the initial red assertion.
    let host = f.host.clone();
    f.close().await;
    eof(outcome);
    let reports = host.reports.lock().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, bucket_from_proto(r.buckets.unwrap()));
    assert_eq!(reports[0].1, Ok(true));
}

#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_applied_old_versions_missing_removed() {
    let mut f = Fixture::new().await;
    let mut missing = request(1);
    missing.buckets.as_mut().unwrap().region_id = 99;
    eof(exchange(&mut f.client, vec![missing]).await);
    assert!(f.stats().await.is_empty());
    for (incoming, expected, degree) in [(3, 3, 1), (2, 3, 2), (3, 3, 3), (4, 4, 4)] {
        eof(exchange(&mut f.client, vec![request(incoming)]).await);
        assert_eq!(f.version(), expected);
        let stats = f.stats().await;
        let buckets = &stats[&1];
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].loads, [100000 * incoming, 2, 3, 400000, 5, 6]);
        assert_eq!(buckets[1].loads, [200000 * incoming, 4, 6, 800000, 10, 12]);
        assert_eq!(buckets[0].interval, 2000);
        assert_eq!(buckets[0].hot_degree, degree);
        assert_eq!(buckets[0].start_key, b"");
        assert_eq!(buckets[0].end_key, b"m");
        assert_eq!(buckets[1].end_key, b"");
    }
    let before = f.stats().await;
    f.host
        .cluster
        .lock()
        .unwrap()
        .stores
        .patch_store(1, |store| {
            store.edit_meta(|m| {
                m.node_state = NodeState::Removed;
                m.state = StoreState::Tombstone;
            })
        });
    eof(exchange(&mut f.client, vec![request(9)]).await);
    assert_eq!(f.version(), 4);
    assert_eq!(f.stats().await, before);
    let reports = f.host.reports.lock().unwrap().clone();
    assert_eq!(
        reports.first().unwrap().1,
        Err(astersql_pd_cluster::BucketRegionNotFound(99))
    );
    assert_eq!(reports.last().unwrap().1, Ok(false));
    f.close().await;
}

#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_defaults_errors_and_empty() {
    let mut f = Fixture::new().await;
    let none = pdpb::ReportBucketsRequest {
        buckets: None,
        ..request(1)
    };
    let empty = pdpb::ReportBucketsRequest {
        buckets: Some(metapb::Buckets::default()),
        ..request(1)
    };
    eof(exchange(&mut f.client, vec![none, empty]).await);
    eof(exchange(&mut f.client, vec![]).await);
    assert!(f.host.reports.lock().unwrap().is_empty());
    assert!(f.stats().await.is_empty());
    let mut zero = request(1);
    zero.buckets.as_mut().unwrap().period_in_ms = 0;
    eof(exchange(&mut f.client, vec![zero]).await);
    assert_eq!(f.stats().await[&1][0].interval, 10000);
    assert_eq!(f.stats().await[&1][0].loads, [20000, 0, 0, 80000, 1, 1]);
    // A single key has zero buckets; absent stats remain absent-length vectors.
    let single = metapb::Buckets {
        region_id: 1,
        version: 2,
        keys: vec![b"m".to_vec()],
        ..Default::default()
    };
    assert_eq!(
        bucket_from_proto(single.clone()).flows,
        BucketFlows::default()
    );
    eof(exchange(
        &mut f.client,
        vec![pdpb::ReportBucketsRequest {
            buckets: Some(single),
            ..request(1)
        }],
    )
    .await);
    assert!(f.stats().await.is_empty());
    assert_eq!(f.version(), 2);
    let wrong = pdpb::ReportBucketsRequest {
        header: None,
        ..request(3)
    };
    assert_eq!(
        exchange(&mut f.client, vec![wrong])
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    f.host.boot.store(false, Ordering::SeqCst);
    let response = exchange(&mut f.client, vec![request(3)]).await.unwrap();
    let header = response.header.unwrap();
    assert_eq!(header.cluster_id, 42);
    assert_eq!(
        header.error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    f.host.enabled.store(false, Ordering::SeqCst);
    assert_eq!(
        exchange(&mut f.client, vec![request(3)])
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unimplemented
    );
    assert_eq!(f.version(), 2);
    f.close().await;
}

#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_competing_streams() {
    let f = Fixture::new().await;
    let mut first = f.client.clone();
    let mut second = f.client.clone();
    let (a, b) = tokio::join!(
        exchange(&mut first, vec![request(3)]),
        exchange(&mut second, vec![request(2)])
    );
    eof(a);
    eof(b);
    assert_eq!(f.version(), 3);
    let reports = f.host.reports.lock().unwrap().clone();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|r| r.1 == Ok(true)));
    // FIFO worker observes host lock order; verify actual final admitted payload.
    let last = reports.last().unwrap().0.version;
    assert_eq!(f.stats().await[&1][0].loads[0], 100000 * last);
    drop(first);
    drop(second);
    f.close().await;
}

async fn wait_active(host: &Host, value: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while host.active.load(Ordering::SeqCst) != value {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_cancel_and_disconnect() {
    for disconnect in [false, true] {
        let f = Fixture::new().await;
        f.host.block.store(true, Ordering::SeqCst);
        let mut client = f.client.clone();
        let call = tokio::spawn(async move { exchange(&mut client, vec![request(1)]).await });
        wait_active(&f.host, 1).await;
        if disconnect {
            call.abort();
            let _ = call.await;
        } else {
            f.cancel.cancel();
            let error = tokio::time::timeout(std::time::Duration::from_secs(3), call)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert_eq!(error.code(), tonic::Code::Cancelled);
        }
        wait_active(&f.host, 0).await;
        assert!(f.host.reports.lock().unwrap().is_empty());
        assert_eq!(f.version(), 0);
        f.close().await;
    }
}

#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_open_stream_has_no_ack() {
    let mut f = Fixture::new().await;
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(request(3)).await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        f.client
            .report_buckets(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(f.version(), 3);
    assert_eq!(f.stats().await[&1][0].loads[0], 300000);
    drop(tx);
    f.close().await;
}

#[tokio::test]
async fn state_task_49_bucket_heartbeat_adapter_same_stream_continues_unknown_store() {
    let mut f = Fixture::new().await;
    // The local route must not invent a reporting-store requirement: the region
    // can precede the store/leader in the canonical cache.
    {
        let mut cluster = f.host.cluster.lock().unwrap();
        let region = cluster.regions.get_region(1).unwrap();
        *cluster = BasicCluster::new();
        cluster.regions.put_region(region);
    }
    let mut missing = request(1);
    missing.buckets.as_mut().unwrap().region_id = 99;
    let none = pdpb::ReportBucketsRequest {
        buckets: None,
        ..request(1)
    };
    let empty = pdpb::ReportBucketsRequest {
        buckets: Some(metapb::Buckets::default()),
        ..request(1)
    };
    eof(exchange(
        &mut f.client,
        vec![missing, none, empty, request(3), request(2)],
    )
    .await);
    assert_eq!(f.version(), 3);
    let stats = f.stats().await;
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[&1][0].hot_degree, 2);
    assert_eq!(stats[&1][0].loads[0], 200000);
    assert_eq!(f.host.reports.lock().unwrap().len(), 3);
    f.close().await;
}
