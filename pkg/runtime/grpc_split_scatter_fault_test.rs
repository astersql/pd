// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[path = "../server/grpc_split_scatter_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_031_scatter_transaction_failure_and_cancel() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let backend = Arc::new(crate::grpc_ask_split_fault_test::ReservationStore {
        inner: Arc::new(fixture.store().await),
        _fixture: std::sync::Mutex::new(fixture),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            backend.clone(),
            crate::NodeConfig {
                cluster_id: 3131,
                member_id: 1,
                name: "split-scatter-fault".into(),
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
    prepare::prepare(&mut client, 3131).await;
    for store_id in 1..=3 {
        client
            .put_store(pdpb::PutStoreRequest {
                header: prepare::header(3131),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("fault-store:{store_id}"),
                    version: "8.5.0".into(),
                    node_state: 1,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let original = scenarios::region(100, b"a", b"w", 3);
    scenarios::send(&tx, 3131, original.clone()).await;
    let mut stream = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    scenarios::wait_region(&mut client, 3131, &original).await;
    for mode in [1, 2] {
        let allocated = client
            .alloc_id(pdpb::AllocIdRequest {
                header: prepare::header(3131),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .id;
        let remaining = (1000 - allocated % 1000) % 1000;
        if remaining > 0 {
            client
                .alloc_id(pdpb::AllocIdRequest {
                    header: prepare::header(3131),
                    count: remaining as u32,
                })
                .await
                .unwrap();
        }
        let before = backend
            .read(Read::prefix(
                b"/pd/3131/alloc_id",
                Consistency::Linearizable,
            ))
            .await
            .unwrap()
            .entries;
        backend.mode.store(mode, Ordering::SeqCst);
        let mut caller = client.clone();
        let task = tokio::spawn(async move {
            caller
                .split_and_scatter_regions(pdpb::SplitAndScatterRegionsRequest {
                    header: prepare::header(3131),
                    split_keys: vec![if mode == 1 {
                        b"m".to_vec()
                    } else {
                        b"p".to_vec()
                    }],
                    retry_limit: 0,
                    group: "fault".into(),
                })
                .await
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let r = stream.message().await.unwrap().unwrap();
                if r.split_region.is_some() {
                    break;
                }
            }
        })
        .await
        .unwrap();
        let replacements = if mode == 1 {
            vec![
                scenarios::region(101, b"a", b"m", 4),
                scenarios::region(102, b"m", b"w", 4),
            ]
        } else {
            vec![
                scenarios::region(102, b"m", b"p", 5),
                scenarios::region(103, b"p", b"w", 5),
            ]
        };
        for r in &replacements {
            scenarios::send(&tx, 3131, r.clone()).await;
        }
        for r in &replacements {
            scenarios::wait_region(&mut client, 3131, r).await;
        }
        if mode == 2 {
            tokio::time::timeout(Duration::from_secs(15), backend.entered.notified())
                .await
                .unwrap();
            cancel.cancel();
        }
        let error = tokio::time::timeout(Duration::from_secs(20), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        if mode == 1 {
            assert_eq!(error.code(), tonic::Code::Unknown);
            assert_eq!(error.message(), "unexpected metadata backend response");
            assert_eq!(
                backend
                    .read(Read::prefix(
                        b"/pd/3131/alloc_id",
                        Consistency::Linearizable
                    ))
                    .await
                    .unwrap()
                    .entries,
                before
            );
            let op = client
                .get_operator(pdpb::GetOperatorRequest {
                    header: prepare::header(3131),
                    region_id: 102,
                })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(op.header.unwrap().error.unwrap().message, "Not Found");
        }
        let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(backend.clone());
        let paths = astersql_pd_keypath::ClusterPaths::new(3131);
        for r in replacements {
            assert_eq!(
                endpoint
                    .load_region(&paths, r.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .id,
                r.id
            );
        }
    }
    node.shutdown().await.unwrap();
}
