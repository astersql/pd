// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{
    BoundType, CreateKeyspaceByIdRequest, Kernel, KeyspaceCreateConfig, KeyspaceCreateResources,
    KeyspaceManager, KeyspaceRegionSplit, KeyspaceSplitConfig,
};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::{Consistency, Keyspace, MetaStore, Read, StorageEndpoint};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
pub(crate) fn dto(meta: &Keyspace) -> Value {
    let mut value = json!({"id":meta.id(),"name":meta.name,"state":match meta.state{0=>"ENABLED",1=>"DISABLED",2=>"ARCHIVED",3=>"TOMBSTONE",_=>"UNKNOWN"},"created_at":meta.created_at,"state_changed_at":meta.state_changed_at});
    if !meta.config.is_empty() {
        value["config"] = json!(meta.config);
    }
    value
}
async fn physical(store: &dyn MetaStore, cluster: u64, id: u32, name: &str) -> Value {
    let paths = ClusterPaths::new(cluster);
    let mut entries = vec![];
    for key in [paths.keyspace_id_path(name), paths.keyspace_meta_path(id)] {
        let s = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        entries.push(json!({"key":key,"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>() }));
    }
    json!(entries)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_037_keyspace_create_go_differential() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    let config = http(
        &format!("{}/pd/api/v1/config", go.url),
        "POST",
        json!({"keyspace.wait-region-split":false}),
    );
    assert_eq!(config.0, 200);
    let mut fixture = crate::harness::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(
            StorageEndpoint::new(store.clone()),
            ClusterPaths::new(42),
            0,
        )
        .await
        .unwrap(),
    ));
    let split = Arc::new(KeyspaceRegionSplit::new(
        Some(labeler),
        None,
        Arc::new(RwLock::new(KeyspaceSplitConfig::default())),
    ));
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(ClusterPaths::new(42));
    let member = astersql_pd_member::Member::bootstrap(
        store.clone(),
        CancellationToken::new(),
        42,
        1,
        b"create37".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(30).await.unwrap());
    let allocator = Arc::new(
        astersql_pd_id::Allocator::with_namespace(
            store.clone(),
            leader.clone(),
            CancellationToken::new(),
            100,
            astersql_pd_id::AllocatorNamespace::Keyspace,
        )
        .unwrap(),
    );
    let m = KeyspaceManager::with_resources(
        storage,
        KeyspaceCreateResources {
            allocator: Some(allocator),
            kernel: Kernel::Classic,
            splitter: Some(split),
            config: Arc::new(RwLock::new(KeyspaceCreateConfig {
                wait_region_split: false,
                bound: BoundType::Txn,
            })),
            ..Default::default()
        },
    );
    let go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let base = format!("{}/pd/api/v2/keyspaces", go.url);
    let mut receipts = vec![];
    for name in ["allocated_a", "allocated_b"] {
        let result = http(&base, "POST", json!({"name":name}));
        assert_eq!(result.0, 200, "{result:?}");
        let meta = m
            .create(
                &CancellationToken::new(),
                astersql_pd_keyspace::CreateKeyspaceRequest {
                    name: name.into(),
                    create_time: result.1["created_at"].as_i64().unwrap(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(result.1, dto(&meta));
        let repeated = http(&base, "POST", json!({"name":name}));
        let error = m
            .create(
                &CancellationToken::new(),
                astersql_pd_keyspace::CreateKeyspaceRequest {
                    name: name.into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(repeated, (500, json!(error.to_string())));
        receipts.push(json!({"case":"allocated_id","http":result,"duplicate":repeated}));
    }
    for (id, name) in [(80, "created80"), (82, "created82"), (84, "created84")] {
        let input = json!({"id":id,"name":name,"config":{"keep":"value","meta_service_group_addrs":"forged","meta_service_group_id":"forged","tso_keyspace_group_id":"999"}});
        let result = http(&format!("{base}/id"), "POST", input.clone());
        assert_eq!(result.0, 200, "{result:?}");
        let meta = m
            .create_by_id(
                &CancellationToken::new(),
                CreateKeyspaceByIdRequest {
                    id: Some(id),
                    name: name.into(),
                    config: serde_json::from_value::<HashMap<String, String>>(
                        input["config"].clone(),
                    )
                    .unwrap(),
                    create_time: result.1["created_at"].as_i64().unwrap(),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.1, dto(&meta));
        for route in [name.to_string(), format!("id/{id}")] {
            assert_eq!(
                http(&format!("{base}/{route}"), "GET", Value::Null),
                (200, dto(&meta))
            );
        }
        assert_eq!(m.load_by_name(name).await.unwrap(), meta);
        assert_eq!(m.load_by_id(id).await.unwrap(), meta);
        let g = physical(&go_store, cluster, id, name).await;
        let r = physical(store.as_ref(), 42, id, name).await;
        // SaveNew commits name and disabled meta together; enabling updates only the meta revision.
        for value in [&g, &r] {
            assert_eq!(
                value[0]["entries"][0]["create_revision"],
                value[1]["entries"][0]["create_revision"]
            );
            assert!(
                value[1]["entries"][0]["mod_revision"].as_i64().unwrap()
                    > value[0]["entries"][0]["mod_revision"].as_i64().unwrap()
            );
        }
        // The adapter retains the exact pinned protobuf format. A single config entry has no map ordering ambiguity.
        assert_eq!(g[0]["entries"][0]["bytes"], r[0]["entries"][0]["bytes"]);
        assert_eq!(g[1]["entries"][0]["bytes"], r[1]["entries"][0]["bytes"]);
        receipts.push(json!({"case":"create_by_id","input":input,"http":result,"go":g,"rust":r}));
    }
    for (id, name) in [(99, "created80"), (80, "another")] {
        let result = http(&format!("{base}/id"), "POST", json!({"id":id,"name":name}));
        let error = m
            .create_by_id(
                &CancellationToken::new(),
                CreateKeyspaceByIdRequest {
                    id: Some(id),
                    name: name.into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(result, (500, json!(error.to_string())));
        receipts.push(json!({"case":"conflict","http":result,"rust_error":error.to_string()}));
    }
    for input in [
        json!({"name":"no_id"}),
        json!({"id":90,"name":""}),
        json!({"id":90,"name":"bad.name"}),
    ] {
        let result = http(&format!("{base}/id"), "POST", input.clone());
        let error = m
            .create_by_id(
                &CancellationToken::new(),
                CreateKeyspaceByIdRequest {
                    id: input["id"].as_u64().map(|v| v as u32),
                    name: input["name"].as_str().unwrap().into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(result, (500, json!(error.to_string())));
        receipts.push(json!({"case":"invalid","input":input,"http":result}));
    }
    let first = http(&format!("{base}?page_token=80&limit=1"), "GET", Value::Null);
    assert_eq!(first.0, 200);
    let pair = m.load_range(80, 2).await.unwrap();
    assert_eq!(first.1["keyspaces"], json!([dto(&pair[0])]));
    assert_eq!(first.1["next_page_token"], pair[1].id().to_string());
    let tail = http(&format!("{base}?page_token=82&limit=0"), "GET", Value::Null);
    assert_eq!(tail.0, 200);
    assert_eq!(
        tail.1["keyspaces"],
        json!(
            m.load_range(82, 0)
                .await
                .unwrap()
                .iter()
                .map(dto)
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(tail.1["next_page_token"], "");
    receipts.push(json!({"case":"pagination","first":first,"tail":tail}));
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let restored = fixture.store().await;
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let restored_go =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let g = physical(&restored_go, cluster, 80, "created80").await;
    let r = physical(&restored, 42, 80, "created80").await;
    assert_eq!(g[1]["entries"][0]["bytes"], r[1]["entries"][0]["bytes"]);
    receipts.push(json!({"case":"same_wal","go":g,"rust":r}));
    leader.resign().await.unwrap();
    if let Some(path) = std::env::var_os("MCS_037_ORACLE_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"cluster_id":cluster,"cases":receipts})).unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_037_group_config_go_differential() {
    let (go, cluster) = crate::group_merge_oracle_test::start_api_go().await;
    assert_eq!(
        http(
            &format!("{}/pd/api/v1/config", go.url),
            "POST",
            json!({"keyspace.wait-region-split":false})
        )
        .0,
        200
    );
    let fixture = crate::harness::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let initial = http(
        &format!("{}/pd/api/v2/tso/keyspace-groups/0", go.url),
        "GET",
        Value::Null,
    );
    assert_eq!(initial.0, 200);
    let initial: astersql_pd_storage_domain::KeyspaceGroup =
        serde_json::from_value(initial.1).unwrap();
    let groups = Arc::new(astersql_pd_keyspace::KeyspaceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
    ));
    groups.create_groups(&[initial]).await.unwrap();
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(
            StorageEndpoint::new(store.clone()),
            ClusterPaths::new(42),
            0,
        )
        .await
        .unwrap(),
    ));
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(ClusterPaths::new(42));
    let m = KeyspaceManager::with_resources(
        storage,
        KeyspaceCreateResources {
            kernel: Kernel::Classic,
            groups: Some(groups.clone()),
            splitter: Some(Arc::new(KeyspaceRegionSplit::new(
                Some(labeler),
                None,
                Arc::new(RwLock::new(KeyspaceSplitConfig::default())),
            ))),
            config: Arc::new(RwLock::new(KeyspaceCreateConfig {
                wait_region_split: false,
                bound: BoundType::Txn,
            })),
            ..Default::default()
        },
    );
    let input = json!({"id":100,"name":"group_config","config":{"user_kind":"unknown","keep":"value","tso_keyspace_group_id":"999","meta_service_group_id":"forged","meta_service_group_addrs":"forged","region_bound_type":"raw"}});
    let result = http(
        &format!("{}/pd/api/v2/keyspaces/id", go.url),
        "POST",
        input.clone(),
    );
    assert_eq!(result.0, 200, "{result:?}");
    let meta = m
        .create_by_id(
            &CancellationToken::new(),
            CreateKeyspaceByIdRequest {
                id: Some(100),
                name: "group_config".into(),
                config: serde_json::from_value(input["config"].clone()).unwrap(),
                create_time: result.1["created_at"].as_i64().unwrap(),
            },
        )
        .await
        .unwrap();
    assert_eq!(result.1, dto(&meta));
    let go_group = http(
        &format!("{}/pd/api/v2/tso/keyspace-groups/0", go.url),
        "GET",
        Value::Null,
    );
    assert_eq!(go_group.0, 200);
    assert_eq!(
        go_group.1,
        serde_json::to_value(groups.get_group(0).await.unwrap().unwrap()).unwrap()
    );
    let go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let g = physical(&go_store, cluster, 100, "group_config").await;
    let r = physical(store.as_ref(), 42, 100, "group_config").await;
    let adapter = astersql_pd_storage_endpoint::StorageEndpoint::new(Arc::new(go_store));
    let s = adapter.keyspace_storage(ClusterPaths::new(cluster));
    let mut txn = astersql_pd_storage_domain::Base::new(s.store.as_ref()).begin_txn();
    assert_eq!(
        s.load_keyspace_meta(&mut txn, &s.paths, 100).await.unwrap(),
        Some(meta)
    );
    if let Some(path) = std::env::var_os("MCS_037_API_ORACLE_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"input":input,"http":result,"group":go_group,"go":g,"rust":r}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
