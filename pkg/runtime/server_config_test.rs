// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_050_active_consumers() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    crate::node_test::seed_bootstrap(store.clone(), 7493).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 7493,
                member_id: 1,
                name: "rules".into(),
                client_url: url,
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let host = node.cluster();

    host.patch_server_config("pd-server.flow-round-by-digit".into(), serde_json::json!(8))
        .await
        .unwrap();
    host.patch_server_config("pd-server.key-type".into(), serde_json::json!("raw"))
        .await
        .unwrap();
    host.patch_server_config(
        "pd-server.min-resolved-ts-persistence-interval".into(),
        serde_json::json!("20ms"),
    )
    .await
    .unwrap();
    host.patch_server_config("cluster-version".into(), serde_json::json!("1.0.0"))
        .await
        .unwrap();
    assert_eq!(
        host.batch_split_version().await.unwrap(),
        Some("1.0.0".into())
    );
    host.patch_server_config("cluster-version".into(), serde_json::json!("5.0.0"))
        .await
        .unwrap();
    assert_eq!(host.batch_split_version().await.unwrap(), None);
    {
        let guard = host.resources.lock().await;
        let r = guard.as_ref().unwrap();
        let state = r.state.lock().unwrap();
        assert_eq!(state.config.flow_round_by_digit, 8);
        assert_eq!(
            state.config.key_type,
            astersql_pd_core_constant::KeyType::RAW
        );
        assert_eq!(
            r.resolved_interval
                .load(std::sync::atomic::Ordering::Relaxed),
            20_000_000
        );
    }
    assert!(
        host.patch_server_config("pd-server.key-type".into(), serde_json::json!("bad"))
            .await
            .is_err()
    );
    assert_eq!(
        host.configuration().await.unwrap().pd_server.key_type,
        "raw"
    );
    node.shutdown().await.unwrap();
}
