// Copyright 2026 AsterSQL.
use super::{NodeConfig, PdNodeBuilder};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
pub(super) mod fixture;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_runtime_network_lifecycle() {
    let fixture = fixture::Fixture::start();
    let cancel = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let config = NodeConfig {
        cluster_id: 42,
        member_id: 7,
        name: "compat".into(),
        client_url: url.clone(),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            Arc::new(fixture.store().await),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = std::process::Command::new("curl")
        .args(["-fsS", &format!("{url}/pd/api/v1/health")])
        .output()
        .unwrap();
    assert!(response.status.success());
    let health: serde_json::Value = serde_json::from_slice(&response.stdout).unwrap();
    assert_eq!(health[0]["health"], true);
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(members.header.unwrap().cluster_id, 42);
    assert_eq!(members.leader.unwrap().member_id, 7);
    let header = Some(pdpb::RequestHeader {
        cluster_id: 42,
        ..Default::default()
    });
    let id = client
        .alloc_id(pdpb::AllocIdRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(id.id, 1);
    let mut stream = client
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header,
            count: 2,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    let tso = stream.message().await.unwrap().unwrap();
    assert_eq!(tso.count, 2);
    assert!(tso.timestamp.unwrap().physical > 0);
    assert_eq!(
        client
            .get_all_stores(pdpb::GetAllStoresRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unimplemented
    );
    drop((stream, client));
    node.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
    assert!(!cancel.is_cancelled());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    assert_eq!(
        node.events(),
        vec![
            "storage",
            "leadership",
            "services",
            "listener",
            "ready",
            "stop-admission",
            "background-joined",
            "leadership-released",
            "storage-released"
        ]
    );
    println!("embedded: {:?}", node.events());
    // Lease was released: same identity and endpoint can be used immediately.
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let mut again = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 42,
                member_id: 7,
                name: "compat".into(),
                client_url: url,
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    again.ready().await.unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), again.shutdown())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn startup_failure_releases_term_and_listener() {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let existing =
        prometheus::IntCounter::new("pd_rust_requests_total", "Minimal Rust PD domain requests")
            .unwrap();
    registry.register(Box::new(existing.clone())).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = NodeConfig {
        cluster_id: 43,
        member_id: 9,
        name: "node".into(),
        client_url: format!("http://{address}"),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    assert!(
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(store.clone(), config.clone(), listener, registry.clone())
            .await
            .is_err()
    );
    registry.unregister(Box::new(existing)).unwrap();
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(store, config, listener, registry)
        .await
        .unwrap();
    node.ready().await.unwrap();
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_loss_retires_term_and_preserves_listener() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = NodeConfig {
        cluster_id: 44,
        member_id: 10,
        name: "node".into(),
        client_url: format!("http://{address}"),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let leader = store
        .read(Read::prefix(b"/pd/44/leader", Consistency::Linearizable))
        .await
        .unwrap();
    store
        .revoke(leader.entries[0].lease.unwrap())
        .await
        .unwrap();
    let old_revision = leader.entries[0].mod_revision;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let current = store
                .read(Read::range(
                    b"/pd/44/leader",
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            if current
                .entries
                .first()
                .is_some_and(|entry| entry.mod_revision > old_revision)
                && node.try_cluster().is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    node.ready().await.unwrap();
    assert!(tokio::net::TcpStream::connect(address).await.is_ok());
    node.shutdown().await.unwrap();
}
