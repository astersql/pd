// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::MetaStore;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_131_behavior() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let mut nodes = vec![];
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 131,
                    member_id: id,
                    name: format!("keyspace-{id}"),
                    client_url: format!("http://{}", listener.local_addr().unwrap()),
                    peer_url: format!("http://peer:{id}"),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
    }
    let old = nodes[0].keyspace_manager().unwrap();
    assert!(Arc::ptr_eq(&old, &nodes[0].keyspace_manager().unwrap()));
    assert!(nodes[1].keyspace_manager().is_none());
    old.save_new(&astersql_pd_storage_domain::Keyspace {
        identity: Some(astersql_pd_storage_domain::KeyspaceIdentity::Id(10)),
        name: "term131".into(),
        ..Default::default()
    })
    .await
    .unwrap();
    nodes[0].shutdown().await.unwrap();
    assert!(old.load_by_id(10).await.is_err());
    let next = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Some(m) = nodes[1].keyspace_manager() {
                break m;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &next));
    assert_eq!(next.load_by_id(10).await.unwrap().name, "term131");
    assert!(old.update_state_by_id(10, 1, 1).await.is_err());
    nodes[1].shutdown().await.unwrap();
}
