// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_020_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2000,
                member_id: 1,
                name: "bootstrap".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let ping = std::process::Command::new("curl")
        .args([
            "-fsS",
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
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 2000,
        ..Default::default()
    });
    let result = client
        .is_bootstrapped(pdpb::IsBootstrappedRequest {
            header: header.clone(),
        })
        .await;
    assert!(result.is_ok(), "IsBootstrapped must be served: {result:?}");
    assert!(!result.unwrap().into_inner().bootstrapped);
    for request in [
        pdpb::BootstrapRequest {
            header: header.clone(),
            ..Default::default()
        },
        pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store::default()),
            ..Default::default()
        },
    ] {
        let error = client
            .bootstrap(request)
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .unwrap();
        assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    }
    let request = pdpb::BootstrapRequest {
        header,
        store: Some(metapb::Store {
            id: 1,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 2,
            peers: vec![metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }],
            ..Default::default()
        }),
    };
    let result = client
        .bootstrap(request.clone())
        .await
        .unwrap()
        .into_inner();
    assert!(result.header.unwrap().error.is_none());
    let result = client.bootstrap(request).await.unwrap().into_inner();
    assert_eq!(
        result.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::AlreadyBootstrapped as i32
    );
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let paths = astersql_pd_keypath::ClusterPaths::new(2000);
    let entries = store
        .read(Read::prefix(
            paths.cluster_path().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    let initial: Vec<_> = entries
        .iter()
        .filter(|e| {
            [
                paths.cluster_path(),
                paths.cluster_bootstrap_time_path(),
                paths.store_path(1),
                paths.region_path(2),
            ]
            .iter()
            .any(|k| k.as_bytes() == e.key)
        })
        .collect();
    assert_eq!(initial.len(), 4);
    use prost::Message;
    let meta = metapb::Cluster::decode(
        initial
            .iter()
            .find(|e| e.key == paths.cluster_path().as_bytes())
            .unwrap()
            .value
            .as_slice(),
    )
    .unwrap();
    assert_eq!((meta.id, meta.max_peer_count), (2000, 3));
    let time = &initial
        .iter()
        .find(|e| e.key == paths.cluster_bootstrap_time_path().as_bytes())
        .unwrap()
        .value;
    assert!(u64::from_be_bytes(time.as_slice().try_into().unwrap()) > 0);
    assert!(
        initial
            .iter()
            .all(|e| e.create_revision == initial[0].create_revision)
    );
    assert!(
        client
            .is_bootstrapped(pdpb::IsBootstrappedRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 2000,
                    ..Default::default()
                })
            })
            .await
            .unwrap()
            .into_inner()
            .bootstrapped
    );
    assert!(
        client
            .get_store(pdpb::GetStoreRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 2000,
                    ..Default::default()
                }),
                store_id: 1
            })
            .await
            .unwrap()
            .into_inner()
            .store
            .is_some()
    );
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2000,
                member_id: 1,
                name: "bootstrap".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    assert!(
        client
            .is_bootstrapped(pdpb::IsBootstrappedRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 2000,
                    ..Default::default()
                })
            })
            .await
            .unwrap()
            .into_inner()
            .bootstrapped
    );
    assert_eq!(
        client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 2000,
                    ..Default::default()
                }),
                region_id: 2,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .region
            .unwrap()
            .id,
        2
    );
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_020_transaction_fault_and_cancel() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use astersql_pd_storage_endpoint::{Cluster, StorageEndpoint};
    let mut fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(2001);
    let meta = Cluster {
        id: 2001,
        max_peer_count: 3,
    };
    let initial_store = astersql_pd_core::StoreMeta {
        id: 1,
        ..Default::default()
    };
    let initial_region = crate::core_region_adapter::region_from_proto(Some(metapb::Region {
        id: 2,
        peers: vec![metapb::Peer {
            id: 3,
            store_id: 1,
            ..Default::default()
        }],
        ..Default::default()
    }))
    .unwrap();
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", proxy_listener.local_addr().unwrap());
    let freeze = CancellationToken::new();
    let proxy_stop = CancellationToken::new();
    let proxy = tokio::spawn(relay(
        proxy_listener,
        fixture.endpoint.trim_start_matches("http://").into(),
        freeze.clone(),
        proxy_stop.clone(),
    ));
    let proxied = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[proxy_url],
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            proxied,
            NodeConfig {
                cluster_id: 2001,
                member_id: 1,
                name: "cancel-bootstrap".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    freeze.cancel();
    let call = client.bootstrap(pdpb::BootstrapRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 2001,
            ..Default::default()
        }),
        store: Some(metapb::Store {
            id: 1,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 2,
            peers: vec![metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), call)
            .await
            .is_err()
    );
    proxy_stop.cancel();
    proxy.await.unwrap();
    // The proxy was deliberately disconnected before shutdown. Lease revocation
    // can race admission cancellation and legitimately observe backend outage.
    if let Err(error) = node.shutdown().await {
        assert!(
            matches!(
                error.downcast_ref::<astersql_pd_member::MemberError>(),
                Some(astersql_pd_member::MemberError::Store(
                    astersql_pd_storage::StoreError::Unavailable
                ))
            ),
            "unexpected shutdown failure: {error:?}"
        );
    }
    fixture.command("stop", "stopped");
    assert!(
        endpoint
            .bootstrap_cluster(&paths, &meta, &initial_store, &initial_region, 1)
            .await
            .is_err()
    );
    fixture.command("start", "restarted");
    assert!(
        store
            .read(Read::prefix(
                paths.cluster_path().as_bytes(),
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        endpoint
            .bootstrap_cluster(&paths, &meta, &initial_store, &initial_region, 2)
            .await
            .unwrap()
    );
    let before = store
        .read(Read::prefix(
            paths.cluster_path().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    assert!(
        !endpoint
            .bootstrap_cluster(&paths, &meta, &initial_store, &initial_region, 3)
            .await
            .unwrap()
    );
    assert_eq!(
        before,
        store
            .read(Read::prefix(
                paths.cluster_path().as_bytes(),
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
    );
    // Stopped handlers reject admission before touching storage, even with no request header.
    struct Host(crate::Identity);
    #[async_trait::async_trait]
    impl crate::PdServices for Host {
        fn identity(&self) -> &crate::Identity {
            &self.0
        }
        async fn healthy(&self) -> Result<(), String> {
            panic!("cancelled handler touched host")
        }
        async fn alloc_id(&self) -> Result<u64, String> {
            unreachable!()
        }
        async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
            unreachable!()
        }
    }
    let cancel = CancellationToken::new();
    cancel.cancel();
    let grpc = crate::grpc::Grpc {
        services: Arc::new(Host(crate::Identity {
            cluster_id: 2001,
            member_id: 1,
            name: String::new(),
            client_url: String::new(),
            peer_url: String::new(),
        })),
        cancel,
    };
    assert_eq!(
        grpc.bootstrap_request(pdpb::BootstrapRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    assert_eq!(
        grpc.bootstrapped_request(pdpb::IsBootstrappedRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
}

async fn relay(
    listener: tokio::net::TcpListener,
    target: String,
    freeze: CancellationToken,
    stop: CancellationToken,
) {
    let mut connections = tokio::task::JoinSet::new();
    loop {
        let (mut socket, _) = tokio::select! {
            _ = stop.cancelled() => break,
            connection = listener.accept() => connection.unwrap(),
        };
        let target = target.clone();
        let freeze = freeze.clone();
        let stop = stop.clone();
        connections.spawn(async move {
            if !freeze.is_cancelled() {
                let mut upstream = tokio::net::TcpStream::connect(target).await.unwrap();
                tokio::select! {
                    _ = freeze.cancelled() => {},
                    _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream) => return,
                }
            }
            stop.cancelled().await;
            drop(socket);
        });
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
}
