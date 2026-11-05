// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{Base, Keyspace, KeyspaceIdentity, KeyspaceStorage, MetaStore};
use std::{collections::HashMap, sync::Arc};
use tokio_util::sync::CancellationToken;
async fn seed(storage: &KeyspaceStorage, id: u32, state: i32, management: &str) {
    let base = Base::new(storage.store.as_ref());
    let mut tx = base.begin_txn();
    storage
        .save_keyspace_meta(
            &mut tx,
            &storage.paths,
            &Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("ks{id}"),
                state,
                config: HashMap::from([("gc_management_type".into(), management.into())]),
                ..Default::default()
            },
        )
        .unwrap();
    tx.commit::<String>().await.unwrap();
}
async fn storage(store: Arc<dyn MetaStore>) -> KeyspaceStorage {
    astersql_pd_storage_endpoint::StorageEndpoint::new(store)
        .keyspace_storage(ClusterPaths::new(42))
}
#[tokio::test]
async fn mcs_048_behavior() {
    let fixture = etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let storage = storage(store).await;
    for id in 0..258 {
        seed(
            &storage,
            id,
            if id == 12 { 1 } else { 0 },
            if id % 2 == 0 {
                "keyspace_level"
            } else {
                "unified"
            },
        )
        .await;
    }
    let snapshot = EnabledSnapshotLoader {
        storage,
        concurrency: 3,
    }
    .load(&CancellationToken::new())
    .await
    .unwrap();
    assert_eq!(
        snapshot.entries.len(),
        257,
        "enabled snapshot must include every enabled page, excluding disabled metadata"
    );
    assert!(snapshot.revision > 0);
    assert_eq!(snapshot.entries[&257].gc_management_type, "unified");
    assert!(!snapshot.entries.contains_key(&12));
}

use astersql_pd_storage_domain::{Consistency, Read, StoreError};
use std::time::Duration;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_048_pins_all_pages_during_real_metadata_change() {
    let mut fixture = etcd_fixture::Fixture::start();
    let writer: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let direct = storage(writer.clone()).await;
    for id in 1..=257 {
        seed(&direct, id, 0, "keyspace_level").await;
    }
    let revision = writer
        .read(Read::prefix(
            direct.paths.keyspace_meta_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .revision;
    let gate = response_gate_test::ResponseGate::start(&fixture.endpoint).await;
    let reader: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[gate.endpoint.clone()],
            Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let loader = EnabledSnapshotLoader {
        storage: storage(reader).await,
        concurrency: 4,
    };
    gate.arm_on_key(direct.paths.keyspace_meta_prefix().as_bytes());
    let load = tokio::spawn(async move { loader.load(&CancellationToken::new()).await });
    tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
        .await
        .unwrap();
    Base::new(writer.as_ref())
        .remove(direct.paths.keyspace_meta_path(257).as_bytes())
        .await
        .unwrap();
    seed(&direct, 300, 0, "unified").await;
    gate.release();
    let snapshot = load.await.unwrap().unwrap();
    gate.close().await;
    assert_eq!(snapshot.revision, revision);
    assert_eq!(snapshot.entries.len(), 257);
    assert!(snapshot.entries.contains_key(&257));
    assert!(!snapshot.entries.contains_key(&300));
    let latest = EnabledSnapshotLoader {
        storage: direct.clone(),
        concurrency: 2,
    }
    .load(&CancellationToken::new())
    .await
    .unwrap();
    assert_eq!(latest.entries.len(), 257);
    assert!(!latest.entries.contains_key(&257));
    assert_eq!(latest.entries[&300].gc_management_type, "unified");
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let recovered = EnabledSnapshotLoader {
        storage: storage(Arc::new(fixture.store().await)).await,
        concurrency: 2,
    }
    .load(&CancellationToken::new())
    .await
    .unwrap();
    assert_eq!(recovered.entries, latest.entries);
    println!(
        "pinned revision={revision} rows={} late-delete=257 late-insert=300 recovered={}",
        snapshot.entries.len(),
        recovered.entries.len()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_048_cancel_and_compaction_discard_attempt() {
    for compact in [false, true] {
        let fixture = etcd_fixture::Fixture::start();
        let writer: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
        let direct = storage(writer.clone()).await;
        for id in 1..=257 {
            seed(&direct, id, 0, "keyspace_level").await;
        }
        let gate = response_gate_test::ResponseGate::start(&fixture.endpoint).await;
        let reader: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[gate.endpoint.clone()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let loader = EnabledSnapshotLoader {
            storage: storage(reader).await,
            concurrency: 4,
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        gate.arm_on_key(direct.paths.keyspace_meta_prefix().as_bytes());
        let load = tokio::spawn(async move { loader.load(&token).await });
        tokio::time::timeout(Duration::from_secs(3), gate.held.notified())
            .await
            .unwrap();
        if compact {
            seed(&direct, 300, 0, "unified").await;
            let revision = writer
                .read(Read::prefix(b"", Consistency::Linearizable))
                .await
                .unwrap()
                .revision;
            writer.compact(revision).await.unwrap();
            gate.release();
        } else {
            cancel.cancel();
        }
        let error = tokio::time::timeout(Duration::from_secs(3), load)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        if compact {
            assert!(
                matches!(
                    error,
                    EnabledSnapshotError::Store(StoreError::Compacted { .. })
                ),
                "{error:?}"
            );
        } else {
            assert!(
                matches!(error, EnabledSnapshotError::Store(StoreError::Cancelled)),
                "{error:?}"
            );
        }
        gate.close().await;
        let recovered = EnabledSnapshotLoader {
            storage: direct,
            concurrency: 2,
        }
        .load(&CancellationToken::new())
        .await
        .unwrap();
        assert_eq!(recovered.entries.len(), if compact { 258 } else { 257 });
    }
}
#[tokio::test]
async fn mcs_048_boundaries_watermark_and_corrupt_pages() {
    let fixture = etcd_fixture::Fixture::start();
    let writer: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let direct = storage(writer.clone()).await;
    let loader = EnabledSnapshotLoader {
        storage: direct.clone(),
        concurrency: 4,
    };
    assert!(
        loader
            .load(&CancellationToken::new())
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    for id in 0..256 {
        seed(&direct, id, 0, "keyspace_level").await;
    }
    assert_eq!(
        loader
            .load(&CancellationToken::new())
            .await
            .unwrap()
            .entries
            .len(),
        256
    );
    for id in [4096, 8192, 0xffffff] {
        seed(&direct, id, 0, "unified").await;
    }
    for hint in [
        vec![],
        vec![1],
        0_u64.to_be_bytes().to_vec(),
        10000_u64.to_be_bytes().to_vec(),
        0xffffff_u64.to_be_bytes().to_vec(),
        u64::MAX.to_be_bytes().to_vec(),
    ] {
        Base::new(writer.as_ref())
            .save(direct.paths.keyspace_alloc_id_path().as_bytes(), &hint)
            .await
            .unwrap();
        let result = loader.load(&CancellationToken::new()).await.unwrap();
        assert_eq!(result.entries.len(), 259);
        assert_eq!(result.entries[&0xffffff].gc_management_type, "unified");
    }
    let key = direct.paths.keyspace_meta_path(8192);
    Base::new(writer.as_ref())
        .save(key.as_bytes(), b"malformed")
        .await
        .unwrap();
    assert!(
        loader
            .load(&CancellationToken::new())
            .await
            .unwrap_err()
            .to_string()
            .contains("decode keyspace metadata")
    );
    seed(&direct, 8192, 0, "unified").await;
    let mut tx = Base::new(writer.as_ref()).begin_txn();
    direct
        .save_keyspace_meta(
            &mut tx,
            &direct.paths,
            &Keyspace {
                identity: Some(KeyspaceIdentity::Id(9)),
                ..Default::default()
            },
        )
        .unwrap();
    tx.commit::<String>().await.unwrap();
    let value = Base::new(writer.as_ref())
        .load(direct.paths.keyspace_meta_path(9).as_bytes())
        .await
        .unwrap();
    Base::new(writer.as_ref())
        .save(key.as_bytes(), &value)
        .await
        .unwrap();
    assert!(
        loader
            .load(&CancellationToken::new())
            .await
            .unwrap_err()
            .to_string()
            .contains("contains ID 9")
    );
    seed(&direct, 8192, 0, "unified").await;
    let bad = format!("{}invalid", direct.paths.keyspace_meta_prefix());
    Base::new(writer.as_ref())
        .save(bad.as_bytes(), b"")
        .await
        .unwrap();
    assert!(
        loader
            .load(&CancellationToken::new())
            .await
            .unwrap_err()
            .to_string()
            .contains("invalid keyspace metadata key")
    );
}
