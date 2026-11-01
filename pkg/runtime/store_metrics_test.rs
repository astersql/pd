// Copyright 2026 AsterSQL.
use super::store_metrics::MeteredStore;
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read, StoreError};
use std::{sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_101_transactions_success_fault_cancel_and_recovery() {
    let mut fixture = crate::node_test::fixture::Fixture::start();
    let scope = astersql_pd_metricutil::ClassicMetrics::new().unwrap();
    let backend = astersql_pd_storage_kv::EtcdMetaStore::connect(
        &[fixture.endpoint.clone()],
        Duration::from_millis(400),
    )
    .await
    .unwrap();
    let store = MeteredStore::new(Arc::new(backend), &scope);
    let count = |result| scope.txn_counter.with_label_values(&[result]).get();
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: b"task101".to_vec(),
                value: b"durable".to_vec(),
                lease: None,
            }],
        )
        .await
        .unwrap();
    assert_eq!(count("success"), 1.);
    assert_eq!(
        store
            .read(Read::prefix(b"task101", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"durable"
    );
    assert_eq!(count("success"), 1.);
    store.transact(vec![], vec![]).await.unwrap();
    assert_eq!(count("success"), 2.);
    let suspended = fixture.suspend();
    // Dropping an in-flight real backend request records one failed observation.
    assert!(
        tokio::time::timeout(Duration::from_millis(20), store.transact(vec![], vec![]))
            .await
            .is_err()
    );
    assert_eq!(count("failed"), 1.);
    drop(suspended);
    fixture.command("stop", "stopped");
    assert_eq!(
        store.transact(vec![], vec![]).await.unwrap_err(),
        StoreError::Unavailable
    );
    assert_eq!(count("failed"), 2.);
    assert_eq!(
        scope
            .txn_duration
            .with_label_values(&["failed"])
            .get_sample_count(),
        2
    );
    fixture.command("start", "restarted");
    assert_eq!(
        store
            .read(Read::prefix(b"task101", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"durable"
    );
    assert_eq!(count("success"), 2.);
    store.transact(vec![], vec![]).await.unwrap();
    assert_eq!(count("success"), 3.);
    assert_eq!(
        scope
            .txn_duration
            .with_label_values(&["success"])
            .get_sample_count(),
        3
    );
}
