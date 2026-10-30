// Copyright 2026 AsterSQL.
use super::*;
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_schedule_config::SharedConfigProvider;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_049_active_consumers() {
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
    host.patch_replication(
        false,
        serde_json::json!({"enable-placement-rules":"true","max-replicas":5}),
    )
    .await
    .unwrap();
    host.patch_replication(true,serde_json::json!({"action":"set","type":"reject-leader","label-key":"zone","label-value":"z1"})).await.unwrap();
    {
        let guard = host.resources.lock().await;
        let state = guard.as_ref().unwrap().state.lock().unwrap();
        assert_eq!(state.rules.get_rule("pd", "default").unwrap().count, 5);
        assert_eq!(state.config.replication.max_replicas, 5);
        assert!(state.config.check_label_property(
            "reject-leader",
            &[astersql_pd_core::StoreLabel {
                key: "zone".into(),
                value: "z1".into()
            }]
        ));
    }
    assert!(
        host.patch_replication(false, serde_json::json!({"max-replicas":0}))
            .await
            .is_err()
    );
    {
        let guard = host.resources.lock().await;
        let state = guard.as_ref().unwrap().state.lock().unwrap();
        assert_eq!(state.rules.get_rule("pd", "default").unwrap().count, 5);
        assert_eq!(state.config.replication.max_replicas, 5);
    }
    {
        let guard = host.resources.lock().await;
        let state = guard.as_ref().unwrap().state.lock().unwrap();
        state
            .cluster
            .stores
            .put_store(Arc::new(astersql_pd_core::StoreInfo::new(
                astersql_pd_core::StoreMeta {
                    id: 99,
                    labels: vec![astersql_pd_core::StoreLabel {
                        key: "engine".into(),
                        value: "tiflash".into(),
                    }],
                    ..Default::default()
                },
            )));
    }
    assert_eq!(
        host.patch_replication(false, serde_json::json!({"enable-placement-rules":"false"}))
            .await,
        Err("cannot disable placement rules with TiFlash nodes".into())
    );
    {
        let guard = host.resources.lock().await;
        let state = guard.as_ref().unwrap().state.lock().unwrap();
        assert!(state.config.replication.enable_placement_rules);
        assert_eq!(state.rules.get_rule("pd", "default").unwrap().count, 5);
    }
    node.shutdown().await.unwrap();
}
