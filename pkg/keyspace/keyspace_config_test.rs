// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity};
use std::{collections::BTreeMap, sync::Arc};
#[tokio::test]
async fn mcs_038_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store)
        .keyspace_storage(ClusterPaths::new(42));
    let manager = KeyspaceManager::new(storage);
    manager
        .save_new(&Keyspace {
            identity: Some(KeyspaceIdentity::Id(38)),
            name: "config38".into(),
            state: 0,
            ..Default::default()
        })
        .await
        .unwrap();
    let value = manager
        .update_config_with_preconditions(
            "config38",
            &[ConfigMutation {
                op: ConfigOp::Put,
                key: "key".into(),
                value: "value".into(),
            }],
            &BTreeMap::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        value.config.get("key").map(String::as_str),
        Some("value"),
        "configuration patch must be committed"
    );
    assert_eq!(manager.load_by_id(38).await.unwrap(), value);
}
use astersql_pd_storage_domain::{
    Base, Consistency, KeyspaceGroup, MetaServiceGroupStatus, MetaStore, Read, StorageEndpoint,
};
use std::collections::HashMap;
fn put(key: &str, value: &str) -> ConfigMutation {
    ConfigMutation {
        op: ConfigOp::Put,
        key: key.into(),
        value: value.into(),
    }
}
fn del(key: &str) -> ConfigMutation {
    ConfigMutation {
        op: ConfigOp::Delete,
        key: key.into(),
        value: String::new(),
    }
}
async fn plain(store: Arc<dyn MetaStore>, state: i32) -> KeyspaceManager {
    let m = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store)
            .keyspace_storage(ClusterPaths::new(42)),
    );
    m.save_new(&Keyspace {
        identity: Some(KeyspaceIdentity::Id(38)),
        name: "config38".into(),
        state,
        config: HashMap::from([("a".into(), "one".into()), ("empty".into(), String::new())]),
        ..Default::default()
    })
    .await
    .unwrap();
    m
}
#[tokio::test]
async fn mcs_038_preconditions_absent_empty_order_and_state_guards() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let m = plain(store.clone(), 0).await;
    let original = m.load_by_id(38).await.unwrap();
    for (key, expect, message) in [
        ("a", None, "a must be absent"),
        ("missing", Some("x"), "missing does not exist"),
        ("a", Some("wrong"), "key=a expected=wrong actual=one"),
    ] {
        let e = m
            .update_config_with_preconditions(
                "config38",
                &[put("a", "two")],
                &BTreeMap::from([(key.into(), expect.map(String::from))]),
            )
            .await
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            format!(
                "[PD:keyspace:ErrKeyspaceConfigPreconditionFailed]keyspace config precondition failed, {message}"
            )
        );
        assert_eq!(m.load_by_id(38).await.unwrap(), original);
    }
    let value = m
        .update_config_with_preconditions(
            "config38",
            &[
                put("a", "two"),
                del("a"),
                put("a", "last"),
                del("missing"),
                put("meta_service_group_addrs", "forged"),
            ],
            &BTreeMap::from([
                ("empty".into(), Some(String::new())),
                ("missing".into(), None),
            ]),
        )
        .await
        .unwrap();
    assert_eq!(value.config["a"], "last");
    assert!(!value.config.contains_key("meta_service_group_addrs"));
    assert!(
        m.update_config(
            "config38",
            &[ConfigMutation {
                op: ConfigOp::Unknown(99),
                key: "a".into(),
                value: "bad".into()
            }]
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("ErrIllegalOperation")
    );
    assert_eq!(m.load_by_id(38).await.unwrap(), value);
    assert!(matches!(
        m.update_config("missing", &[]).await,
        Err(KeyspaceError::NotFound)
    ));
    for state in [2, 3, 99] {
        let mut item = value.clone();
        item.identity = Some(KeyspaceIdentity::Id(100 + state as u32));
        item.name = format!("state{state}");
        item.state = state;
        m.save_new(&item).await.unwrap();
        assert_eq!(
            m.update_config(&item.name, &[put("a", "new")])
                .await
                .unwrap_err()
                .to_string(),
            format!(
                "cannot change config for keyspace with state {}",
                crate::keyspace_config::state_name(state)
            )
        );
        assert_eq!(m.load_by_id(item.id()).await.unwrap(), item);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_038_independent_managers_cas_has_one_winner() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let a = Arc::new(plain(store.clone(), 0).await);
    let b = Arc::new(KeyspaceManager::new(a.storage.clone()));
    let x = a.clone();
    let y = b.clone();
    let pre = BTreeMap::from([("a".into(), Some("one".into()))]);
    let pre2 = pre.clone();
    let (l, r) = tokio::join!(
        tokio::spawn(async move {
            x.update_config_with_preconditions("config38", &[put("a", "left")], &pre)
                .await
        }),
        tokio::spawn(async move {
            y.update_config_with_preconditions("config38", &[put("a", "right")], &pre2)
                .await
        })
    );
    let l = l.unwrap();
    let r = r.unwrap();
    assert_ne!(l.is_ok(), r.is_ok());
    let winner = if let Ok(v) = l { v } else { r.unwrap() };
    assert_eq!(a.load_by_id(38).await.unwrap(), winner);
}
async fn managed(
    store: Arc<dyn MetaStore>,
) -> (
    KeyspaceManager,
    Arc<KeyspaceGroupManager>,
    Arc<MetaServiceGroupManager>,
) {
    let p = ClusterPaths::new(42);
    let groups = Arc::new(KeyspaceGroupManager::new(store.clone(), p.clone()));
    groups
        .create_groups(&[
            KeyspaceGroup {
                id: 1,
                user_kind: "basic".into(),
                keyspaces: Some(vec![38]),
                ..Default::default()
            },
            KeyspaceGroup {
                id: 2,
                user_kind: "basic".into(),
                keyspaces: Some(vec![]),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    let mgm = Arc::new(MetaServiceGroupManager::new(
        store.clone(),
        p.clone(),
        BTreeMap::from([
            ("a".into(), "127.0.0.1:10001".into()),
            ("b".into(), "127.0.0.1:10002".into()),
        ]),
    ));
    let mut txn = Base::new(store.as_ref()).begin_txn();
    let ep = StorageEndpoint::new(store.clone());
    for (id, count) in [("a", 1), ("b", 0)] {
        ep.save_meta_service_group_status(
            &mut txn,
            &p,
            id,
            Some(&MetaServiceGroupStatus {
                enabled: true,
                assignment_count: count,
            }),
        )
        .unwrap();
    }
    txn.commit::<String>().await.unwrap();
    let m = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(store).keyspace_storage(p),
        KeyspaceCreateResources {
            groups: Some(groups.clone()),
            meta_groups: Some(mgm.clone()),
            ..Default::default()
        },
    );
    m.save_new(&Keyspace {
        identity: Some(KeyspaceIdentity::Id(38)),
        name: "config38".into(),
        state: 0,
        config: HashMap::from([
            ("tso_keyspace_group_id".into(), "1".into()),
            ("user_kind".into(), "basic".into()),
            ("meta_service_group_id".into(), "a".into()),
        ]),
        ..Default::default()
    })
    .await
    .unwrap();
    (m, groups, mgm)
}
async fn counts(store: Arc<dyn MetaStore>) -> BTreeMap<String, i64> {
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .load_meta_service_group_status(
            &mut txn,
            &ClusterPaths::new(42),
            &BTreeMap::from([("a".into(), String::new()), ("b".into(), String::new())]),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k, v.assignment_count))
        .collect()
}
#[tokio::test]
async fn mcs_038_meta_group_validation_precedes_tso_move_and_counter_commit() {
    let f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (m, g, _) = managed(store.clone()).await;
    let original = m.load_by_id(38).await.unwrap();
    let e = m
        .update_config(
            "config38",
            &[
                put("tso_keyspace_group_id", "2"),
                put("meta_service_group_id", "unknown"),
            ],
        )
        .await
        .unwrap_err();
    assert!(matches!(
        e,
        KeyspaceError::Assignment(GroupAssignmentError::UnknownGroup)
    ));
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![38])
    );
    assert_eq!(m.load_by_id(38).await.unwrap(), original);
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_meta_service_group_status(
            &mut txn,
            &ClusterPaths::new(42),
            "b",
            Some(&MetaServiceGroupStatus {
                enabled: false,
                assignment_count: 0,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let e = m
        .update_config(
            "config38",
            &[
                put("tso_keyspace_group_id", "2"),
                put("meta_service_group_id", "b"),
            ],
        )
        .await
        .unwrap_err();
    assert!(matches!(
        e,
        KeyspaceError::Assignment(GroupAssignmentError::DisabledGroup)
    ));
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![38])
    );
    assert_eq!(
        counts(store.clone()).await,
        BTreeMap::from([("a".into(), 1), ("b".into(), 0)])
    );
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_meta_service_group_status(
            &mut txn,
            &ClusterPaths::new(42),
            "b",
            Some(&MetaServiceGroupStatus {
                enabled: true,
                assignment_count: 0,
            }),
        )
        .unwrap();
    txn.commit::<String>().await.unwrap();
    let v = m
        .update_config(
            "config38",
            &[
                put("tso_keyspace_group_id", "2"),
                put("meta_service_group_id", "b"),
            ],
        )
        .await
        .unwrap();
    assert_eq!(v.config["meta_service_group_addrs"], "127.0.0.1:10002");
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![])
    );
    assert_eq!(
        g.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![38])
    );
    assert_eq!(
        counts(store.clone()).await,
        BTreeMap::from([("a".into(), 0), ("b".into(), 1)])
    );
}
#[tokio::test]
async fn mcs_038_actual_commit_rejection_preserves_config_but_go_tso_stage_survives_wal() {
    let mut f = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let (m, g, _) = managed(store.clone()).await;
    let original = m.load_by_id(38).await.unwrap();
    let key = ClusterPaths::new(42).keyspace_meta_path(38);
    let before = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(
        m.update_config(
            "config38",
            &[
                put("tso_keyspace_group_id", "2"),
                put("meta_service_group_id", "b"),
                put("oversized", &"x".repeat(2 * 1024 * 1024))
            ]
        )
        .await
        .is_err()
    );
    assert_eq!(m.load_by_id(38).await.unwrap(), original);
    assert_eq!(
        before.entries,
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    assert_eq!(
        g.get_group(1).await.unwrap().unwrap().keyspaces,
        Some(vec![])
    );
    assert_eq!(
        g.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![38])
    );
    assert_eq!(
        counts(store.clone()).await,
        BTreeMap::from([("a".into(), 1), ("b".into(), 0)])
    );
    f.command("stop", "stopped");
    assert!(
        m.update_config("config38", &[put("a", "new")])
            .await
            .is_err()
    );
    f.command("start", "restarted");
    let restored: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let recovered = KeyspaceManager::new(
        astersql_pd_storage_endpoint::StorageEndpoint::new(restored.clone())
            .keyspace_storage(ClusterPaths::new(42)),
    );
    let mut raw_original = original.clone();
    raw_original.config.remove("meta_service_group_addrs");
    assert_eq!(recovered.load_by_id(38).await.unwrap(), raw_original);
    let groups = KeyspaceGroupManager::new(restored.clone(), ClusterPaths::new(42));
    assert_eq!(
        groups.get_group(2).await.unwrap().unwrap().keyspaces,
        Some(vec![38])
    );
    assert_eq!(
        counts(restored).await,
        BTreeMap::from([("a".into(), 1), ("b".into(), 0)])
    );
}
