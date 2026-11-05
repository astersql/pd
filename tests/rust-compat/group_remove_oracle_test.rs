// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{
    KeyspaceCreateResources, KeyspaceGroupManager, KeyspaceManager, MetaServiceGroupManager,
};
use astersql_pd_storage_domain::{
    Base, Consistency, Keyspace, KeyspaceGroup, KeyspaceIdentity, MetaServiceGroupStatus,
    MetaStore, MetaWatch, Read, StorageEndpoint,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
async fn committed_batches(
    watch: &mut dyn MetaWatch,
    p: &ClusterPaths,
    count: usize,
) -> Vec<Value> {
    let mut batches = BTreeMap::<i64, Vec<String>>::new();
    let mut removed = 0;
    tokio::time::timeout(Duration::from_secs(10), async {
        while removed < count {
            for e in watch.next(&CancellationToken::new()).await.unwrap().events {
                let key = String::from_utf8(e.key().to_vec()).unwrap();
                if key == p.keyspace_group_id_path(1)
                    || key.starts_with(&format!("/pd/{}/keyspaces/", p.cluster_id()))
                    || key.starts_with(&format!("/pd/{}/meta_service_groups/", p.cluster_id()))
                {
                    if key.contains("/keyspaces/meta/") {
                        removed += 1;
                    }
                    batches.entry(e.revision()).or_default().push(key);
                }
            }
        }
    })
    .await
    .unwrap();
    batches
        .into_iter()
        .map(|(revision, keys)| json!({"revision":revision,"keys":keys}))
        .collect()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_042_bulk_removal_go_differential() {
    let (go, cluster) = crate::group_merge_oracle_test::start_api_go().await;
    let gs: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let gp = ClusterPaths::new(cluster);
    let rp = ClusterPaths::new(42);
    let g = KeyspaceGroupManager::new(rs.clone(), rp.clone());
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
    let mut reports = vec![];
    for (count, distinct) in [(122_u32, false), (71, true)] {
        let ids = (1000..1000 + count).collect::<Vec<_>>();
        let group = KeyspaceGroup {
            id: 1,
            user_kind: "standard".into(),
            keyspaces: Some(ids.clone()),
            ..Default::default()
        };
        let groups_url = format!("{}/pd/api/v2/tso/keyspace-groups", go.url);
        assert_eq!(
            http(&groups_url, "POST", json!({"keyspace-groups":[group]})).0,
            200
        );
        g.create_groups(&[group.clone()]).await.unwrap();
        for (store, p) in [(&gs, &gp), (&rs, &rp)] {
            let adapter = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
            for &id in &ids {
                let mut txn = Base::new(store.as_ref()).begin_txn();
                let meta = Keyspace {
                    identity: Some(KeyspaceIdentity::Id(id)),
                    name: format!("bulk_{id}"),
                    state: 2,
                    created_at: 123,
                    state_changed_at: 456,
                    config: HashMap::from([
                        ("tso_keyspace_group_id".into(), "1".into()),
                        ("user_kind".into(), "standard".into()),
                        (
                            "meta_service_group_id".into(),
                            if distinct {
                                format!("g{id}")
                            } else {
                                "g".into()
                            },
                        ),
                        ("padding".into(), "x".repeat(32 * 1024)),
                    ]),
                };
                adapter.save_keyspace_meta(&mut txn, p, &meta).unwrap();
                txn.save(
                    p.keyspace_id_path(&meta.name).as_bytes(),
                    id.to_string().as_bytes(),
                )
                .unwrap();
                txn.commit::<String>().await.unwrap();
            }
            for id in if distinct { ids.clone() } else { vec![0] } {
                let mut txn = Base::new(store.as_ref()).begin_txn();
                StorageEndpoint::new(store.clone())
                    .save_meta_service_group_status(
                        &mut txn,
                        p,
                        &if distinct {
                            format!("g{id}")
                        } else {
                            "g".into()
                        },
                        Some(&MetaServiceGroupStatus {
                            enabled: true,
                            assignment_count: if distinct { 1 } else { count as i64 },
                        }),
                    )
                    .unwrap();
                txn.commit::<String>().await.unwrap();
            }
        }
        let before_go = gs
            .read(Read::prefix(
                format!("/pd/{cluster}/").as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let before_rust = rs
            .read(Read::prefix(b"/pd/42/", Consistency::Linearizable))
            .await
            .unwrap();
        let mut gw = gs
            .watch(
                format!("/pd/{cluster}/").into_bytes(),
                before_go.revision + 1,
            )
            .await
            .unwrap();
        let mut rw = rs
            .watch(b"/pd/42/".to_vec(), before_rust.revision + 1)
            .await
            .unwrap();
        let response = http(
            &format!("{groups_url}/1/keyspaces"),
            "DELETE",
            json!({"keyspaces":ids}),
        );
        assert_eq!(response.0, 200, "{response:?}");
        let result = g
            .remove_keyspaces_from_group(&CancellationToken::new(), 1, &m, None, &ids)
            .await
            .unwrap();
        let go_group = http(&format!("{groups_url}/1"), "GET", Value::Null);
        assert_eq!(go_group, (200, serde_json::to_value(&result).unwrap()));
        assert_eq!(result.keyspaces, Some(vec![]));
        let ge = committed_batches(gw.as_mut(), &gp, count as usize).await;
        let re = committed_batches(rw.as_mut(), &rp, count as usize).await;
        assert_eq!(ge.len(), if distinct { 2 } else { 3 });
        assert_eq!(ge.len(), re.len());
        let expected = if distinct {
            vec![118, 97]
        } else {
            vec![120, 120, 10]
        };
        for batches in [&ge, &re] {
            assert_eq!(
                batches
                    .iter()
                    .map(|b| b["keys"].as_array().unwrap().len())
                    .collect::<Vec<_>>(),
                expected
            );
        }

        let mut states = vec![];
        for (store, p) in [(&gs, &gp), (&rs, &rp)] {
            let mut counters = vec![];
            for &id in &ids {
                for key in [
                    p.keyspace_meta_path(id),
                    p.keyspace_id_path(&format!("bulk_{id}")),
                ] {
                    assert!(
                        store
                            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                            .await
                            .unwrap()
                            .entries
                            .is_empty()
                    );
                }
            }
            for id in if distinct { ids.clone() } else { vec![0] } {
                let key = p.meta_service_group_status_path(&if distinct {
                    format!("g{id}")
                } else {
                    "g".into()
                });
                let value = store
                    .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                    .await
                    .unwrap();
                let dto: Value = serde_json::from_slice(&value.entries[0].value).unwrap();
                assert_eq!(dto["assignment_count"], 0);
                counters.push(dto);
            }
            states.push(counters);
        }
        assert_eq!(states[0], states[1]);
        reports.push(json!({"input":ids,"distinct":distinct,"http":response,"group":go_group,"go_batches":ge,"rust_batches":re,"counters":states}));
        assert_eq!(
            http(&format!("{groups_url}/1"), "DELETE", Value::Null).0,
            200
        );
        g.delete_group(1).await.unwrap();
    }
    if let Some(path) = std::env::var_os("MCS_042_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
}
