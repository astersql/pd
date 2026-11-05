// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{KeyspaceCreateResources, KeyspaceManager, MetaServiceGroupManager};
use astersql_pd_storage_domain::{
    Base, Consistency, Keyspace, KeyspaceIdentity, MetaServiceGroupStatus, MetaStore, Read,
    StorageEndpoint,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
async fn state(store: &dyn MetaStore, p: &ClusterPaths) -> Value {
    let mut records = vec![];
    for key in [
        p.keyspace_meta_path(100),
        p.keyspace_id_path("remove_live"),
        p.meta_service_group_status_path("g"),
    ] {
        let s = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        records.push(json!({"key":key,"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>()}));
    }
    json!(records)
}
async fn removal_events(
    watch: &mut dyn astersql_pd_storage_domain::MetaWatch,
    paths: &ClusterPaths,
) -> Value {
    let wanted = [
        paths.keyspace_meta_path(100),
        paths.keyspace_id_path("remove_live"),
        paths.meta_service_group_status_path("g"),
    ];
    let mut records = BTreeMap::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while records.len() < 3 {
            let batch = watch
                .next(&tokio_util::sync::CancellationToken::new())
                .await
                .unwrap();
            for event in batch.events {
                if let Some((i, _)) = wanted
                    .iter()
                    .enumerate()
                    .find(|(_, key)| key.as_bytes() == event.key())
                {
                    records.insert(
                        i,
                        (
                            String::from_utf8_lossy(event.key()).to_string(),
                            event.revision(),
                        ),
                    );
                }
            }
        }
    })
    .await
    .unwrap();
    let revision = records[&0].1;
    assert!(records.values().all(|(_, r)| *r == revision));
    json!(records)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_040_remove_go_persistent_differential() {
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
        json!({"id":100,"name":"remove_live"}),
    );
    assert_eq!(created.0, 200, "{created:?}");
    let mut current = created.1;
    for name in ["DISABLED", "ARCHIVED"] {
        let r = http(
            &format!("{spaces}/remove_live/state"),
            "PUT",
            json!({"state":name}),
        );
        assert_eq!(r.0, 200);
        current = r.1;
    }
    let gs: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let gp = ClusterPaths::new(cluster);
    let mut fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let rp = ClusterPaths::new(42);
    let m = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(rs.clone()).keyspace_storage(rp.clone()),
        KeyspaceCreateResources {
            meta_groups: Some(Arc::new(MetaServiceGroupManager::new(
                rs.clone(),
                rp.clone(),
                BTreeMap::new(),
            ))),
            ..Default::default()
        },
    );
    let mut metadata = Keyspace {
        identity: Some(KeyspaceIdentity::Id(100)),
        name: "remove_live".into(),
        state: 2,
        created_at: current["created_at"].as_i64().unwrap(),
        state_changed_at: current["state_changed_at"].as_i64().unwrap(),
        config: serde_json::from_value(current["config"].clone()).unwrap(),
    };
    metadata
        .config
        .insert("meta_service_group_id".into(), "g".into());
    m.save_new(&metadata).await.unwrap();
    for (store, paths) in [(&gs, &gp), (&rs, &rp)] {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        let adapter = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
        adapter
            .save_keyspace_meta(&mut txn, paths, &metadata)
            .unwrap();
        StorageEndpoint::new(store.clone())
            .save_meta_service_group_status(
                &mut txn,
                paths,
                "g",
                Some(&MetaServiceGroupStatus {
                    enabled: true,
                    assignment_count: 1,
                }),
            )
            .unwrap();
        txn.commit::<String>().await.unwrap();
    }
    let before_go = state(gs.as_ref(), &gp).await;
    let before_rust = state(rs.as_ref(), &rp).await;
    let mut go_watch = gs
        .watch(
            format!("/pd/{cluster}").into_bytes(),
            before_go[0]["revision"].as_i64().unwrap() + 1,
        )
        .await
        .unwrap();
    let mut rust_watch = rs
        .watch(
            b"/pd/42".to_vec(),
            before_rust[0]["revision"].as_i64().unwrap() + 1,
        )
        .await
        .unwrap();
    // The existing Go production entry owns membership/batching; compare this task's metadata/counter boundary.
    let response = http(
        &format!("{groups}/1/keyspaces"),
        "DELETE",
        json!({"keyspaces":[100]}),
    );
    assert_eq!(response.0, 200, "{response:?}");
    m.remove(100).await.unwrap();
    let go_events = removal_events(go_watch.as_mut(), &gp).await;
    let rust_events = removal_events(rust_watch.as_mut(), &rp).await;
    let after_go = state(gs.as_ref(), &gp).await;
    let after_rust = state(rs.as_ref(), &rp).await;
    for snapshot in [&after_go, &after_rust] {
        assert_eq!(snapshot[0]["entries"], json!([]));
        assert_eq!(snapshot[1]["entries"], json!([]));
        let raw: Vec<u8> =
            serde_json::from_value(snapshot[2]["entries"][0]["bytes"].clone()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&raw).unwrap()["assignment_count"],
            0
        );
    }
    assert!(m.cache.get(100).is_none());
    assert_eq!(
        http(&format!("{spaces}/id/100"), "GET", Value::Null),
        (500, json!(m.load_by_id(100).await.unwrap_err().to_string()))
    );
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let recovered_rust = state(&fixture.store().await, &rp).await;
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let recovered_go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let recovered_go = state(&recovered_go_store, &gp).await;
    for i in 0..3 {
        assert_eq!(after_go[i]["entries"], recovered_go[i]["entries"]);
        assert_eq!(after_rust[i]["entries"], recovered_rust[i]["entries"]);
    }
    if let Some(path) = std::env::var_os("MCS_040_ORACLE_REPORT") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"cluster_id":cluster,"http":response,"before_go":before_go,"before_rust":before_rust,"after_go":after_go,"after_rust":after_rust,"go_events":go_events,"rust_events":rust_events,"recovered_go":recovered_go,"recovered_rust":recovered_rust,"scope":"metadata, name index and meta assignment counter; TSO membership batching remains task42"})).unwrap()).unwrap();
    }
}
