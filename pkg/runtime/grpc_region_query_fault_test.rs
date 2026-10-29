// Copyright 2026 AsterSQL.
use super::grpc_store_fault_test::{Host, RegistrationStore};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_025_pending_query_cancellation() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let backend = Arc::new(RegistrationStore {
        inner,
        _fixture: std::sync::Mutex::new(fixture),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = crate::NodeConfig {
        cluster_id: 2302,
        member_id: 1,
        name: "lookup-cancel".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                backend.clone(),
                config.clone(),
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let router = astersql_pd_server::router(
        Arc::new(Host {
            cluster: node.cluster(),
            identity: config,
        }),
        cancel.clone(),
    );
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let client = PdClient::connect(url).await.unwrap();
    // The adapter uses the real canonical heartbeat cache with buckets enabled.
    node.cluster()
        .register_store(astersql_pd_core::StoreMeta {
            id: 4,
            address: "store4".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let peer = astersql_pd_core::Peer {
        id: 8,
        store_id: 4,
        ..Default::default()
    };
    let mut region = astersql_pd_core::RegionInfo::new(
        astersql_pd_core::RegionMeta {
            id: 2,
            epoch: Some(astersql_pd_core::RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    assert!(region.compare_and_set_report_buckets(Some(Arc::new(
        astersql_pd_core::RegionBuckets {
            region_id: 2,
            version: 9,
            keys: vec![b"a".to_vec(), b"m".to_vec()],
            ..Default::default()
        }
    ))));
    use astersql_pd_server::region_heartbeat_adapter::RegionHeartbeatService;
    node.cluster().region_heartbeat(region).await.unwrap();
    let mut bucket_client = client.clone();
    for need_buckets in [false, true] {
        let h = Some(pdpb::RequestHeader {
            cluster_id: 2302,
            ..Default::default()
        });
        let mut stream = bucket_client
            .query_region(tokio_stream::iter([pdpb::QueryRegionRequest {
                header: h,
                ids: vec![2],
                need_buckets,
                ..Default::default()
            }]))
            .await
            .unwrap()
            .into_inner();
        let response = stream.message().await.unwrap().unwrap();
        let region = &response.regions_by_id[&2];
        assert_eq!(region.buckets.is_some(), need_buckets);
        if let Some(b) = &region.buckets {
            assert_eq!(b.version, 9);
            assert_eq!(b.region_id, 2);
        }
        assert!(stream.message().await.unwrap().is_none());
    }

    backend.mode.store(2, Ordering::SeqCst);
    let mut writer = client.clone();
    let write = tokio::spawn(async move {
        writer
            .put_store(pdpb::PutStoreRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 2302,
                    ..Default::default()
                }),
                store: Some(metapb::Store {
                    id: 5,
                    address: "store5".into(),
                    ..Default::default()
                }),
            })
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    let mut reads = vec![];
    for _ in 0..2 {
        let mut client = client.clone();
        reads.push(tokio::spawn(async move {
            let mut stream = client
                .query_region(tokio_stream::iter([pdpb::QueryRegionRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: 2302,
                        ..Default::default()
                    }),
                    ids: vec![2],
                    ..Default::default()
                }]))
                .await?
                .into_inner();
            stream.message().await.map(|_| ())
        }));
    }
    // The held real registration transaction owns the canonical resource mutex.
    tokio::task::yield_now().await;
    for r in &reads {
        assert!(!r.is_finished());
    }
    cancel.cancel();
    for r in reads {
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(5), r)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err()
                .code(),
            tonic::Code::Unavailable
        );
    }
    assert_eq!(
        write.await.unwrap().unwrap_err().code(),
        tonic::Code::Unavailable
    );
    assert!(
        backend
            .read(Read::prefix(
                b"/pd/2302/raft/s/00000000000000000005",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        node.cluster()
            .registered_stores()
            .await
            .unwrap()
            .iter()
            .all(|s| s.id != 5)
    );
    stop.cancel();
    serving.await.unwrap();
    node.shutdown().await.unwrap();
}
