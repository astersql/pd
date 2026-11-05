// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{
    BoundType, Kernel, KeyspaceCreateConfig, KeyspaceCreateResources, KeyspaceManager,
    KeyspaceRegionSplit, KeyspaceSplitConfig, PreallocationOutcome,
};
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read, StorageEndpoint};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
struct ConfigFile(PathBuf);
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn manager(
    store: Arc<dyn MetaStore>,
    allocator: Option<Arc<astersql_pd_id::Allocator>>,
) -> KeyspaceManager {
    let p = ClusterPaths::new(42);
    let labeler = Arc::new(Mutex::new(
        RegionLabeler::load(StorageEndpoint::new(store.clone()), p.clone(), 0)
            .await
            .unwrap(),
    ));
    KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(p),
        KeyspaceCreateResources {
            kernel: Kernel::Classic,
            allocator,
            splitter: Some(Arc::new(KeyspaceRegionSplit::new(
                Some(labeler),
                None,
                Arc::new(RwLock::new(KeyspaceSplitConfig::default())),
            ))),
            config: Arc::new(RwLock::new(KeyspaceCreateConfig {
                wait_region_split: false,
                bound: BoundType::Raw,
            })),
            ..Default::default()
        },
    )
}
async fn records(store: &dyn MetaStore, p: &ClusterPaths, items: &[(u32, String)]) -> Value {
    let mut result = vec![];
    for (id, name) in items {
        let mut values = vec![];
        for key in [
            p.keyspace_meta_path(*id),
            p.keyspace_id_path(name),
            p.region_label_key_path(&format!("keyspaces/{id}")),
        ] {
            let s = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            values.push(json!({"key":key,"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>()}));
        }
        result.push(json!({"id":id,"records":values}));
    }
    json!(result)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_041_bootstrap_preallocation_go_differential() {
    let config = ConfigFile(
        std::env::temp_dir().join(format!("pd-bootstrap41-{}.toml", std::process::id())),
    );
    let config_text = "[keyspace]\npre-alloc = ['pre_one', 'pre_two']\nwait-region-split = false\n[pd-server]\nkey-type = 'raw'\n";
    std::fs::write(&config.0, config_text).unwrap();
    let args = ["--config", config.0.to_str().unwrap()];
    let (mut go, cluster) =
        crate::group_merge_oracle_test::start_bootstrapped_go_with_args(None, &args).await;
    let base = format!("{}/pd/api/v2/keyspaces", go.url);
    let mut allocated = vec![];
    for name in ["pre_one", "pre_two"] {
        let value = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let response = http(&format!("{base}/{name}"), "GET", Value::Null);
                if response.0 == 200 && response.1["state"] == "ENABLED" {
                    break response.1;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        allocated.push(value);
    }
    allocated.sort_by_key(|meta| meta["id"].as_u64().unwrap());
    let default = http(&format!("{base}/DEFAULT"), "GET", Value::Null);
    assert_eq!(default.0, 200);
    assert_eq!(default.1["config"]["region_bound_type"], "raw");
    let mut fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let member = astersql_pd_member::Member::bootstrap(
        rs.clone(),
        CancellationToken::new(),
        42,
        1,
        b"bootstrap41".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(120).await.unwrap());
    let allocator = Arc::new(
        astersql_pd_id::Allocator::with_namespace(
            rs.clone(),
            leader.clone(),
            CancellationToken::new(),
            100,
            astersql_pd_id::AllocatorNamespace::Keyspace,
        )
        .unwrap(),
    );
    let m = manager(rs.clone(), Some(allocator)).await;
    let work = m
        .bootstrap_with_preallocations(
            default.1["created_at"].as_i64().unwrap(),
            &["pre_one".into(), "pre_two".into()],
        )
        .await
        .unwrap();
    assert_eq!(work.preallocations.len(), 2);
    assert_eq!(
        crate::keyspace_create_oracle_test::dto(&m.load_by_id(0).await.unwrap()),
        default.1
    );
    // Go launches independent workers. Drive Rust in the observed allocation order to compare the same valid linearization.
    for actual in &allocated {
        let outcome = m
            .preallocate(
                &CancellationToken::new(),
                actual["name"].as_str().unwrap(),
                actual["created_at"].as_i64().unwrap(),
            )
            .await
            .unwrap();
        let PreallocationOutcome::Created(meta) = outcome else {
            panic!("fresh preallocation must create metadata")
        };
        assert_eq!(crate::keyspace_create_oracle_test::dto(&meta), *actual);
    }
    let mut items = vec![(0, "DEFAULT".into())];
    items.extend(allocated.iter().map(|m| {
        (
            m["id"].as_u64().unwrap() as u32,
            m["name"].as_str().unwrap().into(),
        )
    }));
    let gs =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let gp = ClusterPaths::new(cluster);
    let rp = ClusterPaths::new(42);
    let before_go = records(&gs, &gp, &items).await;
    let before_rust = records(rs.as_ref(), &rp, &items).await;
    for i in 0..items.len() {
        for j in 0..2 {
            assert_eq!(
                before_go[i]["records"][j]["entries"][0]["bytes"],
                before_rust[i]["records"][j]["entries"][0]["bytes"]
            );
        }
        let g: Vec<u8> =
            serde_json::from_value(before_go[i]["records"][2]["entries"][0]["bytes"].clone())
                .unwrap();
        let r: Vec<u8> =
            serde_json::from_value(before_rust[i]["records"][2]["entries"][0]["bytes"].clone())
                .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&g).unwrap(),
            serde_json::from_slice::<Value>(&r).unwrap()
        );
    }
    leader.resign().await.unwrap();
    // A normal restart does not call keyspace.Manager.Bootstrap again.
    // Recover only this fixture's cluster marker, preserving all keyspace records, to re-enter the real Bootstrap RPC.
    astersql_pd_storage_domain::Base::new(&gs)
        .remove(gp.cluster_path().as_bytes())
        .await
        .unwrap();
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let fresh: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let restored = manager(fresh.clone(), None).await;
    restored
        .bootstrap(default.1["created_at"].as_i64().unwrap() + 1000)
        .await
        .unwrap();
    for item in &allocated {
        assert!(matches!(
            restored
                .preallocate(&CancellationToken::new(), item["name"].as_str().unwrap(), 0)
                .await
                .unwrap(),
            PreallocationOutcome::AlreadyExists
        ));
    }
    let after_rust = records(fresh.as_ref(), &rp, &items).await;
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &args)
        .unwrap();
    go.ready().unwrap();
    let fresh_go =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let mut pd = astersql_pd_proto::pdpb::pd_client::PdClient::connect(go.url.clone())
        .await
        .unwrap();
    let bootstrap = pd
        .bootstrap(astersql_pd_proto::pdpb::BootstrapRequest {
            header: Some(astersql_pd_proto::pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            store: Some(astersql_pd_proto::metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
                ..Default::default()
            }),
            region: Some(astersql_pd_proto::metapb::Region {
                id: 2,
                peers: vec![astersql_pd_proto::metapb::Peer {
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
    let rebootstrap_header = bootstrap.header.unwrap();
    assert!(rebootstrap_header.error.is_none(), "{rebootstrap_header:?}");
    let after_go = records(&fresh_go, &gp, &items).await;
    for i in 0..items.len() {
        for j in 0..2 {
            assert_eq!(
                before_go[i]["records"][j]["entries"],
                after_go[i]["records"][j]["entries"]
            );
            assert_eq!(
                before_rust[i]["records"][j]["entries"],
                after_rust[i]["records"][j]["entries"]
            );
        }
    }
    assert!(
        after_go[0]["records"][2]["entries"][0]["mod_revision"]
            .as_i64()
            .unwrap()
            > before_go[0]["records"][2]["entries"][0]["mod_revision"]
                .as_i64()
                .unwrap()
    );
    assert!(
        after_rust[0]["records"][2]["entries"][0]["mod_revision"]
            .as_i64()
            .unwrap()
            > before_rust[0]["records"][2]["entries"][0]["mod_revision"]
                .as_i64()
                .unwrap()
    );
    if let Some(path) = std::env::var_os("MCS_041_ORACLE_REPORT") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"config":config_text,"default":default,"preallocations_in_observed_order":allocated,"before_go":before_go,"before_rust":before_rust,"after_go":after_go,"after_rust":after_rust,"rebootstrap":{"fixture_cluster_marker_removed":gp.cluster_path(),"header_cluster_id":rebootstrap_header.cluster_id}})).unwrap()).unwrap();
    }
}
