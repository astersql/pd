// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

use crate::etcd_fixture as fixture;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_018_behavior() {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    // An unreported restored region keeps the real scheduler's preparation gate
    // closed, so background ID reservations cannot obscure rejected-write effects.
    astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .save_region(
            &astersql_pd_keypath::ClusterPaths::new(1800),
            &astersql_pd_core::RegionMeta {
                id: 7,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut nodes = Vec::new();
    let mut clients = Vec::new();
    let mut registries = Vec::new();
    for member_id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let registry = Arc::new(prometheus::Registry::new());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 1800,
                        member_id,
                        name: format!("role-{member_id}"),
                        client_url: url.clone(),
                        peer_url: String::new(),
                    },
                    listener,
                    registry.clone(),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        let ping = std::process::Command::new("curl")
            .args([
                "-sS",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                &format!("{url}/pd/api/v1/ping"),
            ])
            .output()
            .unwrap();
        assert!(ping.status.success());
        assert_eq!(ping.stdout, b"200");
        clients.push(PdClient::connect(url).await.unwrap());
        registries.push(registry);
    }
    let header = Some(pdpb::RequestHeader {
        cluster_id: 1800,
        ..Default::default()
    });
    let first = clients[0]
        .alloc_id(pdpb::AllocIdRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    for (index, client) in clients.iter_mut().enumerate() {
        for request_header in [None, Some(pdpb::RequestHeader::default()), header.clone()] {
            if index == 0 && request_header == header {
                continue;
            }
            let error = client
                .alloc_id(pdpb::AllocIdRequest {
                    header: request_header.clone(),
                    ..Default::default()
                })
                .await
                .unwrap_err();
            let expected = if index == 0 {
                tonic::Code::FailedPrecondition
            } else {
                tonic::Code::Unavailable
            };
            assert_eq!(
                error.code(),
                expected,
                "role must be checked before cluster id"
            );
            if index == 1 {
                assert_eq!(error.message(), "not leader");
            }
            let mut stream = client
                .tso(tokio_stream::iter([pdpb::TsoRequest {
                    header: request_header.clone(),
                    count: 1,
                    ..Default::default()
                }]))
                .await
                .unwrap()
                .into_inner();
            // TSO separately validates identity before asking its lease-backed allocator.
            let tso_expected = if request_header == header {
                tonic::Code::Unavailable
            } else {
                tonic::Code::FailedPrecondition
            };
            assert_eq!(stream.message().await.unwrap_err().code(), tso_expected);
        }
    }
    let next = clients[0]
        .alloc_id(pdpb::AllocIdRequest {
            header,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    assert_eq!(next, first + 1, "rejected writes must not consume IDs");
    // An open input stream must not prevent host cancellation and joined shutdown.
    let (_sender, receiver) = tokio::sync::mpsc::channel(1);
    let mut stream = clients[0]
        .tso(tokio_stream::wrappers::ReceiverStream::new(receiver))
        .await
        .unwrap()
        .into_inner();
    for node in &mut nodes {
        tokio::time::timeout(Duration::from_secs(10), node.shutdown())
            .await
            .unwrap()
            .unwrap();
    }
    assert!(stream.message().await.unwrap().is_none());
    for registry in registries {
        assert!(registry.gather().is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_018_bootstrap_and_state() {
    use astersql_pd_core::{BasicCluster, NodeState, StoreInfo, StoreMeta};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_endpoint::{Cluster, StorageEndpoint};
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let storage = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(1801);
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    let observed = cluster.stores.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_cluster(cluster)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 1801,
                member_id: 1,
                name: "bootstrap".into(),
                client_url: url.clone(),
                peer_url: String::new(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 1801,
        ..Default::default()
    });
    // Unary admission checks role and identity before returning a bootstrap header.
    assert_eq!(
        client
            .store_heartbeat(pdpb::StoreHeartbeatRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    let request = pdpb::StoreHeartbeatRequest {
        header: header.clone(),
        stats: Some(pdpb::StoreStats {
            store_id: 1,
            capacity: 180,
            ..Default::default()
        }),
        ..Default::default()
    };
    let before = observed.get_store(1).unwrap().store_stats();
    let response = client
        .store_heartbeat(request.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.header.unwrap(),
        pdpb::ResponseHeader {
            cluster_id: 1801,
            error: Some(pdpb::Error {
                r#type: pdpb::ErrorType::NotBootstrapped as i32,
                message: "cluster is not bootstrapped".into()
            })
        }
    );
    assert_eq!(observed.get_store(1).unwrap().store_stats(), before);
    assert!(storage.load_meta(&paths).await.unwrap().is_none());
    assert!(storage.load_store_meta(&paths, 1).await.unwrap().is_none());
    // Go checks missing stats after role/header admission but before bootstrap.
    let no_stats = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(no_stats.code(), tonic::Code::Unknown);
    let read = client
        .get_store(pdpb::GetStoreRequest {
            header: header.clone(),
            store_id: 1,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        read.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    // Streams have Go's distinct bootstrap-before-header ordering.
    let mut regions = client
        .region_heartbeat(tokio_stream::iter(
            [pdpb::RegionHeartbeatRequest::default()],
        ))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        regions
            .message()
            .await
            .unwrap()
            .unwrap()
            .header
            .unwrap()
            .error
            .unwrap()
            .r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    assert!(regions.message().await.unwrap().is_none());
    let buckets = client
        .report_buckets(tokio_stream::iter([pdpb::ReportBucketsRequest::default()]))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        buckets.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    // Use the existing persistence API, shared by startup recovery and future Bootstrap.
    storage
        .save_meta(
            &paths,
            &Cluster {
                id: 1801,
                max_peer_count: 1,
            },
        )
        .await
        .unwrap();
    assert!(
        client
            .store_heartbeat(request)
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    assert_eq!(observed.get_store(1).unwrap().store_stats().capacity, 180);
    let read = client
        .get_store(pdpb::GetStoreRequest {
            header,
            store_id: 1,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(read.store.unwrap().id, 1);
    // Corrupt only the persisted marker through a real MetaStore transaction.
    // Decode failure must remain a transport error, never mean "bootstrapped".
    use astersql_pd_storage::{MetaStore, Mutation};
    store
        .transact(
            vec![],
            vec![Mutation::put(paths.cluster_path().as_bytes(), b"\xff")],
        )
        .await
        .unwrap();
    let error = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 1801,
                ..Default::default()
            }),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                capacity: 999,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    assert_eq!(observed.get_store(1).unwrap().store_stats().capacity, 180);
    storage
        .save_meta(
            &paths,
            &Cluster {
                id: 1801,
                max_peer_count: 1,
            },
        )
        .await
        .unwrap();
    node.shutdown().await.unwrap();
    assert_eq!(storage.load_meta(&paths).await.unwrap().unwrap().id, 1801);
}
