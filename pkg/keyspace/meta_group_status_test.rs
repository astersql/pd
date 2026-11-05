// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Base, CallbackTxnError, Consistency, MetaStore, Read};
use std::{collections::BTreeMap, sync::Arc};
#[tokio::test]
async fn sa_012_status_target_and_errors() {
    let mut fixture = etcd_fixture::Fixture::start_with_txn_limit(2);
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(12);
    let manager = MetaServiceGroupManager::new(
        store.clone(),
        paths.clone(),
        BTreeMap::from_iter((0..4).map(|i| (format!("g{i}"), fixture.endpoint.clone()))),
    );
    for i in 0..4 {
        manager
            .patch_status(&format!("g{i}"), Some(0), Some(true))
            .await
            .unwrap();
    }
    // An all-groups compare set exceeds the real server's transaction budget.
    assert!(manager.get_status().await.is_err());
    manager
        .patch_status("g0", Some(5), Some(false))
        .await
        .unwrap();
    let read = || {
        Read::prefix(
            format!("/pd/{}/meta_service_groups/", paths.cluster_id()).as_bytes(),
            Consistency::Linearizable,
        )
    };
    let before = store.read(read()).await.unwrap().entries;
    assert!(matches!(
        manager.patch_status("missing", None, None).await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::UnknownGroup
        ))
    ));
    assert!(matches!(
        manager.patch_status("missing", Some(-1), None).await,
        Err(CallbackTxnError::Callback(
            GroupAssignmentError::InvalidCount
        ))
    ));
    assert_eq!(before, store.read(read()).await.unwrap().entries);
    manager.patch_status("g0", None, None).await.unwrap();
    let value = Base::new(store.as_ref())
        .load(paths.meta_service_group_status_path("g0").as_bytes())
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&value).unwrap(),
        serde_json::json!({"assignment_count":5,"enabled":false})
    );
    fixture.command("stop", "stopped");
    assert!(
        manager
            .patch_status("g0", Some(9), Some(true))
            .await
            .is_err()
    );
    fixture.command("start", "restarted");
    assert_eq!(
        value,
        Base::new(store.as_ref())
            .load(paths.meta_service_group_status_path("g0").as_bytes())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn sa_012_counter_pages_real_catalog() {
    use astersql_pd_storage_domain::{Keyspace, KeyspaceIdentity};
    let fixture = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(12);
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
        .keyspace_storage(paths.clone());
    for id in 0..131 {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        let meta = Keyspace {
            identity: Some(KeyspaceIdentity::Id(id)),
            name: format!("k{id}"),
            config: std::collections::HashMap::from([(
                "meta_service_group_id".into(),
                if id == 130 { "other" } else { "g" }.into(),
            )]),
            ..Default::default()
        };
        storage.save_keyspace_meta(&mut txn, &paths, &meta).unwrap();
        txn.commit::<std::convert::Infallible>().await.unwrap();
    }
    let counts = MetaGroupKeyspaceCounter(storage)
        .count(&["g".into()])
        .await
        .unwrap();
    assert_eq!(counts, BTreeMap::from([("g".into(), 130)]));
}
