// Copyright 2026 AsterSQL.
use super::*;
use crate::tso_group_watch_test::fixture;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{
    Consistency, KeyspaceGroup, MetaStore, Mutation, Read, SplitState,
};
use std::sync::Arc;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_073_handoff_corruption_cancel_and_durable_boundary() {
    let f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(873);
    let group = KeyspaceGroup {
        id: 2,
        split_state: Some(SplitState { split_source: 1 }),
        keyspaces: Some(vec![101]),
        ..Default::default()
    };
    let source = p.timestamp_path(1);
    let target = p.timestamp_path(2);
    store
        .transact(
            vec![],
            vec![
                Mutation::put(
                    p.keyspace_group_id_path(2).as_bytes(),
                    &serde_json::to_vec(&group).unwrap(),
                ),
                Mutation::put(source.as_bytes(), b"corrupt"),
            ],
        )
        .await
        .unwrap();
    let cfg = GroupWatchConfig {
        store: store.clone(),
        runtime: tokio::runtime::Handle::current(),
        cancel: tokio_util::sync::CancellationToken::new(),
        cluster_id: 873,
        address: "http://handoff".into(),
        bootstrap_keyspace: 0,
        participant: Arc::new(|_| b"handoff".to_vec()),
        clock: Arc::new(SystemClock),
        lease_seconds: 3,
        save_interval_millis: 1000,
    };
    assert!(matches!(
        reserve_group_handoff(&cfg, &group).await,
        Err(TsoError::CorruptBoundary)
    ));
    assert!(
        store
            .read(Read::prefix(target.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let bound = 8_000_000_000u64;
    store
        .transact(
            vec![],
            vec![Mutation::put(source.as_bytes(), &bound.to_be_bytes())],
        )
        .await
        .unwrap();
    reserve_group_handoff(&cfg, &group).await.unwrap();
    let saved = store
        .read(Read::prefix(target.as_bytes(), Consistency::Linearizable))
        .await
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(
        u64::from_be_bytes(saved.value.try_into().unwrap()),
        bound + 1_000_000
    );
    assert_eq!(
        store
            .read(Read::prefix(source.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        bound.to_be_bytes()
    );
    cfg.cancel.cancel();
    assert!(reserve_group_handoff(&cfg, &group).await.is_err());
    assert_eq!(
        store
            .read(Read::prefix(target.as_bytes(), Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .mod_revision,
        saved.mod_revision
    );
}
