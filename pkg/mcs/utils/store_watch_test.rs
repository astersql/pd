// Copyright 2026 AsterSQL.
use super::StoreWatcher;
use astersql_pd_core::{StoreInfo, StoreMeta, StoreStats, StoresInfo};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::metapb;
use astersql_pd_storage_domain::{MemoryStore, MetaStore, Mutation};
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
async fn put(store: &dyn MetaStore, paths: ClusterPaths, id: u64, address: &str) {
    store
        .transact(
            vec![],
            vec![Mutation::put(
                paths.store_path(id).as_bytes(),
                &metapb::Store {
                    id,
                    address: address.into(),
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn store_watch_snapshot_and_forced_reload_preserve_local_stats_and_reconcile_deletes() {
    let backend = Arc::new(MemoryStore::default());
    let paths = ClusterPaths::new(83);
    let stores = StoresInfo::new();
    let cancel = CancellationToken::new();
    let mut existing = StoreInfo::new(StoreMeta {
        id: 1,
        address: "old".into(),
        ..Default::default()
    });
    existing.set_new_store_stats(StoreStats {
        capacity: 37,
        ..Default::default()
    });
    stores.put_store(Arc::new(existing));
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        ..Default::default()
    })));
    put(backend.as_ref(), paths, 1, "snapshot").await;
    let mut watch = StoreWatcher::load(backend.clone(), paths, stores.clone(), &cancel)
        .await
        .unwrap();
    assert_eq!(
        stores.get_store(1).unwrap().meta().unwrap().address,
        "snapshot"
    );
    assert_eq!(stores.get_store(1).unwrap().store_stats().capacity, 37);
    assert!(stores.get_store(2).is_none());
    backend
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: paths.store_path(1).into_bytes(),
            }],
        )
        .await
        .unwrap();
    put(backend.as_ref(), paths, 3, "reloaded").await;
    watch.reload(&cancel).await.unwrap();
    assert!(stores.get_store(1).is_none());
    assert_eq!(
        stores.get_store(3).unwrap().meta().unwrap().address,
        "reloaded"
    );
}
#[tokio::test]
async fn store_watch_compaction_and_invalid_snapshot_never_publish_partial_state() {
    let backend = Arc::new(MemoryStore::default());
    let paths = ClusterPaths::new(83);
    let stores = StoresInfo::new();
    let cancel = CancellationToken::new();
    put(backend.as_ref(), paths, 1, "initial").await;
    let mut watch = StoreWatcher::load(backend.clone(), paths, stores.clone(), &cancel)
        .await
        .unwrap();
    backend
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: paths.store_path(1).into_bytes(),
            }],
        )
        .await
        .unwrap();
    put(backend.as_ref(), paths, 2, "after").await;
    let revision = backend
        .read(astersql_pd_storage_domain::Read::prefix(
            b"",
            astersql_pd_storage_domain::Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .revision;
    backend.compact(revision).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while stores.get_store(1).is_some() || stores.get_store(2).is_none() {
            let _ = watch.advance(&cancel).await;
        }
    })
    .await
    .unwrap();
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        address: "cached".into(),
        ..Default::default()
    })));
    put(backend.as_ref(), paths, 1, "must not publish").await;
    backend
        .transact(
            vec![],
            vec![Mutation::put(paths.store_path(2).as_bytes(), b"invalid")],
        )
        .await
        .unwrap();
    assert!(watch.reload(&cancel).await.is_err());
    assert_eq!(
        stores.get_store(2).unwrap().meta().unwrap().address,
        "after"
    );
    assert_eq!(
        stores.get_store(1).unwrap().meta().unwrap().address,
        "cached"
    );
}
