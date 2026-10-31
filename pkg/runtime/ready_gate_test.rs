// Copyright 2026 AsterSQL.
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn task_094_behavior() {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut ready = crate::ready_gate::ReadyGate::new(receiver);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), ready.wait())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), ready.wait())
            .await
            .is_err(),
        "a cancelled wait must not acknowledge an unstarted listener/worker"
    );
    sender.send(()).unwrap();
    ready.wait().await.unwrap();
    ready.wait().await.unwrap();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut ready = crate::ready_gate::ReadyGate::new(receiver);
    drop(sender);
    assert!(ready.wait().await.is_err());
    assert!(
        ready.wait().await.is_err(),
        "failed startup must remain failed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_094_real_leader_follower_and_cancel() {
    real_roles(false).await;
    real_roles(true).await;
}

async fn real_roles(bootstrapped: bool) {
    use astersql_pd_storage::MetaStore;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    if bootstrapped {
        crate::node_test::seed_bootstrap(store.clone(), 794).await;
    }
    let cancel = CancellationToken::new();
    let mut nodes = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let config = crate::NodeConfig {
            cluster_id: 794,
            member_id: id,
            name: format!("ready-{id}"),
            client_url: url.clone(),
            peer_url: format!("http://peer{id}"),
        };
        let mut node = crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                config,
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), node.ready())
            .await
            .unwrap()
            .unwrap();
        node.ready().await.unwrap();
        let output = std::process::Command::new("curl")
            .args([
                "-fsS",
                "--max-time",
                "5",
                &format!("{url}/pd/api/v2/ready?verbose"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(status["is_bootstrapped"], bootstrapped);
        assert_eq!(status["region_loaded"], bootstrapped);
        let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url)
            .await
            .unwrap();
        let members = client
            .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert_eq!(members.leader.unwrap().member_id, 1);
        assert_eq!(node.try_cluster().is_some(), id == 1);
        nodes.push(node);
    }
    cancel.cancel();
    for node in &mut nodes {
        assert!(node.ready().await.is_err());
        node.shutdown().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_094_cancelled_start_never_becomes_ready() {
    use std::sync::Arc;
    let fixture = crate::node_test::fixture::Fixture::start();
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let registry = Arc::new(prometheus::Registry::new());
    let result = crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel)
        .start(
            Arc::new(fixture.store().await),
            crate::NodeConfig {
                cluster_id: 794,
                member_id: 3,
                name: "cancelled".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://peer3".into(),
            },
            listener,
            registry.clone(),
        )
        .await;
    assert_eq!(result.err().unwrap().to_string(), "node is stopped");
    assert!(registry.gather().is_empty());
    drop(tokio::net::TcpListener::bind(address).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_094_unavailable_leader_does_not_acknowledge_follower() {
    use astersql_pd_storage::MetaStore;
    use prost::Message;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 795).await;
    let cancel = CancellationToken::new();
    // A real lease holder whose TCP socket cannot serve region synchronization.
    let unavailable = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let member = astersql_pd_member::Member::bootstrap(
        store.clone(),
        cancel.clone(),
        795,
        1,
        astersql_pd_proto::pdpb::Member {
            member_id: 1,
            name: "unavailable".into(),
            client_urls: vec![format!("http://{}", unavailable.local_addr().unwrap())],
            ..Default::default()
        }
        .encode_to_vec(),
    )
    .await
    .unwrap();
    let leadership = member.campaign(5).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut node = crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store,
            crate::NodeConfig {
                cluster_id: 795,
                member_id: 2,
                name: "waiting".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://peer2".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_millis(50), node.ready()).await;
    let second = tokio::time::timeout(Duration::from_millis(50), node.ready()).await;
    cancel.cancel();
    let cancelled = node.ready().await;
    let cleanup = node.shutdown().await;
    leadership.resign().await.unwrap();
    assert!(
        first.is_err(),
        "follower must wait for the durable region view to synchronize"
    );
    assert!(
        second.is_err(),
        "retry must not bypass pending follower synchronization"
    );
    assert!(cancelled.is_err());
    cleanup.unwrap();
    drop(tokio::net::TcpListener::bind(address).await.unwrap());
}
