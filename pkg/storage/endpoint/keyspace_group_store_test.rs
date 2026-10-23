// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

// Actual Go save captured by task 22, revision 5, membership/00002.
const GO_JSON: &[u8] = br#"{"id":2,"user-kind":"enterprise","split-state":{"split-source":2},"members":[{"address":"http://tso-2","priority":0}],"keyspaces":[42,43]}"#;

fn group(id: u32) -> KeyspaceGroup {
    KeyspaceGroup {
        id,
        user_kind: "basic".into(),
        keyspaces: Some(vec![id]),
        ..Default::default()
    }
}

async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let p = ClusterPaths::new(42);
    assert!(ep.load_keyspace_groups(&p, 0, 0).await.unwrap().is_empty());
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_group(txn, &p, 2).await?, None);
        Ok::<_, KeyspaceGroupStoreError>(())
    })
    .await
    .unwrap();
    let mut go: KeyspaceGroup = serde_json::from_slice(GO_JSON).unwrap();
    go.keyspace_lookup_table.insert(42);
    base.run_in_txn(async |txn| ep.save_keyspace_group(txn, &p, &go))
        .await
        .unwrap();
    assert_eq!(
        base.load(p.keyspace_group_id_path(2).as_bytes())
            .await
            .unwrap(),
        GO_JSON
    );
    go.keyspace_lookup_table.clear();
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_group(txn, &p, 2).await?, Some(go.clone()));
        assert_eq!(
            ep.load_keyspace_group(txn, &ClusterPaths::new(43), 2)
                .await?,
            None
        );
        Ok::<_, KeyspaceGroupStoreError>(())
    })
    .await
    .unwrap();
    // Buffered writes are invisible until commit; both keys share a revision.
    base.run_in_txn(async |txn| {
        ep.save_keyspace_group(txn, &p, &group(0))?;
        ep.save_keyspace_group(txn, &p, &group(1))?;
        assert_eq!(ep.load_keyspace_group(txn, &p, 1).await?, None);
        assert_eq!(ep.load_keyspace_groups(&p, 0, 0).await?, vec![go.clone()]);
        Ok::<_, KeyspaceGroupStoreError>(())
    })
    .await
    .unwrap();
    let snapshot = store
        .read(Read::prefix(
            p.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(
        snapshot.entries[0].mod_revision,
        snapshot.entries[1].mod_revision
    );
    // Default deletion fails before commit and rolls back preceding writes.
    let protected = base
        .run_in_txn(async |txn| {
            ep.save_keyspace_group(txn, &p, &group(9))?;
            ep.delete_keyspace_group(txn, &p, 0)
        })
        .await;
    assert!(matches!(
        protected,
        Err(CallbackTxnError::Callback(
            KeyspaceGroupStoreError::ModifyDefaultKeyspaceGroup
        ))
    ));
    assert!(
        base.load(p.keyspace_group_id_path(9).as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ep.load_keyspace_groups(&p, 0, 0).await.unwrap(),
        vec![group(0), group(1), go.clone()]
    );
    assert_eq!(
        ep.load_keyspace_groups(&p, 1, 1).await.unwrap(),
        vec![group(1)]
    );
    assert_eq!(ep.load_keyspace_groups(&p, 2, 0).await.unwrap(), vec![go]);
    assert!(ep.load_keyspace_groups(&p, 3, 0).await.unwrap().is_empty());
    // Abort a buffered overwrite and delete together.
    let aborted = base
        .run_in_txn(async |txn| {
            ep.delete_keyspace_group(txn, &p, 2)?;
            ep.save_keyspace_group(
                txn,
                &p,
                &KeyspaceGroup {
                    id: 1,
                    user_kind: "changed".into(),
                    ..Default::default()
                },
            )?;
            Err::<(), _>(KeyspaceGroupStoreError::ModifyDefaultKeyspaceGroup)
        })
        .await;
    assert!(matches!(aborted, Err(CallbackTxnError::Callback(_))));
    assert_eq!(
        base.load(p.keyspace_group_id_path(2).as_bytes())
            .await
            .unwrap(),
        GO_JSON
    );
    assert_eq!(
        ep.load_keyspace_groups(&p, 1, 1).await.unwrap(),
        vec![group(1)]
    );
    // A missing-key read guards against concurrent creation and rolls back deletes.
    let created = base
        .run_in_txn(async |txn| {
            assert_eq!(ep.load_keyspace_group(txn, &p, 9).await?, None);
            ep.delete_keyspace_group(txn, &p, 1)?;
            base.save(p.keyspace_group_id_path(9).as_bytes(), GO_JSON)
                .await
                .map_err(EndpointError::from)?;
            Ok::<_, KeyspaceGroupStoreError>(())
        })
        .await;
    assert!(matches!(created, Err(CallbackTxnError::Conflict)));
    assert_eq!(
        ep.load_keyspace_groups(&p, 1, 1).await.unwrap(),
        vec![group(1)]
    );
    base.remove(p.keyspace_group_id_path(9).as_bytes())
        .await
        .unwrap();
    // Same-byte rewrite must conflict (revision guards, not value comparison).
    let conflict = base
        .run_in_txn(async |txn| {
            ep.load_keyspace_group(txn, &p, 2).await?;
            ep.delete_keyspace_group(txn, &p, 1)?;
            base.save(p.keyspace_group_id_path(2).as_bytes(), GO_JSON)
                .await
                .map_err(EndpointError::from)?;
            Ok::<_, KeyspaceGroupStoreError>(())
        })
        .await;
    assert!(matches!(conflict, Err(CallbackTxnError::Conflict)));
    assert!(
        !base
            .load(p.keyspace_group_id_path(1).as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    // Empty point bytes collapse to absence; the same range value is invalid JSON.
    base.save(p.keyspace_group_id_path(2).as_bytes(), b"")
        .await
        .unwrap();
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_group(txn, &p, 2).await?, None);
        Ok::<_, KeyspaceGroupStoreError>(())
    })
    .await
    .unwrap();
    for raw in [b"".as_slice(), b"{bad", b"[]", b"{\"id\":-1}", b"\xff"] {
        base.save(p.keyspace_group_id_path(2).as_bytes(), raw)
            .await
            .unwrap();
        assert!(matches!(
            ep.load_keyspace_groups(&p, 2, 0).await,
            Err(KeyspaceGroupStoreError::Endpoint(
                EndpointError::JsonUnmarshal(_)
            ))
        ));
        if !raw.is_empty() {
            assert!(matches!(
                base.run_in_txn(async |txn| ep.load_keyspace_group(txn, &p, 2).await)
                    .await,
                Err(CallbackTxnError::Callback(
                    KeyspaceGroupStoreError::Endpoint(EndpointError::JsonUnmarshal(_))
                ))
            ));
        }
        // Bounded range must not decode corrupt values beyond its limit.
        assert_eq!(
            ep.load_keyspace_groups(&p, 0, 1).await.unwrap(),
            vec![group(0)]
        );
    }
    base.save(p.keyspace_group_id_path(2).as_bytes(), b"null")
        .await
        .unwrap();
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_keyspace_group(txn, &p, 2).await?,
            Some(KeyspaceGroup::default())
        );
        Ok::<_, KeyspaceGroupStoreError>(())
    })
    .await
    .unwrap();
    assert_eq!(
        ep.load_keyspace_groups(&p, 2, 0).await.unwrap(),
        vec![KeyspaceGroup::default()]
    );
    base.run_in_txn(async |txn| ep.delete_keyspace_group(txn, &p, 2))
        .await
        .unwrap();
    base.run_in_txn(async |txn| ep.delete_keyspace_group(txn, &p, 2))
        .await
        .unwrap();
    assert_eq!(ep.load_keyspace_groups(&p, 2, 0).await.unwrap(), vec![]);
    // Physical %05d paths sort lexically even when IDs exceed five digits.
    for id in [99999, 100000, u32::MAX] {
        base.run_in_txn(async |txn| ep.save_keyspace_group(txn, &p, &group(id)))
            .await
            .unwrap();
    }
    assert_eq!(
        ep.load_keyspace_groups(&p, 2, 0)
            .await
            .unwrap()
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![100000, u32::MAX, 99999]
    );
    assert_eq!(
        ep.load_keyspace_groups(&p, 99999, 0).await.unwrap(),
        vec![group(99999)]
    );
    // Zero-limit scans do not acquire per-entry transaction guards.
    let many = ClusterPaths::new(44);
    for id in 0..205 {
        base.run_in_txn(async |txn| ep.save_keyspace_group(txn, &many, &group(id)))
            .await
            .unwrap();
    }
    let all = ep.load_keyspace_groups(&many, 0, 0).await.unwrap();
    assert_eq!(all.len(), 205);
    assert_eq!(
        all.iter().map(|g| g.id).collect::<Vec<_>>(),
        (0..205).collect::<Vec<_>>()
    );
    // Go encoding/json HTML and line separator escaping (source-derived boundary).
    let escaped = KeyspaceGroup {
        id: 1,
        user_kind: "<&>\u{2028}\u{2029}".into(),
        ..Default::default()
    };
    base.run_in_txn(async |txn| ep.save_keyspace_group(txn, &many, &escaped))
        .await
        .unwrap();
    let raw = base
        .load(many.keyspace_group_id_path(1).as_bytes())
        .await
        .unwrap();
    assert!(
        std::str::from_utf8(&raw)
            .unwrap()
            .contains(r"\u003c\u0026\u003e\u2028\u2029")
    );
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_group(txn, &many, 1).await?, Some(escaped));
        Ok::<_, KeyspaceGroupStoreError>(())
    })
    .await
    .unwrap();
    // Go's large-value revision-protected read/write path remains usable.
    let big = KeyspaceGroup {
        id: 1,
        keyspaces: Some((0..1000).collect()),
        ..group(1)
    };
    base.run_in_txn(async |txn| ep.save_keyspace_group(txn, &p, &big))
        .await
        .unwrap();
    base.run_in_txn(async |txn| {
        let loaded = ep.load_keyspace_group(txn, &p, 1).await?.unwrap();
        assert_eq!(loaded, big);
        ep.save_keyspace_group(txn, &p, &loaded)
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn keyspace_group_store_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn keyspace_group_store_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let store = Arc::new(fixture.store().await);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let p = ClusterPaths::new(99);
    base.run_in_txn(async |txn| ep.save_keyspace_group(txn, &p, &group(7)))
        .await
        .unwrap();
    let began = std::time::Instant::now();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let restarted = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        restarted.load_keyspace_groups(&p, 0, 0).await.unwrap(),
        vec![group(7)]
    );
    eprintln!(
        "keyspace_group_store_etcd: same-WAL restart and recovered group 7 at {:?}",
        began.elapsed()
    );
    drop(fixture);
    assert!(matches!(
        base.run_in_txn(async |txn| ep.load_keyspace_group(txn, &p, 7).await)
            .await,
        Err(CallbackTxnError::Callback(
            KeyspaceGroupStoreError::Endpoint(EndpointError::Store(_))
        ))
    ));
    assert!(matches!(
        base.run_in_txn(async |txn| ep.delete_keyspace_group(txn, &p, 7))
            .await,
        Err(CallbackTxnError::Store(_))
    ));
    eprintln!(
        "keyspace_group_store_etcd: fixture exited; point read and delete commit report backend failure at {:?}",
        began.elapsed()
    );
    assert!(matches!(
        restarted.load_keyspace_groups(&p, 0, 0).await,
        Err(KeyspaceGroupStoreError::Endpoint(EndpointError::Store(_)))
    ));
}

#[test]
fn keyspace_group_store_go_wire() {
    let value: KeyspaceGroup = serde_json::from_slice(GO_JSON).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), GO_JSON);
}
