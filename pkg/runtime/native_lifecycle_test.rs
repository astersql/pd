// Copyright 2026 AsterSQL.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use astersql_pd_member::MemberError;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage_kv::{NativeMetaStore, StoreError, open_isolated};

use crate::{NodeConfig, PdNode, PdNodeBuilder, bootstrap_config};

async fn open_store(directory: &Path, initialize: bool) -> Arc<NativeMetaStore> {
    let store = open_isolated(directory, 27, 1).await.unwrap();
    if initialize {
        store
            .consensus()
            .initialize(BTreeSet::from([1]))
            .await
            .unwrap();
    }
    store
        .consensus()
        .wait(Some(Duration::from_secs(5)))
        .current_leader(1, "local native voter")
        .await
        .unwrap();
    Arc::new(store)
}

async fn ready(node: &mut PdNode) {
    timeout(Duration::from_secs(5), node.ready())
        .await
        .unwrap()
        .unwrap();
}

async fn allocate_id(config: &NodeConfig) -> u64 {
    let mut client = PdClient::connect(config.client_url.clone()).await.unwrap();
    let mut request = tonic::Request::new(pdpb::AllocIdRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: config.cluster_id,
            ..Default::default()
        }),
        ..Default::default()
    });
    request.set_timeout(Duration::from_secs(5));
    let response = client.alloc_id(request).await.unwrap().into_inner();
    assert!(response.header.unwrap().error.is_none());
    response.id
}

async fn close_store(store: Arc<NativeMetaStore>) {
    let Ok(store) = Arc::try_unwrap(store) else {
        panic!("PD retained the host's store after shutdown");
    };
    store.close().await.unwrap();
}

#[tokio::test]
async fn theme10_native_lifecycle() {
    timeout(Duration::from_secs(25), async {
        // The offline network is sufficient for one voter. The host owns Raft
        // and the test uses the same SQLite directory across both PD lifetimes.
        let directory = tempfile::tempdir().unwrap();
        let store = open_store(directory.path(), true).await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let config = bootstrap_config(
            store.as_ref(),
            "native-embedded".into(),
            format!("http://{address}"),
            "http://127.0.0.1:2380".into(),
        )
        .await
        .unwrap();
        let host = Handle::current();
        let cancel = CancellationToken::new();
        let registry = Arc::new(prometheus::Registry::new());
        let sentinel =
            prometheus::IntGauge::new("native_lifecycle_host", "Host collector").unwrap();
        sentinel.set(27);
        registry.register(Box::new(sentinel)).unwrap();
        let mut pd = PdNodeBuilder::new(host.clone(), cancel.clone())
            .start(store.clone(), config.clone(), listener, registry.clone())
            .await
            .unwrap();
        ready(&mut pd).await;
        let first_id = allocate_id(&config).await;
        assert!(first_id > 0);
        let metrics = registry.gather();
        assert!(
            metrics
                .iter()
                .any(|family| family.name() == "native_lifecycle_host")
        );
        assert!(
            metrics
                .iter()
                .any(|family| family.name() == "pd_server_info")
        );
        pd.shutdown().await.unwrap();
        assert_eq!(pd.events().last(), Some(&"storage-released"));
        assert!(!cancel.is_cancelled());
        drop(pd);
        close_store(store).await;

        let store = open_store(directory.path(), false).await;
        let listener = TcpListener::bind(address).await.unwrap();
        let mut pd = PdNodeBuilder::new(host.clone(), cancel.clone())
            .start(
                store.clone(),
                config.clone(),
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        ready(&mut pd).await;
        let recovered_id = allocate_id(&config).await;
        assert!(recovered_id > first_id);
        println!("native WAL: allocated ID {first_id}, after reopen {recovered_id}");

        store.consensus().shutdown().await.unwrap();
        assert!(
            timeout(Duration::from_secs(5), pd.ready())
                .await
                .unwrap()
                .is_err()
        );
        // Closing Raft also makes lease revocation unavailable. The worker
        // reports that error after releasing PD's listener and storage reference.
        let error = timeout(Duration::from_secs(5), pd.wait())
            .await
            .unwrap()
            .unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<MemberError>(),
                Some(MemberError::Store(StoreError::Unavailable))
            ),
            "unexpected worker error: {error}"
        );
        pd.shutdown().await.unwrap();
        assert_eq!(pd.events().last(), Some(&"storage-released"));
        drop(pd);

        let listener = TcpListener::bind(address).await.unwrap();
        let result = PdNodeBuilder::new(host.clone(), cancel.clone())
            .start(
                store.clone(),
                config,
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await;
        assert!(
            result.is_err(),
            "start succeeded with a stopped native store"
        );
        drop(TcpListener::bind(address).await.unwrap());
        close_store(store).await;
        assert!(!cancel.is_cancelled());
        assert_eq!(host.spawn(async { 27 }).await.unwrap(), 27);
        println!("native store shutdown: readiness denied, startup rejected, listener released");
    })
    .await
    .unwrap();
}
