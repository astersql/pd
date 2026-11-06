// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn identity(id: u64, name: &str, client: &str, peer: &str) -> NodeConfig {
    NodeConfig {
        cluster_id: 8800,
        member_id: id,
        name: name.into(),
        client_url: client.into(),
        peer_url: peer.into(),
    }
}
fn bind_reserved(address: std::net::SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(address)?;
    socket.listen(1024)
}
async fn attempt(
    store: Arc<dyn MetaStore>,
    config: NodeConfig,
    join: &str,
    cancel: CancellationToken,
) -> Result<crate::PdNode, crate::NodeError> {
    // A connected socket reserves this ephemeral source port while allowing an
    // explicit SO_REUSEADDR listener at the same address. Other :0 fixtures cannot
    // steal the just-closed listener's address before the release assertion.
    let sink = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let reservation = tokio::net::TcpSocket::new_v4().unwrap();
    reservation.set_reuseaddr(true).unwrap();
    reservation.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = reservation.local_addr().unwrap();
    let _reservation = reservation
        .connect(sink.local_addr().unwrap())
        .await
        .unwrap();
    let _peer = sink.accept().await.unwrap().0;
    drop(sink);
    let listener = bind_reserved(address).unwrap();
    assert_eq!(
        bind_reserved(address).unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse,
        "the reservation must not permit overlapping listeners"
    );
    let result = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel)
        .with_join(join)
        .start(
            store,
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await;
    if result.is_err() {
        bind_reserved(address).unwrap();
    }
    result
}
async fn reject(store: Arc<dyn MetaStore>, config: NodeConfig, join: &str, expected: &str) {
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    let error = match attempt(store.clone(), config, join, CancellationToken::new()).await {
        Ok(mut node) => {
            node.shutdown().await.unwrap();
            panic!("expected preflight error {expected}");
        }
        Err(error) => error.to_string(),
    };
    assert!(error.contains(expected), "{error}");
    let after = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    assert_eq!(
        before, after,
        "rejected admission must not persist anything"
    );
}
async fn seed(store: &dyn MetaStore, id: u64, name: &str, client: &str, peer: &str) {
    let member = astersql_pd_proto::pdpb::Member {
        member_id: id,
        name: name.into(),
        client_urls: if client.is_empty() {
            vec![]
        } else {
            vec![client.into()]
        },
        peer_urls: vec![peer.into()],
        ..Default::default()
    };
    store
        .transact(
            vec![],
            vec![Mutation::put(
                format!("/pd/8800/rust/members/{id}").as_bytes(),
                &member.encode_to_vec(),
            )],
        )
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_088_behavior() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    reject(
        store.clone(),
        identity(88, "self", "http://self:2379", "http://self:2380"),
        "http://self:2379",
        "join self is forbidden",
    )
    .await;
    seed(
        store.as_ref(),
        1,
        "existing",
        "http://PD.EXAMPLE.:02379",
        "http://first:2380",
    )
    .await;
    reject(
        store.clone(),
        identity(88, "fresh", "http://pd.example:2379", "http://fresh:2380"),
        "http://PD.EXAMPLE.:02379",
        "already used by member",
    )
    .await;
    reject(
        store.clone(),
        identity(88, "existing", "http://fresh:2379", "http://fresh:2380"),
        "http://pd.example:2379",
        "missing data or join a duplicated pd",
    )
    .await;
    reject(
        store.clone(),
        identity(1, "renamed", "http://fresh:2379", "http://fresh:2380"),
        "http://pd.example:2379",
        "member identity conflict",
    )
    .await;
    seed(store.as_ref(), 2, "", "", "http://unfinished:2380").await;
    reject(
        store.clone(),
        identity(88, "fresh", "http://fresh:2379", "http://fresh:2380"),
        "http://pd.example:2379",
        "has not joined successfully",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_088_success_and_cancel() {
    let mut fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    seed(
        store.as_ref(),
        1,
        "existing",
        "http://existing:2379",
        "http://existing:2380",
    )
    .await;
    let config = identity(88, "fresh", "http://127.0.0.1:0", "http://fresh:2380");
    let mut node = attempt(
        store.clone(),
        config.clone(),
        "http://existing:2379",
        CancellationToken::new(),
    )
    .await
    .unwrap();
    node.ready().await.unwrap();
    let entry = store
        .read(Read::prefix(
            b"/pd/8800/rust/members/88",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    let member = astersql_pd_proto::pdpb::Member::decode(entry.value.as_slice()).unwrap();
    let mut client =
        astersql_pd_proto::pdpb::pd_client::PdClient::connect(member.client_urls[0].clone())
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
        crate::member_admin_test::request(&member.client_urls[0], "GET", "/pd/api/v1/ping", "").0,
        200
    );
    node.shutdown().await.unwrap();
    // No fresh --join on restart: the immutable persisted identity is reusable.
    let mut restart = config.clone();
    restart.client_url = member.client_urls[0].clone();
    let mut node = attempt(store.clone(), restart.clone(), "", CancellationToken::new())
        .await
        .unwrap();
    node.ready().await.unwrap();
    node.shutdown().await.unwrap();
    reject(
        store.clone(),
        restart,
        "http://existing:2379",
        "missing data or join a duplicated pd",
    )
    .await;
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = attempt(
        store.clone(),
        identity(
            89,
            "cancelled",
            "http://127.0.0.1:0",
            "http://cancelled:2380",
        ),
        "http://existing:2379",
        cancel,
    )
    .await;
    assert_eq!(result.err().unwrap().to_string(), "node is stopped");
    assert_eq!(
        before,
        store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    fixture.command("stop", "stopped");
    assert!(
        tokio::time::timeout(
            Duration::from_secs(5),
            crate::join_preflight::check(
                store.as_ref(),
                &config,
                "http://existing:2379",
                &CancellationToken::new()
            )
        )
        .await
        .unwrap()
        .is_err()
    );
    fixture.command("start", "restarted");
    assert_eq!(
        before,
        store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_088_url_boundaries() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    seed(
        store.as_ref(),
        1,
        "existing",
        "http://[::1]:02379",
        "http://[::1]:02380",
    )
    .await;
    reject(
        store.clone(),
        identity(
            88,
            "fresh",
            "http://[0:0:0:0:0:0:0:1]:2379",
            "http://other:2380",
        ),
        "http://[::1]:2379",
        "already used by member",
    )
    .await;
    // Peer-equivalent entries are excluded by Go's URL conflict precheck.
    let own = identity(
        88,
        "fresh",
        "http://[::1]:2379",
        "http://[0:0:0:0:0:0:0:1]:2380",
    );
    crate::join_preflight::check(
        store.as_ref(),
        &own,
        "http://[::1]:02379",
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    reject(
        store.clone(),
        identity(88, "fresh", "http://fresh:2379", "http://fresh:2380"),
        "http://unknown:2379",
        "join target is not registered",
    )
    .await;
    reject(
        store.clone(),
        identity(88, "fresh", "http://fresh:2379", "http://fresh:2380"),
        "http://bad:99999",
        "invalid transport endpoint",
    )
    .await;
    store
        .transact(
            vec![],
            vec![Mutation::put(b"/pd/8800/rust/members/9", &[0xff])],
        )
        .await
        .unwrap();
    reject(
        store,
        identity(88, "fresh", "http://fresh:2379", "http://fresh:2380"),
        "http://[::1]:2379",
        "failed to decode Protobuf",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_088_assigned_port_conflict() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let advertised = format!("http://{address}");
    seed(
        store.as_ref(),
        1,
        "existing",
        &advertised,
        "http://existing:2380",
    )
    .await;
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    let result = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_join(&advertised)
        .start(
            store.clone(),
            identity(88, "fresh", "http://127.0.0.1:0", "http://fresh:2380"),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await;
    let error = match result {
        Ok(mut node) => {
            node.shutdown().await.unwrap();
            panic!("assigned advertisement must be checked before registration");
        }
        Err(error) => error.to_string(),
    };
    assert!(error.contains("already used by member"), "{error}");
    assert_eq!(
        before,
        store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    tokio::net::TcpListener::bind(address).await.unwrap();
}
