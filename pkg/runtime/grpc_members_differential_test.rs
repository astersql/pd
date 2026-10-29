// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

type Client = PdClient<tonic::transport::Channel>;
async fn observe(
    client: &mut Client,
    cluster_id: u64,
    follower: bool,
    external_metadata: bool,
) -> Value {
    let mut observations = Vec::new();
    for header in [
        None,
        Some(pdpb::RequestHeader {
            cluster_id: 1,
            ..Default::default()
        }),
    ] {
        let response = client
            .get_members(pdpb::GetMembersRequest { header })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response.header,
            Some(pdpb::ResponseHeader {
                cluster_id,
                error: None
            })
        );
        assert_eq!(response.members.len(), 2);
        assert!(response.tso_allocator_leaders.is_empty());
        let leader = response.leader.unwrap();
        assert!(response.members.contains(&leader));
        assert!(
            leader
                .client_urls
                .iter()
                .all(|u| u.starts_with("http://127.0.0.1:"))
        );
        if external_metadata {
            assert!(response.etcd_leader.is_none());
        } else {
            let etcd = response.etcd_leader.as_ref().expect("embedded etcd leader");
            assert!(response.members.contains(etcd));
        }
        let mut fields = Vec::new();
        for member in response.members {
            assert_ne!(member.member_id, 0);
            assert_eq!(member.client_urls.len(), 1);
            assert_eq!(member.peer_urls.len(), 1);
            assert!(member.client_urls[0].starts_with("http://127.0.0.1:"));
            assert!(member.peer_urls[0].starts_with("http://127.0.0.1:"));
            assert!(["compat", "request-follower"].contains(&member.name.as_str()));
            assert_eq!(
                member.name == "compat",
                member.member_id == leader.member_id
            );
            assert_eq!(member.leader_priority, 0);
            assert!(
                member.deploy_path.is_empty()
                    && member.binary_version.is_empty()
                    && member.git_hash.is_empty()
                    && member.dc_location.is_empty()
            );
            fields.push(json!({"name":member.name,"leader":member.member_id==leader.member_id,"client_urls":member.client_urls.len(),"peer_urls":member.peer_urls.len()}));
        }
        fields.sort_by_key(|value| value["leader"].as_bool());
        observations.push(json!(fields));
    }
    for header in [
        None,
        Some(pdpb::ResponseHeader {
            cluster_id: 1,
            error: None,
        }),
    ] {
        let response = client
            .get_cluster_info(pdpb::GetClusterInfoRequest { header })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response.header,
            Some(pdpb::ResponseHeader {
                cluster_id,
                error: None
            })
        );
        assert_eq!(
            response.service_modes,
            vec![pdpb::ServiceMode::PdSvcMode as i32]
        );
        assert!(response.tso_urls.is_empty());
        observations.push(json!({"modes":response.service_modes,"tso_urls":response.tso_urls}));
    }
    json!({"follower":follower,"observations":observations})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_members_differential() {
    use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
    use astersql_pd_compat_tests::harness::Server;
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut rust = Vec::new();
    let mut peers = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let peer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let peer_url = format!("http://{}", peer.local_addr().unwrap());
        peers.push(peer);
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 1802,
                        member_id: id,
                        name: if id == 1 {
                            "compat".into()
                        } else {
                            "request-follower".into()
                        },
                        client_url: url.clone(),
                        peer_url,
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let client = PdClient::connect(url).await.unwrap();
        rust.push(client);
        nodes.push(node);
    }
    let mut rust_observations = Vec::new();
    for (index, client) in rust.iter_mut().enumerate() {
        rust_observations.push(observe(client, 1802, index == 1, true).await);
    }
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut leader = Server::start(&binary).unwrap();
    leader.ready().unwrap();
    let join = format!("--join={}", leader.url);
    let mut follower = Server::start_with_args(
        &binary,
        &["--name=request-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    let mut client = PdClient::connect(leader.url.clone()).await.unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let mut go = vec![observe(&mut client, cluster_id, false, false).await];
    let mut client = PdClient::connect(follower.url.clone()).await.unwrap();
    go.push(observe(&mut client, cluster_id, true, false).await);
    println!("Go observations: {go:?}\nRust observations: {rust_observations:?}");
    assert_eq!(rust_observations, go);
    follower
        .stop_with_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    leader
        .stop_with_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
    fault_differential(&binary, &fixture.endpoint, store).await;
}

// A real TCP fault boundary: freeze forwarding but retain accepted sockets.
// Requests then wait for their actual backend deadlines instead of a mock result.
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
struct FaultHost {
    identity: astersql_pd_server::Identity,
    store: Arc<dyn astersql_pd_storage::MetaStore>,
}
#[async_trait::async_trait]
impl astersql_pd_server::PdServices for FaultHost {
    fn identity(&self) -> &astersql_pd_server::Identity {
        &self.identity
    }
    fn discovery_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        Err("not leader".into())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("not leader".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("not leader".into())
    }
}
async fn observe_fault(client: &mut Client, cluster_id: u64) -> Value {
    use std::time::Duration;
    // Dropping a real in-flight RPC cancels its HTTP/2 stream. The server remains
    // usable and the next request observes the actual metadata deadline.
    assert!(
        tokio::time::timeout(
            Duration::from_millis(50),
            client.get_members(pdpb::GetMembersRequest::default())
        )
        .await
        .is_err()
    );
    let response = tokio::time::timeout(
        Duration::from_secs(20),
        client.get_members(pdpb::GetMembersRequest::default()),
    )
    .await
    .unwrap()
    .unwrap()
    .into_inner();
    let header = response.header.unwrap();
    assert_eq!(header.cluster_id, cluster_id);
    let error = header.error.unwrap();
    assert!(!error.message.is_empty());
    println!("actual discovery backend error: {}", error.message);
    assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
    assert!(
        response.members.is_empty() && response.leader.is_none() && response.etcd_leader.is_none()
    );
    let info = client
        .get_cluster_info(pdpb::GetClusterInfoRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        info.header.unwrap(),
        pdpb::ResponseHeader {
            cluster_id,
            error: None
        }
    );
    json!({"rpc":"OK","error_type":error.r#type,"members":response.members.len(),"leader":false,"etcd_leader":false,"cluster_info_modes":info.service_modes})
}
async fn fault_differential(
    binary: &std::path::Path,
    endpoint: &str,
    store: Arc<dyn astersql_pd_storage::MetaStore>,
) {
    use astersql_pd_storage::{Consistency, Read};
    use std::time::Duration;
    let before = store
        .read(Read::prefix(b"/pd/1802/", Consistency::Linearizable))
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", listener.local_addr().unwrap());
    let freeze = CancellationToken::new();
    let stop = CancellationToken::new();
    let proxy = tokio::spawn(relay(
        listener,
        endpoint.trim_start_matches("http://").into(),
        freeze.clone(),
        stop.clone(),
    ));
    let fault_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[proxy_url], Duration::from_secs(2))
            .await
            .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let host = Arc::new(FaultHost {
        identity: astersql_pd_server::Identity {
            cluster_id: 1802,
            member_id: 1,
            name: "compat".into(),
            client_url: url.clone(),
            peer_url: String::new(),
        },
        store: Arc::new(fault_store),
    });
    let router = astersql_pd_server::router(host, cancel.clone());
    let shutdown = cancel.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap()
    });
    let mut rust = PdClient::connect(url).await.unwrap();
    // Warm the real connection before freezing it.
    assert!(
        rust.get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    freeze.cancel();
    let rust_observation = observe_fault(&mut rust, 1802).await;
    drop(rust);
    cancel.cancel();
    server.await.unwrap();
    stop.cancel();
    proxy.await.unwrap();
    let after = store
        .read(Read::prefix(b"/pd/1802/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let advertise = format!(
        "--advertise-client-urls=http://{}",
        listener.local_addr().unwrap()
    );
    let mut go =
        astersql_pd_compat_tests::harness::Server::start_with_args(binary, &[&advertise]).unwrap();
    let freeze = CancellationToken::new();
    let stop = CancellationToken::new();
    let proxy = tokio::spawn(relay(
        listener,
        go.url.trim_start_matches("http://").into(),
        freeze.clone(),
        stop.clone(),
    ));
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    freeze.cancel();
    // The Go membership cache TTL is one second. Let it expire before faulting
    // the read; this is an observed cache policy, not a failpoint or mock clock.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let go_observation = observe_fault(&mut client, cluster_id).await;
    assert_eq!(rust_observation, go_observation);
    stop.cancel();
    proxy.await.unwrap();
    go.stop_with_timeout(Duration::from_secs(30)).unwrap();
}
