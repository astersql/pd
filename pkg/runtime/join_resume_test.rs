// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use prost::Message;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn recovery_case(resume_join: &str) {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let existing = astersql_pd_proto::pdpb::Member {
        member_id: 1,
        name: "existing".into(),
        client_urls: vec!["http://existing:2379".into()],
        peer_urls: vec!["http://existing:2380".into()],
        ..Default::default()
    };
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/8900/rust/members/1",
                &existing.encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = NodeConfig {
        cluster_id: 8900,
        member_id: 89,
        name: "recover".into(),
        client_url: format!("http://{address}"),
        peer_url: "http://recover:2380".into(),
    };
    let registry = Arc::new(prometheus::Registry::new());
    registry
        .register(Box::new(
            prometheus::IntCounter::new(
                "pd_rust_requests_total",
                "Minimal Rust PD domain requests",
            )
            .unwrap(),
        ))
        .unwrap();
    let failure = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_join("http://existing:2379")
        .start(store.clone(), config.clone(), listener, registry)
        .await;
    assert!(
        failure.is_err(),
        "real collector conflict must fail after registration"
    );
    let before = store
        .read(Read::prefix(
            b"/pd/8900/rust/members/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    assert_eq!(before.len(), 2, "registration must survive startup failure");
    let pending = store
        .read(Read::prefix(
            b"/pd/8900/rust/join-pending/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    assert_eq!(pending.len(), 1);
    let mut wrong = config.clone();
    wrong.peer_url = "http://different:2380".into();
    assert!(
        crate::join_preflight::check(
            store.as_ref(),
            &wrong,
            "http://existing:2379",
            &CancellationToken::new()
        )
        .await
        .is_err()
    );

    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let recovered = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_join(resume_join)
        .start(
            store.clone(),
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await;
    let mut node =
        recovered.unwrap_or_else(|e| panic!("failed registration must resume same identity: {e}"));
    node.ready().await.unwrap();
    let mut client =
        astersql_pd_proto::pdpb::pd_client::PdClient::connect(config.client_url.clone())
            .await
            .unwrap();
    assert_eq!(
        client
            .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .members
            .len(),
        2
    );
    assert_eq!(
        crate::member_admin_test::request(&config.client_url, "GET", "/pd/api/v1/ping", "").0,
        200
    );
    assert_eq!(
        before,
        store
            .read(Read::prefix(
                b"/pd/8900/rust/members/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
    );
    assert!(
        store
            .read(Read::prefix(
                b"/pd/8900/rust/join-pending/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let duplicate = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_join("http://existing:2379")
        .start(
            store.clone(),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await;
    assert!(
        duplicate
            .err()
            .unwrap()
            .to_string()
            .contains("missing data or join a duplicated pd")
    );
    assert_eq!(
        before,
        store
            .read(Read::prefix(
                b"/pd/8900/rust/members/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
    );
    let _listener = tokio::net::TcpListener::bind(address).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_089_timeout_cancels_initialization() {
    use std::time::Duration;
    let fixture = crate::node_test::fixture::Fixture::start();
    let direct = fixture.store().await;
    let before = direct
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    // A real TCP gate holds the remote HTTP/2 handshake until the caller times out.
    let gate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", gate.local_addr().unwrap());
    let (accepted_tx, accepted) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let upstream = fixture.endpoint.trim_start_matches("http://").to_owned();
    let proxy = tokio::spawn(async move {
        let (mut inbound, _) = gate.accept().await.unwrap();
        accepted_tx.send(()).unwrap();
        released.await.unwrap();
        let mut outbound = tokio::net::TcpStream::connect(upstream).await.unwrap();
        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
    });
    let store = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[endpoint], Duration::from_secs(10))
            .await
            .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = NodeConfig {
        cluster_id: 8901,
        member_id: 89,
        name: "cancelled".into(),
        client_url: format!("http://{address}"),
        peer_url: "http://cancelled:2380".into(),
    };
    let builder = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_join("http://existing:2379");
    let start = builder.start(
        store.clone(),
        config,
        listener,
        Arc::new(prometheus::Registry::new()),
    );
    let mut start = Box::pin(start);
    tokio::select! { result = &mut start => panic!("unexpected startup: {}", result.err().unwrap()), result = accepted => result.unwrap() }
    assert!(
        tokio::time::timeout(Duration::from_millis(30), start)
            .await
            .is_err()
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(listener) = tokio::net::TcpListener::bind(address).await {
            drop(listener);
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed-out initialization retained listener"
        );
        tokio::task::yield_now().await;
    }
    release.send(()).unwrap();
    // A subsequent request proves the same real connection can make progress;
    // cancelled initialization must not resume and publish a member afterwards.
    store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(
        before,
        direct
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    drop(store);
    proxy.abort();
    let _ = proxy.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_089_behavior() {
    recovery_case("http://existing:2379").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_089_restart_without_join() {
    recovery_case("").await;
}
