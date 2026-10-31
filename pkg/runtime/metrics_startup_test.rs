// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, etcd_diagnostics::EtcdMemberDiagnostics};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_registration_precedes_diagnostic_binding() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut client = etcd_client::Client::connect([fixture.endpoint.clone()], None)
        .await
        .unwrap();
    let member = client.status().await.unwrap().header().unwrap().member_id();
    let diagnostics = Arc::new(
        EtcdMemberDiagnostics::connect(fixture.endpoint.clone(), member, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let registry = Arc::new(prometheus::Registry::new());
    let conflict = prometheus::GaugeVec::new(
        prometheus::Opts::new(
            "pd_server_info",
            "Indicate the pd server info, and the value is the start timestamp (s).",
        ),
        &["version", "hash"],
    )
    .unwrap();
    conflict.with_label_values(&["host", "host"]).set(91.0);
    registry.register(Box::new(conflict)).unwrap();
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let result = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_etcd_member_diagnostics(diagnostics)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 8687,
                member_id: 86,
                name: "collision".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        "registration failure must precede persistent diagnostic binding"
    );
    assert_eq!(registry.gather().len(), 1);
    assert_eq!(
        registry.gather()[0].get_metric()[0].get_gauge().get_value(),
        91.0
    );
    let _listener = tokio::net::TcpListener::bind(address).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_bound_consensus_metrics() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut client = etcd_client::Client::connect([fixture.endpoint.clone()], None)
        .await
        .unwrap();
    let member = client.status().await.unwrap().header().unwrap().member_id();
    let diagnostics = Arc::new(
        EtcdMemberDiagnostics::connect(fixture.endpoint.clone(), member, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let registry = Arc::new(prometheus::Registry::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_etcd_member_diagnostics(diagnostics)
        .start(
            store,
            NodeConfig {
                cluster_id: 8699,
                member_id: 86,
                name: "consensus-metrics".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(family) = registry
            .gather()
            .iter()
            .find(|f| f.name() == "pd_server_etcd_state")
        {
            assert_eq!(
                family.get_field_type(),
                prometheus::proto::MetricType::GAUGE
            );
            assert_eq!(family.help(), "Etcd raft states.");
            assert!(
                family
                    .get_metric()
                    .iter()
                    .all(|m| m.get_label()[0].name() == "type")
            );
            let values: std::collections::BTreeMap<_, _> = family
                .get_metric()
                .iter()
                .map(|m| {
                    (
                        m.get_label()[0].value().to_owned(),
                        m.get_gauge().get_value(),
                    )
                })
                .collect();
            assert!(values["term"] >= 1.0);
            assert!(values["committedIndex"] >= values["appliedIndex"]);
            assert!(values["appliedIndex"] > 0.0);
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "bound consensus metrics were not collected"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(registry.gather().is_empty());
}
