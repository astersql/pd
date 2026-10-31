// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

// A single-thread host also detects blocking shutdown/private-runtime nesting.
#[tokio::test]
async fn task_096_behavior() {
    scenario(false).await;
}

#[tokio::test]
async fn task_096_failed_start_and_host_cancel() {
    scenario(true).await;
}

async fn scenario(failed_start: bool) {
    tokio::time::timeout(Duration::from_secs(30), async {
        let fixture = fixture::Fixture::start();
        let store = Arc::new(fixture.store().await);
        let parent = CancellationToken::new();
        let handle = tokio::runtime::Handle::current();
        let global_before = prometheus::gather();
        let (ping, mut pings) = tokio::sync::mpsc::channel::<tokio::sync::oneshot::Sender<()>>(1);
        let host_cancel = parent.clone();
        let host = handle.spawn(async move {
            loop {
                tokio::select! {
                    _ = host_cancel.cancelled() => break,
                    Some(reply) = pings.recv() => { reply.send(()).unwrap(); }
                }
            }
        });
        let mut nodes = Vec::new();
        let mut clients = Vec::new();
        let mut registries = Vec::new();
        let mut addresses = Vec::new();
        for id in [960, 961] {
            crate::node_test::seed_bootstrap(store.clone(), id).await;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let registry = Arc::new(prometheus::Registry::new());
            let sentinel = prometheus::IntGauge::new("host_owned", "Host collector").unwrap();
            sentinel.set(id as i64);
            registry.register(Box::new(sentinel)).unwrap();
            let mut node = PdNodeBuilder::new(handle.clone(), parent.clone())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: id,
                        member_id: id,
                        name: format!("embedded-{id}"),
                        client_url: format!("http://{address}"),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    registry.clone(),
                )
                .await
                .unwrap();
            node.ready().await.unwrap();
            clients.push(
                PdClient::connect(format!("http://{address}"))
                    .await
                    .unwrap(),
            );
            addresses.push(address);
            registries.push(registry);
            nodes.push(node);
        }
        let first = alloc(&mut clients[1], 961).await;
        alloc(&mut clients[0], 960).await;
        let leader_before = store
            .read(Read::range(
                b"/pd/961/leader",
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        assert_eq!(leader_before.entries.len(), 1);
        if failed_start {
            // Deliberate collector ownership conflict is a real startup error.
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let result = PdNodeBuilder::new(handle.clone(), parent.clone())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 962,
                        member_id: 962,
                        name: "rejected".into(),
                        client_url: format!("http://{address}"),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    registries[1].clone(),
                )
                .await;
            assert!(result.is_err());
            assert!(tokio::net::TcpStream::connect(address).await.is_err());
            assert!(
                store
                    .read(Read::prefix(b"/pd/962/", Consistency::Linearizable))
                    .await
                    .unwrap()
                    .entries
                    .is_empty()
            );
            alloc(&mut clients[0], 960).await;
        }
        // Both instances are live; closing A must preserve B and the host task.
        nodes[0].shutdown().await.unwrap();
        nodes[0].shutdown().await.unwrap();
        assert!(!parent.is_cancelled());
        assert!(tokio::net::TcpStream::connect(addresses[0]).await.is_err());
        let rebound = tokio::net::TcpListener::bind(addresses[0]).await.unwrap();
        drop(rebound);
        assert!(alloc(&mut clients[1], 961).await > first);
        let wrong_cluster = clients[1]
            .alloc_id(pdpb::AllocIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 960,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(wrong_cluster.code(), tonic::Code::FailedPrecondition);
        let url = format!("http://{}/pd/api/v1/health", addresses[1]);
        let response = tokio::task::spawn_blocking(move || {
            std::process::Command::new("curl")
                .args(["-fsS", "--max-time", "3", &url])
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert!(response.status.success());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response.stdout).unwrap()[0]["health"],
            true
        );
        let leader_after = store
            .read(Read::range(
                b"/pd/961/leader",
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        assert_eq!(
            leader_before.entries[0].mod_revision,
            leader_after.entries[0].mod_revision
        );
        assert!(
            store
                .read(Read::range(
                    b"/pd/960/leader",
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
                .is_empty()
        );
        assert!(
            store
                .read(Read::prefix(b"/pd/961/", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.key.ends_with(b"alloc_id"))
        );
        let (reply, pong) = tokio::sync::oneshot::channel();
        ping.send(reply).await.unwrap();
        pong.await.unwrap();
        assert!(!host.is_finished());
        assert_eq!(registries[0].gather().len(), 1);
        assert!(registries[1].gather().len() > 1);
        if failed_start {
            parent.cancel();
        }
        nodes[1].shutdown().await.unwrap();
        assert_eq!(registries[1].gather().len(), 1);
        assert_eq!(prometheus::gather(), global_before);
        assert_eq!(parent.is_cancelled(), failed_start);
        parent.cancel();
        host.await.unwrap();
        drop((nodes, clients));
        assert_eq!(Arc::strong_count(&store), 1);
    })
    .await
    .expect("embedded nodes must not stall the host executor");
}

async fn alloc(client: &mut PdClient<tonic::transport::Channel>, cluster_id: u64) -> u64 {
    let response = client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    response.id
}

// Go exposes process lifecycle, not the Rust Handle/Registry embedding API.
// Compare only its observable independent-node shutdown contract here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_096_readonly_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut a = Server::start(&binary).unwrap();
    let mut b = Server::start(&binary).unwrap();
    a.ready().unwrap();
    b.ready().unwrap();
    let mut client = PdClient::connect(b.url.clone()).await.unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let before = alloc(&mut client, cluster_id).await;
    let wrong = client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster_id.wrapping_add(1),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(wrong.code(), tonic::Code::FailedPrecondition);
    a.stop().unwrap();
    assert!(
        tokio::net::TcpStream::connect(a.url.strip_prefix("http://").unwrap())
            .await
            .is_err()
    );
    assert!(alloc(&mut client, cluster_id).await > before);
    let response = std::process::Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            "3",
            &format!("{}/pd/api/v1/health", b.url),
        ])
        .output()
        .unwrap();
    assert!(response.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.stdout).unwrap()[0]["health"],
        true
    );
    b.stop().unwrap();
    drop((a, b));
    scenario(false).await;
    scenario(true).await;
}
