// Copyright 2026 AsterSQL.
use crate::{group_merge_oracle_test::start_api_go, meta_group_assignment_oracle_test::http};
use astersql_pd_balancer::RoundRobin;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::KeyspaceGroupManager;
use astersql_pd_storage_domain::{Consistency, KeyspaceGroup, MetaStore, Mutation, Read};
use serde_json::json;
use std::{
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the admitted read-only Go PD binary and official etcd"]
async fn mcs_033_registered_nodes_go_differential() {
    let (go, id) = start_api_go().await;
    let go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let go_paths = ClusterPaths::new(id);
    let listeners = [
        TcpListener::bind("127.0.0.1:0").unwrap(),
        TcpListener::bind("127.0.0.1:0").unwrap(),
    ];
    let addresses: Vec<_> = listeners
        .iter()
        .map(|l| format!("http://{}", l.local_addr().unwrap()))
        .collect();
    let registrations:Vec<_>=addresses.iter().map(|addr|Mutation::put(go_paths.registry_path("tso",addr).as_bytes(),json!({"name":addr,"service-addr":addr,"version":"fixture","git-hash":"native-listener","deploy-path":"","start-timestamp":0}).to_string().as_bytes())).collect();
    assert!(
        go_store
            .transact(vec![], registrations)
            .await
            .unwrap()
            .succeeded
    );
    let rows = go_store
        .read(Read::prefix(
            go_paths.service_path("tso").as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(rows.entries.len(), 2);
    let nodes = Arc::new(RoundRobin::new());
    for row in &rows.entries {
        let value: serde_json::Value = serde_json::from_slice(&row.value).unwrap();
        nodes.put(value["service-addr"].as_str().unwrap().to_owned());
    }
    let fixture = crate::harness::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager =
        KeyspaceGroupManager::new(store, ClusterPaths::new(42)).with_nodes_balancer(nodes);
    let group = KeyspaceGroup {
        id: 1,
        user_kind: "standard".into(),
        keyspaces: Some(vec![42]),
        ..Default::default()
    };
    let url = format!("{}/pd/api/v2/tso/keyspace-groups", go.url);
    assert_eq!(
        http(&url, "POST", json!({"keyspace-groups":[group]})).0,
        200
    );
    manager.create_groups(&[group]).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let response = http(&format!("{url}/1"), "PATCH", json!({"nodes":addresses}));
        if response.0 == 200 {
            break;
        }
        assert_eq!(response.1, json!("node does not exist"), "{response:?}");
        assert!(
            Instant::now() < deadline,
            "registry watch did not publish candidates"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(manager.get_nodes_count(), 2);
    for address in &addresses {
        assert!(manager.registered_node(address).is_some());
    }
    manager.set_nodes(1, &addresses).await.unwrap();
    let alias = addresses[0].replacen("http://", "https://", 1);
    let escaped = alias.replace(':', "%3A").replace('/', "%2F");
    assert_eq!(
        http(
            &format!("{url}/1/{escaped}"),
            "PATCH",
            json!({"priority":-7})
        )
        .0,
        200
    );
    manager.set_priority(1, &alias, -7).await.unwrap();
    let go_group = http(&format!("{url}/1"), "GET", json!(null));
    assert_eq!(go_group.0, 200);
    let actual: KeyspaceGroup = serde_json::from_value(go_group.1.clone()).unwrap();
    let rust = manager.get_group(1).await.unwrap().unwrap();
    assert_eq!(actual, rust);
    let invalid = http(
        &format!("{url}/1"),
        "PATCH",
        json!({"nodes":["http://absent:1234"]}),
    );
    assert_eq!(invalid, (400, json!("node does not exist")));
    assert!(manager.registered_node("http://absent:1234").is_none());
    assert_eq!(manager.get_group(1).await.unwrap().unwrap(), rust);
    assert_eq!(http(&format!("{url}/1"), "GET", json!(null)), go_group);
    if let Some(path) = std::env::var_os("MCS_033_ORACLE_REPORT") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"registrations":rows.entries.iter().map(|r|String::from_utf8_lossy(&r.value).into_owned()).collect::<Vec<_>>(),"go_group":go_group,"rust_group":rust,"invalid":invalid})).unwrap()).unwrap();
    }
}
