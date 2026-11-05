// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{
    ConfigMutation, ConfigOp, KeyspaceCreateResources, KeyspaceGroupManager, KeyspaceManager,
};
use astersql_pd_storage_domain::{Keyspace, KeyspaceGroup, KeyspaceIdentity};
use std::{collections::BTreeMap, sync::Arc};
fn domain_meta(value: &Value) -> Keyspace {
    Keyspace {
        identity: Some(KeyspaceIdentity::Id(value["id"].as_u64().unwrap() as u32)),
        name: value["name"].as_str().unwrap().into(),
        state: 0,
        created_at: value["created_at"].as_i64().unwrap(),
        state_changed_at: value["state_changed_at"].as_i64().unwrap(),
        config: serde_json::from_value(value["config"].clone()).unwrap_or_default(),
    }
}
fn mutations(input: &Value) -> Vec<ConfigMutation> {
    input
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| ConfigMutation {
            op: if value.is_null() {
                ConfigOp::Delete
            } else {
                ConfigOp::Put
            },
            key: key.clone(),
            value: value.as_str().unwrap_or("").into(),
        })
        .collect()
}

use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use serde_json::{Value, json};
use std::{process::Command, time::Duration};
async fn snapshot(store: &dyn MetaStore, cluster: u64) -> Value {
    let p = ClusterPaths::new(cluster);
    let mut result = vec![];
    for key in [
        p.keyspace_meta_path(100),
        p.keyspace_group_id_path(1),
        p.keyspace_group_id_path(2),
    ] {
        let s = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        result.push(json!({"key":key,"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>() }));
    }
    json!(result)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary; observes actual fault semantics before implementing Rust"]
async fn mcs_038_go_commit_failure_state() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_api_go().await;
    assert_eq!(
        http(
            &format!("{}/pd/api/v1/config", go.url),
            "POST",
            json!({"keyspace.wait-region-split":false})
        )
        .0,
        200
    );
    let groups = format!("{}/pd/api/v2/tso/keyspace-groups", go.url);
    assert_eq!(
        http(
            &groups,
            "POST",
            json!({"keyspace-groups":[{"id":1,"user-kind":"basic","keyspaces":[]}]})
        )
        .0,
        200
    );
    let spaces = format!("{}/pd/api/v2/keyspaces", go.url);
    let created = http(
        &format!("{spaces}/id"),
        "POST",
        json!({"id":100,"name":"commit_fault"}),
    );
    assert_eq!(created.0, 200, "{created:?}");
    assert_eq!(created.1["config"]["tso_keyspace_group_id"], "1");
    assert_eq!(
        http(
            &groups,
            "POST",
            json!({"keyspace-groups":[{"id":2,"user-kind":"basic","keyspaces":[]}]})
        )
        .0,
        200
    );
    let store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let mut fixture = crate::harness::fixture::Fixture::start();
    let rust_store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let rust_groups = Arc::new(KeyspaceGroupManager::new(
        rust_store.clone(),
        ClusterPaths::new(42),
    ));
    let mut shared_groups = vec![];
    for id in [1, 2] {
        let response = http(&format!("{groups}/{id}"), "GET", Value::Null);
        assert_eq!(response.0, 200);
        shared_groups.push(serde_json::from_value::<KeyspaceGroup>(response.1).unwrap());
    }
    rust_groups.create_groups(&shared_groups).await.unwrap();
    let m = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(rust_store.clone())
            .keyspace_storage(ClusterPaths::new(42)),
        KeyspaceCreateResources {
            groups: Some(rust_groups.clone()),
            ..Default::default()
        },
    );
    m.save_new(&domain_meta(&created.1)).await.unwrap();
    let rust_before = snapshot(rust_store.as_ref(), 42).await;
    let before = snapshot(&store, cluster).await;
    let input = json!({"config":{"tso_keyspace_group_id":"2","oversized":"x".repeat(2*1024*1024)}});
    let path = go.dir.join("config-fault-input.json");
    std::fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "40",
            "-X",
            "PATCH",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &format!("@{}", path.display()),
            "-w",
            "\n%{http_code}",
            &format!("{spaces}/commit_fault/config"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    let (body, status) = output.rsplit_once('\n').unwrap();
    assert_eq!(status, "500", "{body}");
    assert!(
        body.contains("too large") || body.contains("larger than max"),
        "{body}"
    );
    let rust_error = m
        .update_config("commit_fault", &mutations(&input["config"]))
        .await
        .unwrap_err();
    let rust_after = snapshot(rust_store.as_ref(), 42).await;
    assert_eq!(m.load_by_id(100).await.unwrap(), domain_meta(&created.1));
    assert_eq!(rust_before[0]["entries"], rust_after[0]["entries"]);
    assert_eq!(
        rust_groups.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![])
    );
    assert_eq!(
        rust_groups.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![100])
    );
    let after = snapshot(&store, cluster).await;
    let meta = http(&format!("{spaces}/id/100"), "GET", Value::Null);
    let source = http(&format!("{groups}/1"), "GET", Value::Null);
    let target = http(&format!("{groups}/2"), "GET", Value::Null);
    assert_eq!(meta, created);
    assert_eq!(source.0, 200);
    assert_eq!(target.0, 200);
    assert_eq!(source.1["keyspaces"], json!([]));
    assert_eq!(target.1["keyspaces"], json!([100]));
    assert_eq!(before[0]["entries"], after[0]["entries"]);
    assert_ne!(before[1]["entries"], after[1]["entries"]);
    assert_ne!(before[2]["entries"], after[2]["entries"]);
    go.stop().unwrap();
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let restarted =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let recovered = snapshot(&restarted, cluster).await;
    for i in 0..3 {
        assert_eq!(after[i]["entries"], recovered[i]["entries"]);
    }
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let rust_recovered = snapshot(&fixture.store().await, 42).await;
    for i in 0..3 {
        assert_eq!(rust_after[i]["entries"], rust_recovered[i]["entries"]);
    }
    let report = json!({"cluster_id":cluster,"patch":{"new_group":"2","oversized_value_bytes":2*1024*1024},"http":{"status":status,"body":body},"before":before,"after":after,"recovered":recovered,"meta":meta,"source":source,"target":target,"plan_requires":"transaction failure retains original state","observed":"metadata retained group 1 while membership durably moved to group 2", "rust":{"error":rust_error.to_string(),"before":rust_before,"after":rust_after,"recovered":rust_recovered}});
    if let Some(path) = std::env::var_os("MCS_038_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!(
        "real Go commit failure leaves metadata group=1 and durable membership group=2; HTTP={status} {body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_038_merge_patch_preconditions_go_differential() {
    let (go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    assert_eq!(
        http(
            &format!("{}/pd/api/v1/config", go.url),
            "POST",
            json!({"keyspace.wait-region-split":false})
        )
        .0,
        200
    );
    let spaces = format!("{}/pd/api/v2/keyspaces", go.url);
    let initial = http(
        &format!("{spaces}/id"),
        "POST",
        json!({"id":100,"name":"config_patch","config":{"a":"one","empty":""}}),
    );
    assert_eq!(initial.0, 200, "{initial:?}");
    let f = crate::harness::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let mgm = Arc::new(astersql_pd_keyspace::MetaServiceGroupManager::new(
        store.clone(),
        ClusterPaths::new(42),
        BTreeMap::new(),
    ));
    let manager = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(ClusterPaths::new(42)),
        KeyspaceCreateResources {
            meta_groups: Some(mgm),
            ..Default::default()
        },
    );
    manager.save_new(&domain_meta(&initial.1)).await.unwrap();
    let go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let mut receipts = vec![];
    for (input, expected_status) in [
        (
            json!({"config":{"a":"two","new":"yes"},"preconditions":{"a":"one","missing":null,"empty":""}}),
            200,
        ),
        (
            json!({"config":{"a":"bad"},"preconditions":{"a":null}}),
            409,
        ),
        (
            json!({"config":{"a":"bad"},"preconditions":{"missing":"value"}}),
            409,
        ),
        (
            json!({"config":{"a":"bad"},"preconditions":{"a":"wrong"}}),
            409,
        ),
        (
            json!({"config":{"new":null,"a":"three"},"preconditions":{"a":"two"}}),
            200,
        ),
        (json!({"config":{"meta_service_group_addrs":"forged"}}), 200),
        (json!({"config":{"meta_service_group_id":"unknown"}}), 400),
        (json!({"config":{}}), 200),
    ] {
        let before_go = snapshot(&go_store, cluster).await;
        let before_rust = snapshot(store.as_ref(), 42).await;
        let response = http(
            &format!("{spaces}/config_patch/config"),
            "PATCH",
            input.clone(),
        );
        assert_eq!(response.0, expected_status, "{response:?}");
        let preconditions = if input["preconditions"].is_null() {
            BTreeMap::new()
        } else {
            serde_json::from_value::<BTreeMap<String, Option<String>>>(
                input["preconditions"].clone(),
            )
            .unwrap()
        };
        let result = manager
            .update_config_with_preconditions(
                "config_patch",
                &mutations(&input["config"]),
                &preconditions,
            )
            .await;
        match result {
            Ok(meta) => {
                assert_eq!(expected_status, 200);
                assert_eq!(response.1, crate::keyspace_create_oracle_test::dto(&meta));
            }
            Err(error) => {
                assert_ne!(expected_status, 200);
                assert_eq!(response.1, json!(error.to_string()));
            }
        }
        let after_go = snapshot(&go_store, cluster).await;
        let after_rust = snapshot(store.as_ref(), 42).await;
        if expected_status != 200 {
            assert_eq!(before_go[0]["entries"], after_go[0]["entries"]);
            assert_eq!(before_rust[0]["entries"], after_rust[0]["entries"]);
        } else {
            for (before, after) in [(&before_go, &after_go), (&before_rust, &after_rust)] {
                assert!(
                    after[0]["entries"][0]["mod_revision"].as_i64().unwrap()
                        > before[0]["entries"][0]["mod_revision"].as_i64().unwrap()
                );
            }
        }
        let current = http(&format!("{spaces}/id/100"), "GET", Value::Null);
        assert_eq!(
            current,
            (
                200,
                crate::keyspace_create_oracle_test::dto(&manager.load_by_id(100).await.unwrap())
            )
        );
        receipts.push(json!({"input":input,"http":response,"before_go":before_go,"after_go":after_go,"before_rust":before_rust,"after_rust":after_rust}));
    }
    if let Some(path) = std::env::var_os("MCS_038_PATCH_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"cases":receipts})).unwrap(),
        )
        .unwrap();
    }
}
