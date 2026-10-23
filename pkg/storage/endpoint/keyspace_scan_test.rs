// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, MemoryStore, MetaStore};
use std::sync::Arc;

fn meta(id: u32) -> Keyspace {
    Keyspace {
        identity: Some(KeyspaceIdentity::Id(id)),
        name: format!("space/{id}"),
        ..Default::default()
    }
}

async fn scan(store: Arc<dyn MetaStore>, start: u32, limit: usize) -> Vec<Keyspace> {
    let ep = StorageEndpoint::new(store.clone());
    Base::new(store.as_ref())
        .run_in_txn(async |txn| {
            ep.load_range_keyspace(txn, &ClusterPaths::new(21), start, limit)
                .await
        })
        .await
        .unwrap()
}

async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(21);
    for id in (0..205).rev().chain([u32::MAX]) {
        base.run_in_txn(async |txn| ep.save_keyspace_meta(txn, &paths, &meta(id)))
            .await
            .unwrap();
    }
    // Invalid protobuf in indexes, adjacent prefixes and another cluster must be isolated.
    for key in [
        paths.keyspace_id_path("zzzz"),
        format!(
            "{}x/00000000",
            paths.keyspace_meta_prefix().trim_end_matches('/')
        ),
        ClusterPaths::new(22).keyspace_meta_path(0),
    ] {
        base.save(key.as_bytes(), b"\xff").await.unwrap();
    }
    assert_eq!(scan(store.clone(), 0, 1).await, [meta(0)]);
    assert_eq!(scan(store.clone(), 100, 2).await, [meta(100), meta(101)]);
    assert_eq!(
        scan(store.clone(), 204, 0).await,
        [meta(204), meta(u32::MAX)]
    );
    assert_eq!(scan(store.clone(), u32::MAX, 0).await, [meta(u32::MAX)]);
    assert_eq!(scan(store.clone(), 205, 1).await, [meta(u32::MAX)]);
    // Separate transactions keep each page below the canonical transaction operation limit.
    let mut ids = vec![];
    let mut start = 0;
    let mut pages = 0;
    loop {
        let page = scan(store.clone(), start, 31).await;
        assert!(!page.is_empty());
        let last = page.last().unwrap().id();
        ids.extend(page.iter().map(Keyspace::id));
        pages += 1;
        if last == u32::MAX {
            break;
        }
        start = last + 1;
    }
    assert_eq!(pages, 7);
    assert_eq!(ids, (0..205).chain([u32::MAX]).collect::<Vec<_>>());
    // Go pads to a minimum of eight digits, so wider IDs retain physical byte ordering.
    // In particular, a numeric ID successor is not a general physical pagination cursor.
    for id in [99_999_999, 100_000_000] {
        base.run_in_txn(async |txn| ep.save_keyspace_meta(txn, &paths, &meta(id)))
            .await
            .unwrap();
    }
    assert_eq!(
        scan(store.clone(), 100_000_000, 0).await,
        [meta(100_000_000), meta(u32::MAX), meta(99_999_999)]
    );
    assert_eq!(
        scan(store.clone(), u32::MAX, 0).await,
        [meta(u32::MAX), meta(99_999_999)]
    );
    assert_eq!(scan(store.clone(), 99_999_999, 0).await, [meta(99_999_999)]);
}

#[tokio::test]
async fn keyspace_scan_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn keyspace_scan_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store = Arc::new(fixture.store().await);
    assert_eq!(
        scan(store.clone(), u32::MAX, 0).await,
        [meta(u32::MAX), meta(99_999_999)]
    );
    drop(fixture);
    let ep = StorageEndpoint::new(store.clone());
    assert!(matches!(
        Base::new(store.as_ref())
            .run_in_txn(async |txn| {
                ep.load_range_keyspace(txn, &ClusterPaths::new(21), 0, 1)
                    .await
            })
            .await,
        Err(CallbackTxnError::Callback(KeyspaceMetaError::Store(_)))
    ));
}
