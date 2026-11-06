// Copyright 2026 AsterSQL.
use super::{BaseServer, DelegateConnections};
use astersql_pd_mcs_metastorage_server::{Host, Service};
use astersql_pd_proto::meta_storagepb::{
    self as p, meta_storage_client::MetaStorageClient, meta_storage_server::MetaStorageServer,
};
use astersql_pd_storage_domain::{MemoryStore, MetaStore, Mutation};
use std::{
    io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio_util::sync::CancellationToken;
struct Storage(Arc<MemoryStore>);
#[async_trait::async_trait]
impl Host for Storage {
    fn cluster_id(&self) -> u64 {
        73
    }
    fn store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.0.clone())
    }
    async fn is_serving(&self) -> bool {
        true
    }
}
struct CountedListener {
    tcp: tokio::net::TcpListener,
    accepted: Arc<AtomicUsize>,
}
impl axum::serve::Listener for CountedListener {
    type Io = tokio::net::TcpStream;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let connection = self.tcp.accept().await.unwrap();
        self.accepted.fetch_add(1, Ordering::SeqCst);
        connection
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.tcp.local_addr()
    }
}
async fn serve_store(parent: &CancellationToken, value: &[u8]) -> (BaseServer, Arc<AtomicUsize>) {
    let store = Arc::new(MemoryStore::default());
    store
        .transact(vec![], vec![Mutation::put(b"hello", value)])
        .await
        .unwrap();
    let host = Arc::new(Storage(store));
    let accepted = Arc::new(AtomicUsize::new(0));
    let listener = CountedListener {
        tcp: tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
        accepted: accepted.clone(),
    };
    let server = BaseServer::start(
        tokio::runtime::Handle::current(),
        parent,
        listener,
        move |cancel| {
            Ok(
                tonic::service::Routes::new(MetaStorageServer::new(Service::new(host, cancel)))
                    .into_axum_router(),
            )
        },
        vec![],
    )
    .await
    .unwrap();
    (server, accepted)
}
#[tokio::test]
async fn mcs_063_same_target_reuses_transport_and_hosts_keep_independent_values() {
    let parent = CancellationToken::new();
    let (mut server, accepted) = serve_store(&parent, b"first").await;
    let pool = DelegateConnections::new(tokio::runtime::Handle::current(), &parent, None);
    let address = format!("http://{}", server.address);
    for _ in 0..2 {
        let mut client = MetaStorageClient::new(pool.get(&address).await.unwrap());
        let response = client
            .get(p::GetRequest {
                key: b"hello".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.kvs[0].value, b"first");
    }
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "same target must reuse the actual accepted TCP connection"
    );
    pool.shutdown();
    server.shutdown().await.unwrap();
    assert!(!parent.is_cancelled());
}
#[tokio::test]
async fn mcs_063_invalidation_shutdown_and_host_isolation_close_retained_clients() {
    let parent = CancellationToken::new();
    let (mut first, accepted) = serve_store(&parent, b"first").await;
    let (mut second, _) = serve_store(&parent, b"second").await;
    let address = format!("http://{}", first.address);
    let other = format!("http://{}", second.address);
    let pool = DelegateConnections::new(tokio::runtime::Handle::current(), &parent, None);
    let independent = DelegateConnections::new(tokio::runtime::Handle::current(), &parent, None);
    let mut retained = MetaStorageClient::new(pool.get(&address).await.unwrap());
    let mut watch = retained
        .watch(p::WatchRequest {
            key: b"hello".to_vec(),
            start_revision: 1,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(watch.message().await.unwrap().unwrap().events.len(), 1);
    let mut other_client = MetaStorageClient::new(pool.get(&other).await.unwrap());
    assert_eq!(
        other_client
            .get(p::GetRequest {
                key: b"hello".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .kvs[0]
            .value,
        b"second"
    );
    let mut independent_client = MetaStorageClient::new(independent.get(&address).await.unwrap());
    independent_client
        .get(p::GetRequest::default())
        .await
        .unwrap();
    assert_eq!(accepted.load(Ordering::SeqCst), 2);
    pool.invalidate(&address);
    let ended = tokio::time::timeout(std::time::Duration::from_secs(1), watch.message())
        .await
        .unwrap();
    assert!(!matches!(ended, Ok(Some(_))));
    assert!(retained.get(p::GetRequest::default()).await.is_err());
    let mut renewed = MetaStorageClient::new(pool.get(&address).await.unwrap());
    renewed.get(p::GetRequest::default()).await.unwrap();
    assert_eq!(accepted.load(Ordering::SeqCst), 3);
    pool.shutdown();
    pool.shutdown();
    assert!(renewed.get(p::GetRequest::default()).await.is_err());
    assert!(other_client.get(p::GetRequest::default()).await.is_err());
    independent_client
        .get(p::GetRequest::default())
        .await
        .unwrap();
    assert!(!parent.is_cancelled());
    independent.shutdown();
    first.shutdown().await.unwrap();
    second.shutdown().await.unwrap();
    assert_eq!(tokio::spawn(async { 7 }).await.unwrap(), 7);
}
#[tokio::test]
async fn mcs_063_parent_cancel_interrupts_real_pending_tls_handshake() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("https://{}", listener.local_addr().unwrap());
    let parent = CancellationToken::new();
    let pool = Arc::new(DelegateConnections::new(
        tokio::runtime::Handle::current(),
        &parent,
        Some(tonic::transport::ClientTlsConfig::new()),
    ));
    let pending = pool.clone();
    let get = tokio::spawn(async move { pending.get(&address).await.map(|_| ()) });
    let (_socket, _) = tokio::time::timeout(std::time::Duration::from_secs(1), listener.accept())
        .await
        .unwrap()
        .unwrap();
    parent.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), get)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    pool.shutdown();
    assert_eq!(tokio::spawn(async { 9 }).await.unwrap(), 9);
}
#[tokio::test]
async fn mcs_063_explicit_plaintext_credentials_override_https_target_scheme() {
    let parent = CancellationToken::new();
    let (mut server, accepted) = serve_store(&parent, b"plaintext").await;
    let pool = DelegateConnections::new(tokio::runtime::Handle::current(), &parent, None);
    let mut client = MetaStorageClient::new(
        pool.get(&format!("https://{}", server.address))
            .await
            .unwrap(),
    );
    let response = client
        .get(p::GetRequest {
            key: b"hello".to_vec(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.kvs[0].value, b"plaintext");
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    pool.shutdown();
    server.shutdown().await.unwrap();
}
