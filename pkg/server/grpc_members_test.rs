// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_019_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut clients = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 1900,
                        member_id: id,
                        name: format!("member-{id}"),
                        client_url: url.clone(),
                        peer_url: format!("http://peer-{id}"),
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
        clients.push(PdClient::connect(url).await.unwrap());
        nodes.push(node);
    }
    for client in &mut clients {
        let response = client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            response.members.len(),
            2,
            "must discover the full registered membership"
        );
        assert_eq!(response.leader.unwrap().member_id, 1);
        assert!(response.etcd_leader.is_none());
        for header in [
            None,
            Some(pdpb::ResponseHeader {
                cluster_id: 999,
                error: None,
            }),
        ] {
            let info = client
                .get_cluster_info(pdpb::GetClusterInfoRequest { header })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                info.header.unwrap(),
                pdpb::ResponseHeader {
                    cluster_id: 1900,
                    error: None
                }
            );
            assert_eq!(
                info.service_modes,
                vec![pdpb::ServiceMode::PdSvcMode as i32]
            );
            assert!(info.tso_urls.is_empty());
        }
        let response = client
            .get_members(pdpb::GetMembersRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: 999,
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.members.len(), 2);
        assert!(response.header.unwrap().error.is_none());
    }
    // Retirement changes the lease-backed leader, not durable membership.
    nodes[0].shutdown().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let response = clients[1]
                .get_members(pdpb::GetMembersRequest::default())
                .await
                .unwrap()
                .into_inner();
            assert_eq!(response.members.len(), 2);
            if response.leader.is_some_and(|m| m.member_id == 2) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
}

struct DiscoveryHost {
    identity: crate::Identity,
    store: Arc<dyn astersql_pd_storage::MetaStore>,
}
#[async_trait::async_trait]
impl crate::PdServices for DiscoveryHost {
    fn identity(&self) -> &crate::Identity {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_019_errors_and_fields() {
    use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
    use prost::Message;
    let mut fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let a = pdpb::Member {
        member_id: 1,
        name: "first".into(),
        client_urls: vec!["http://one".into(), "https://one".into()],
        peer_urls: vec!["http://peer".into(), "https://peer".into()],
        leader_priority: 99,
        binary_version: "HTTP-only".into(),
        ..Default::default()
    };
    let backend_leader = store.consensus_leader_id().await.unwrap().unwrap();
    assert_ne!(backend_leader, 1);
    let backend_key = format!("/pd/1901/rust/members/{backend_leader}");
    let b = pdpb::Member {
        member_id: backend_leader,
        name: "second".into(),
        ..Default::default()
    };
    store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/pd/1901/rust/members/1", &a.encode_to_vec()),
                Mutation::put(backend_key.as_bytes(), &b.encode_to_vec()),
                Mutation::put(b"/pd/1901/leader", &a.encode_to_vec()),
            ],
        )
        .await
        .unwrap();
    let host = Arc::new(DiscoveryHost {
        identity: crate::Identity {
            cluster_id: 1901,
            member_id: 2,
            name: "second".into(),
            client_url: String::new(),
            peer_url: String::new(),
        },
        store: store.clone(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let router = crate::router(host.clone(), cancel.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap()
    });
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let before = store
        .read(Read::prefix(b"/pd/1901/", Consistency::Linearizable))
        .await
        .unwrap();
    let response = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.members.len(), 2);
    let first = response.members.iter().find(|m| m.member_id == 1).unwrap();
    assert_eq!(first.name, a.name);
    assert_eq!(first.client_urls, a.client_urls);
    assert_eq!(first.peer_urls, a.peer_urls);
    assert_eq!(first.leader_priority, 0);
    assert!(first.binary_version.is_empty());
    assert_eq!(response.leader.as_ref(), Some(first));
    assert_eq!(response.etcd_leader, Some(b));
    let after = store
        .read(Read::prefix(b"/pd/1901/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries, "discovery must be read only");
    // Corruption at the real storage boundary is a response error, not transport failure.
    store
        .transact(vec![], vec![Mutation::put(backend_key.as_bytes(), b"\xff")])
        .await
        .unwrap();
    let response = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert!(response.members.is_empty());
    assert!(response.leader.is_none());
    assert_eq!(
        response.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::Unknown as i32
    );
    assert!(
        client
            .get_cluster_info(pdpb::GetClusterInfoRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    store
        .transact(
            vec![],
            vec![
                Mutation::put(
                    backend_key.as_bytes(),
                    &pdpb::Member {
                        member_id: backend_leader,
                        ..Default::default()
                    }
                    .encode_to_vec(),
                ),
                Mutation::put(
                    b"/pd/1901/leader",
                    &pdpb::Member {
                        member_id: 99,
                        ..Default::default()
                    }
                    .encode_to_vec(),
                ),
            ],
        )
        .await
        .unwrap();
    assert!(
        client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .leader
            .is_none()
    );
    fixture.command("stop", "stopped");
    let response = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::Unknown as i32
    );
    cancel.cancel();
    drop(client);
    // Validate stopped admission directly while the real listener drains.
    let grpc = crate::grpc::Grpc {
        services: host,
        cancel,
    };
    use pdpb::pd_server::Pd;
    assert_eq!(
        grpc.get_members(tonic::Request::new(pdpb::GetMembersRequest::default()))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    assert_eq!(
        grpc.get_cluster_info(tonic::Request::new(pdpb::GetClusterInfoRequest::default()))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    server.await.unwrap();
}
