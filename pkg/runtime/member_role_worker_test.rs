// Copyright 2026 AsterSQL.
use super::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::*;
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn header() -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: 707,
        ..Default::default()
    })
}
async fn alloc(
    client: &mut PdClient<tonic::transport::Channel>,
) -> std::result::Result<u64, tonic::Status> {
    client
        .alloc_id(pdpb::AllocIdRequest {
            header: header(),
            ..Default::default()
        })
        .await
        .map(|r| r.into_inner().id)
}
async fn tso(
    client: &mut PdClient<tonic::transport::Channel>,
) -> std::result::Result<i64, tonic::Status> {
    let mut stream = client
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header: header(),
            count: 1,
            ..Default::default()
        }]))
        .await?
        .into_inner();
    let reply = stream.message().await?.unwrap();
    Ok(reply.timestamp.unwrap().physical)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_007_behavior() {
    rust_roles().await;
}

async fn rust_roles() -> serde_json::Value {
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut clients = Vec::new();
    let mut registries = Vec::new();
    let parent = CancellationToken::new();
    for id in 1..=3 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let config = NodeConfig {
            cluster_id: 707,
            member_id: id,
            name: format!("role-{id}"),
            client_url: url.clone(),
            peer_url: format!("http://127.0.0.1:{}", 2380 + id),
        };
        let registry = Arc::new(prometheus::Registry::new());
        let result = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.clone())
            .start(store.clone(), config, listener, registry.clone())
            .await;
        if let Err(error) = &result {
            for node in &mut nodes {
                let node: &mut super::PdNode = node;
                node.shutdown().await.unwrap();
            }
            panic!("follower must start under existing leader: {error}");
        }
        let mut node = result.unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        clients.push(PdClient::connect(url).await.unwrap());
        registries.push(registry);
    }
    let first = alloc(&mut clients[0]).await.unwrap();
    let physical = tso(&mut clients[0]).await.unwrap();
    for client in &mut clients[1..] {
        assert!(alloc(client).await.is_err());
        assert!(tso(client).await.is_err());
    }
    assert!(nodes[1].try_cluster().is_none());
    // A follower blocked in watch must cancel without disturbing the current lease.
    nodes[2].shutdown().await.unwrap();
    assert!(registries[2].gather().is_empty());
    assert!(alloc(&mut clients[0]).await.unwrap() > first);
    let old_host = nodes[0].cluster();
    nodes[0].shutdown().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if alloc(&mut clients[1]).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(tso(&mut clients[1]).await.unwrap() > physical);
    assert!(old_host.snapshot().await.is_err());
    // Revoke the serving lease. The next request must fail even before renewal/watch
    // maintenance runs; it cannot consume a cached TSO/ID window from the old term.
    let snapshot = store
        .read(Read::range(
            b"/pd/707/leader",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let entry = &snapshot.entries[0];
    assert_eq!(
        pdpb::Member::decode(entry.value.as_slice())
            .unwrap()
            .member_id,
        2
    );
    store.revoke(entry.lease.unwrap()).await.unwrap();
    // Runtime may already have acquired a new term, so compare persisted lease/revision
    // instead of assuming a scheduler delay after the revoke response.
    let old_revision = entry.mod_revision;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let current = store
                .read(Read::range(
                    b"/pd/707/leader",
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            if current
                .entries
                .first()
                .is_some_and(|e| e.mod_revision > old_revision)
                && tso(&mut clients[1]).await.is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    nodes[1].shutdown().await.unwrap();
    for registry in registries {
        assert!(registry.gather().is_empty());
    }
    assert!(!parent.is_cancelled());
    assert!(
        store
            .read(Read::range(
                b"/pd/707/leader",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    serde_json::json!({"members":3,"follower_rejects":true,"id_increases":true,"tso_increases":true,"handover":true,"revoke_recovers":true})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_007_stale_scheduling_transaction_is_fenced() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let first = Arc::new(
        astersql_pd_member::Leadership::campaign(
            store.clone(),
            cancel.clone(),
            707,
            b"first".to_vec(),
            5,
        )
        .await
        .unwrap(),
    );
    let fenced = super::member_role_worker::TermStore::new(store.clone(), first.clone(), cancel);
    fenced
        .transact(vec![], vec![Mutation::put(b"/schedule/accepted", b"first")])
        .await
        .unwrap();
    store.revoke(first.lease_id()).await.unwrap();
    let second = astersql_pd_member::Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        707,
        b"second".to_vec(),
        5,
    )
    .await
    .unwrap();
    assert!(
        fenced
            .transact(vec![], vec![Mutation::put(b"/schedule/rejected", b"stale")])
            .await
            .is_err()
    );
    assert!(
        store
            .read(Read::prefix(
                b"/schedule/rejected",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    second.validate().await.unwrap();
    second.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_member_role_differential() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle};
    use astersql_pd_storage_kv::EtcdMetaStore;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    readonly_oracle::validate(
        "go",
        &binary,
        None,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/rust-compat/fixtures/health-member.json"),
    )
    .unwrap();
    let rust = rust_roles().await;
    let mut first = Server::start(&binary).unwrap();
    first.ready().unwrap();
    let join = format!("--join={}", first.url);
    let mut second = Server::start_with_args(
        &binary,
        &["--name=role-second", "--initial-cluster=", &join],
    )
    .unwrap();
    second.ready().unwrap();
    // Strict etcd reconfiguration can temporarily reject MemberAdd while the
    // newly joined pair establishes health. Only that pre-admission error retries.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut third = loop {
        let mut candidate =
            Server::start_with_args(&binary, &["--name=role-third", "--initial-cluster=", &join])
                .unwrap();
        match candidate.ready() {
            Ok(()) => break candidate,
            Err(error)
                if error.contains("etcdserver: unhealthy cluster")
                    && tokio::time::Instant::now() < deadline =>
            {
                drop(candidate);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            Err(error) => panic!("third member failed: {error}"),
        }
    };
    let mut client = PdClient::connect(first.url.clone()).await.unwrap();
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(members.members.len(), 3);
    let cluster_id = members.header.unwrap().cluster_id;
    let request_header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let request = pdpb::AllocIdRequest {
        header: request_header.clone(),
        ..Default::default()
    };
    let old_id = client
        .alloc_id(request.clone())
        .await
        .unwrap()
        .into_inner()
        .id;
    let mut follower = PdClient::connect(second.url.clone()).await.unwrap();
    assert!(follower.alloc_id(request.clone()).await.is_err());
    async fn time(
        client: &mut PdClient<tonic::transport::Channel>,
        header: Option<pdpb::RequestHeader>,
    ) -> std::result::Result<i64, tonic::Status> {
        let mut stream = client
            .tso(tokio_stream::iter([pdpb::TsoRequest {
                header,
                count: 1,
                ..Default::default()
            }]))
            .await?
            .into_inner();
        Ok(stream.message().await?.unwrap().timestamp.unwrap().physical)
    }
    assert!(time(&mut follower, request_header.clone()).await.is_err());
    let old_time = time(&mut client, request_header.clone()).await.unwrap();
    first.stop_with_timeout(Duration::from_secs(30)).unwrap();
    let leader_url = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            if let Ok(reply) = follower
                .get_members(pdpb::GetMembersRequest::default())
                .await
            {
                if let Some(leader) = reply.into_inner().leader {
                    if leader.client_urls[0] != first.url {
                        break leader.client_urls[0].clone();
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let mut leader = PdClient::connect(leader_url).await.unwrap();
    let id = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(reply) = leader.alloc_id(request.clone()).await {
                break reply.into_inner().id;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert!(id > old_id);
    assert!(time(&mut leader, request_header.clone()).await.unwrap() > old_time);
    let backend = EtcdMetaStore::connect(
        &[second.url.clone(), third.url.clone()],
        Duration::from_secs(3),
    )
    .await
    .unwrap();
    let key = format!("/pd/{cluster_id}/leader");
    let snapshot = backend
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    let old = &snapshot.entries[0];
    backend.revoke(old.lease.unwrap()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let snapshot = backend
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            if let Some(entry) = snapshot
                .entries
                .first()
                .filter(|entry| entry.mod_revision > old.mod_revision)
            {
                let member = pdpb::Member::decode(entry.value.as_slice()).unwrap();
                let mut client = PdClient::connect(member.client_urls[0].clone())
                    .await
                    .unwrap();
                if let Ok(reply) = client.alloc_id(request.clone()).await {
                    assert!(reply.into_inner().id > id);
                    if time(&mut client, request_header.clone()).await.is_ok() {
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let go = serde_json::json!({"members":members.members.len(),"follower_rejects":true,"id_increases":true,"tso_increases":true,"handover":true,"revoke_recovers":true});
    println!("Go={go}; Rust={rust}");
    assert_eq!(go, rust);
    drop((client, follower, leader, backend));
    second.stop_with_timeout(Duration::from_secs(30)).unwrap();
    third.stop_with_timeout(Duration::from_secs(30)).unwrap();
}
