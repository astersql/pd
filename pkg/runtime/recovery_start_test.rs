// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, node_test::fixture};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_endpoint::StorageEndpoint;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
pub(crate) fn http(url: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sS", "--max-time", "6", "-w", "\n%{http_code}"]);
    if let Some(body) = body {
        cmd.args([
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &body.to_string(),
        ]);
    }
    let output = cmd.arg(format!("{url}/pd/api/v1/{path}")).output().unwrap();
    assert!(
        output.status.success(),
        "curl: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    let (body, status) = output.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), serde_json::from_str(body).unwrap())
}
pub(crate) async fn seed(url: &str) -> (PdClient<tonic::transport::Channel>, u64, metapb::Region) {
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let cluster = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let region = metapb::Region {
        id: 100,
        peers: vec![metapb::Peer {
            id: 11,
            store_id: 1,
            ..Default::default()
        }],
        ..Default::default()
    };
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "store-1:20160".into(),
                ..Default::default()
            }),
            region: Some(region.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    for id in 2..=3 {
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id,
                    address: format!("store-{id}:20160"),
                    node_state: metapb::NodeState::Serving as i32,
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
    }
    client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header,
            stats: Some(pdpb::StoreStats {
                store_id: 2,
                capacity: 100000000,
                available: 90000000,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    (client, cluster, region)
}
async fn observe(url: &str, auto: bool) -> (Vec<(u16, Value)>, Value, u64) {
    let (mut client, cluster, region) = seed(url).await;
    let path = "admin/unsafe/remove-failed-stores";
    let inputs = [
        json!({"stores":[]}),
        json!({"stores":[1,9]}),
        json!({"stores":[1,2]}),
        json!({"stores":"bad"}),
        json!({"stores":[2.9]}),
        json!({"stores":[-1]}),
        json!({"stores":[],"timeout":0}),
        json!({"stores":[],"timeout":1.5}),
        json!({"stores":[],"plan-execution-timeout":0}),
        json!({"stores":[],"plan-execution-timeout":5,"plan_execution_timeout":5}),
        json!({"stores":[],"disable-paranoid-check":"true"}),
        json!({"stores":[],"disable-paranoid-check":true,"disable_paranoid_check":true}),
    ];
    let mut results = Vec::new();
    for input in inputs {
        let result = http(url, path, Some(&input));
        assert_ne!(result.0, 200, "{input}");
        results.push(result);
    }
    // Failed admission must leave the disconnected, otherwise valid first store alive.
    assert_ne!(
        client
            .get_store(pdpb::GetStoreRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: cluster,
                    ..Default::default()
                }),
                store_id: 1
            })
            .await
            .unwrap()
            .into_inner()
            .store
            .unwrap()
            .state,
        metapb::StoreState::Tombstone as i32
    );
    let body = if auto {
        json!({"auto-detect":true,"stores":"ignored","plan_execution_timeout":300,"disable_paranoid_check":true})
    } else {
        json!({"stores":[1,1],"timeout":600,"plan-execution-timeout":300,"disable-paranoid-check":true})
    };
    results.push(http(url, path, Some(&body)));
    assert_eq!(
        results.last().unwrap(),
        &(200, json!("Request has been accepted."))
    );
    results.push(http(url, path, Some(&json!({"stores":[]}))));
    let store = client
        .get_store(pdpb::GetStoreRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            store_id: 1,
        })
        .await
        .unwrap()
        .into_inner()
        .store
        .unwrap();
    let split = client
        .ask_split(pdpb::AskSplitRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            region: Some(region),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .error
        .unwrap();
    assert!(split.message.contains("scheduling is halted"), "{split:?}");
    (
        results,
        json!({"store_state":store.state,"node_state":store.node_state,"split_error":split.message}),
        cluster,
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_016_native_start_matches_existing_go_requests_and_halts_split() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    for auto in [false, true] {
        let binary = pinned_go_binary().unwrap();
        let mut go = Server::start(&binary).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let (go_replies, go_state, _) = observe(&go.url, auto).await;
        let mut oracle_client = PdClient::connect(go.url.clone()).await.unwrap();
        let oracle_cluster = oracle_client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let oracle_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[go.url.clone()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let durable_go = StorageEndpoint::new(Arc::new(oracle_store))
            .load_store_meta(&ClusterPaths::new(oracle_cluster), 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            durable_go.state == astersql_pd_core::StoreState::Tombstone,
            !auto
        );
        let show = http(&go.url, "admin/unsafe/remove-failed-stores/show", None);
        assert_eq!(show.0, 200);
        assert_eq!(
            show.1.as_array().unwrap().last().unwrap()["info"],
            format!(
                "Collecting reports from alive stores(0/{})",
                if auto { 3 } else { 2 }
            )
        );
        assert_eq!(
            show.1[0]["info"],
            "Unsafe recovery enters collect report stage"
        );
        assert!(
            show.1[0]["details"]
                .as_array()
                .unwrap()
                .contains(&json!("plan execution timeout 5m0s"))
        );
        let fixture = fixture::Fixture::start();
        let store = Arc::new(fixture.store().await);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 1600,
                        member_id: 16,
                        name: "recovery".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let (rust_replies, rust_state, cluster) = observe(&url, auto).await;
        assert_eq!(rust_replies, go_replies, "auto={auto}");
        assert_eq!(rust_state, go_state, "auto={auto}");
        let (stage, halt, alive, step) =
            node.cluster().unsafe_recovery_start_state().await.unwrap();
        assert_eq!(stage, astersql_pd_unsaferecovery::Stage::CollectReport);
        assert!(halt);
        assert_ne!(step, 0);
        assert_eq!(alive, if auto { vec![1, 2, 3] } else { vec![2, 3] });
        let persisted = StorageEndpoint::new(store)
            .load_store_meta(&ClusterPaths::new(cluster), 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            persisted.state == astersql_pd_core::StoreState::Tombstone,
            !auto
        );
        node.shutdown().await.unwrap();
        go.stop().unwrap();
        println!(
            "Go/Rust {auto}: exact {} HTTP results, matching durable store state and halted AskSplit; initial alive stores {alive:?}",
            rust_replies.len()
        );
    }
}
