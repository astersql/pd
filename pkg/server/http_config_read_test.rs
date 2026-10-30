// Copyright 2026 AsterSQL.
use crate::http_member_test::request;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_047_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 747,
                member_id: 1,
                name: "config".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let full = observe(&url);
    assert_eq!(full["replication"]["enable-placement-rules"], "false");
    let defaults = request(&url, "GET", "/pd/api/v1/config/default", "");
    assert_eq!(defaults.0, 200);
    assert_eq!(defaults.1["replication"]["enable-placement-rules"], "true");
    assert_eq!(defaults.1["schedule"]["max-merge-region-keys"], 0);
    assert_eq!(full["schedule"]["max-merge-region-keys"], 540000);
    for key in [
        "disable-raft-learner",
        "disable-remove-down-replica",
        "store-balance-rate",
    ] {
        assert!(full["schedule"].get(key).is_none());
    }
    assert!(full.get("log-file").is_none());
    assert!(full.get("log-level").is_none());
    let response = (200, full);
    node.shutdown().await.unwrap();
    assert_eq!(
        response.0, 200,
        "missing configuration read route: {response:?}"
    );
    assert_eq!(response.1["name"], "config");
}

fn observe(url: &str) -> serde_json::Value {
    let (status, full) = request(url, "GET", "/pd/api/v1/config", "");
    assert_eq!(status, 200);
    for (path, section) in [
        ("schedule", "schedule"),
        ("pd-server", "pd-server"),
        ("replicate", "replication"),
        ("label-property", "label-property"),
        ("cluster-version", "cluster-version"),
        ("replication-mode", "replication-mode"),
    ] {
        assert_eq!(
            request(
                url,
                "GET",
                &format!("/pd/api/v1/config/{path}?unused=ignored"),
                ""
            ),
            (200, full[section].clone())
        );
    }
    full
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_047_differential() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use serde_json::json;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let defaults = request(&go.url, "GET", "/pd/api/v1/config/default", "");
    let initial = observe(&go.url);
    use astersql_pd_proto::{metapb, pdpb};
    let mut client = pdpb::pd_client::PdClient::connect(go.url.clone())
        .await
        .unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    let update = request(&go.url, "POST", "/pd/api/v1/config", &json!({"replication.max-replicas":5,"schedule.max-merge-region-size":37,"schedule.max-merge-region-keys":0,"pd-server.flow-round-by-digit":4}).to_string());
    assert_eq!(update.0, 200, "{update:?}");
    let changed = observe(&go.url);
    assert_eq!(changed["schedule"]["max-merge-region-keys"], 370000);
    assert_eq!(changed["replication"]["max-replicas"], 5);
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(7471);
    let address = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap()
    };
    for expected in [initial, changed] {
        // Feed the same dynamic configuration, with zero merge-keys to exercise
        // the HTTP effective-value projection rather than precomputing it.
        let mut persisted = expected.clone();
        persisted["schedule"]["max-merge-region-keys"] = json!(0);
        endpoint.save_config(&paths, &persisted).await.unwrap();
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 7471,
                        member_id: 1,
                        name: "config".into(),
                        client_url: url.clone(),
                        peer_url: "http://peer".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let mut progress = node.cluster().watch_schedule();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while progress.borrow().phase
                != astersql_pd_runtime::cluster_host::SchedulePhase::Running
            {
                progress.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let before = store
            .read(Read::range(
                paths.config_path().as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();

        let rust_defaults = request(&url, "GET", "/pd/api/v1/config/default", "");
        let actual = observe(&url);
        println!(
            "Go defaults: {}\nRust defaults: {}",
            defaults.1, rust_defaults.1
        );
        assert_eq!(rust_defaults, defaults);
        assert_eq!(actual["name"], "config");
        assert_eq!(actual["client-urls"], url);
        assert_eq!(
            actual.as_object().unwrap().keys().collect::<Vec<_>>(),
            expected.as_object().unwrap().keys().collect::<Vec<_>>()
        );
        for section in [
            "schedule",
            "replication",
            "pd-server",
            "label-property",
            "cluster-version",
            "replication-mode",
            "keyspace",
            "micro-service",
        ] {
            assert_eq!(actual[section], expected[section], "section {section}");
        }
        assert_eq!(
            before.entries,
            store
                .read(Read::range(
                    paths.config_path().as_bytes(),
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
        );
        node.shutdown().await.unwrap();
    }
    go.stop().unwrap();
}
