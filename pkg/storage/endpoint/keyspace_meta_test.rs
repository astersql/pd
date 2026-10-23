// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

fn meta(id: u32, name: &str) -> Keyspace {
    Keyspace {
        identity: Some(KeyspaceIdentity::Id(id)),
        name: name.into(),
        ..Default::default()
    }
}

async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_meta(txn, &paths, 0).await?, None);
        assert_eq!(ep.load_keyspace_id(txn, &paths, "zero").await?, None);
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    // Actual Go KeyspaceMeta observed at etcd revision 35; source TestLoadKeyspaceByIDGRPC.
    let mut go_meta = meta(4, "grpc_load_by_id");
    go_meta.state = 1;
    go_meta.state_changed_at = 1;
    go_meta
        .config
        .insert("encryption".into(), "test-encryption-config".into());
    base.run_in_txn(async |txn| ep.save_keyspace_meta(txn, &paths, &go_meta))
        .await
        .unwrap();
    assert_eq!(
        base.load(paths.keyspace_meta_path(4).as_bytes())
            .await
            .unwrap(),
        GO_WIRE
    );
    base.save(paths.keyspace_meta_path(4).as_bytes(), GO_WIRE)
        .await
        .unwrap();
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_keyspace_meta(txn, &paths, 4).await?,
            Some(go_meta.clone())
        );
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    base.remove(paths.keyspace_meta_path(4).as_bytes())
        .await
        .unwrap();

    let zero = meta(0, "zero");
    // Writes stay buffered; two fields appear in a single backend revision.
    base.run_in_txn(async |txn| {
        ep.save_keyspace_meta(txn, &paths, &zero)?;
        ep.save_keyspace_id(txn, &paths, 0, "zero")?;
        assert!(
            base.load(paths.keyspace_meta_path(0).as_bytes())
                .await?
                .is_empty()
        );
        assert_eq!(ep.load_keyspace_id(txn, &paths, "zero").await?, None);
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    let snapshot = store
        .read(Read::prefix(
            paths.keyspace_meta_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let index = store
        .read(Read::range(
            paths.keyspace_id_path("zero").as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(
        snapshot.entries[0].mod_revision,
        index.entries[0].mod_revision
    );
    assert_eq!(index.entries[0].value, b"0");
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_keyspace_meta(txn, &paths, 0).await?,
            Some(zero.clone())
        );
        assert_eq!(ep.load_keyspace_id(txn, &paths, "zero").await?, Some(0));
        assert_eq!(
            ep.load_keyspace_meta(txn, &ClusterPaths::new(43), 0)
                .await?,
            None
        );
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    // Callback errors roll back both writes and deletes.
    let result = base
        .run_in_txn(async |txn| {
            ep.remove_keyspace(txn, &paths, 0, "zero")?;
            ep.save_keyspace_meta(txn, &paths, &meta(9, "aborted"))?;
            ep.save_keyspace_id(txn, &paths, 9, "aborted")?;
            Err::<(), _>(KeyspaceMetaError::InvalidId)
        })
        .await;
    assert!(matches!(
        result,
        Err(CallbackTxnError::Callback(KeyspaceMetaError::InvalidId))
    ));
    assert_eq!(
        base.load(paths.keyspace_id_path("zero").as_bytes())
            .await
            .unwrap(),
        b"0"
    );
    assert!(
        base.load(paths.keyspace_meta_path(9).as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        base.load(paths.keyspace_id_path("aborted").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    base.run_in_txn(async |txn| ep.remove_keyspace(txn, &paths, 0, "zero"))
        .await
        .unwrap();
    assert!(
        base.load(paths.keyspace_meta_path(0).as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        base.load(paths.keyspace_id_path("zero").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    base.run_in_txn(async |txn| ep.remove_keyspace(txn, &paths, 0, "zero"))
        .await
        .unwrap();

    // Decimal parsing matches strconv.ParseUint(base 10, bitSize 24).
    for (raw, expected) in [
        (b"".as_slice(), None),
        (b"0", Some(0)),
        (b"00001", Some(1)),
        (b"16777215", Some(16777215)),
    ] {
        base.save(paths.keyspace_id_path("parse").as_bytes(), raw)
            .await
            .unwrap();
        base.run_in_txn(async |txn| {
            assert_eq!(ep.load_keyspace_id(txn, &paths, "parse").await?, expected);
            Ok::<_, KeyspaceMetaError>(())
        })
        .await
        .unwrap();
    }
    for raw in [
        b"+1".as_slice(),
        b"-1",
        b" 1",
        b"1_0",
        b"16777216",
        b"4294967296",
        b"\xff",
    ] {
        base.save(paths.keyspace_id_path("parse").as_bytes(), raw)
            .await
            .unwrap();
        let result = base
            .run_in_txn(async |txn| ep.load_keyspace_id(txn, &paths, "parse").await)
            .await;
        assert!(matches!(
            result,
            Err(CallbackTxnError::Callback(KeyspaceMetaError::InvalidId))
        ));
    }
    base.run_in_txn(async |txn| ep.save_keyspace_id(txn, &paths, u32::MAX, ""))
        .await
        .unwrap();
    assert_eq!(
        base.load(paths.keyspace_id_path("").as_bytes())
            .await
            .unwrap(),
        b"4294967295"
    );
    // Zero limit returns the entire physical range, subject to transaction limits.
    for id in 0..205 {
        base.run_in_txn(async |txn| {
            ep.save_keyspace_meta(txn, &paths, &meta(id, &format!("space/{id}")))
        })
        .await
        .unwrap();
    }
    let oversized = base
        .run_in_txn(async |txn| {
            let all = ep.load_range_keyspace(txn, &paths, 0, 0).await?;
            assert_eq!(all.len(), 205);
            assert_eq!(
                all.iter().map(Keyspace::id).collect::<Vec<_>>(),
                (0..205).collect::<Vec<_>>()
            );
            Ok::<_, KeyspaceMetaError>(())
        })
        .await;
    assert!(matches!(
        oversized,
        Err(CallbackTxnError::Store(
            astersql_pd_storage_kv::StoreError::InvalidArgument
        ))
    ));
    // Each returned entry adds a read guard; a bounded range commits normally.
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_range_keyspace(txn, &paths, 100, 2).await?,
            vec![meta(100, "space/100"), meta(101, "space/101")]
        );
        assert!(
            ep.load_range_keyspace(txn, &paths, 205, 0)
                .await?
                .is_empty()
        );
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    // Missing/empty point reads collapse; empty range entries decode as defaults.
    base.save(paths.keyspace_meta_path(204).as_bytes(), b"")
        .await
        .unwrap();
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_meta(txn, &paths, 204).await?, None);
        assert_eq!(
            ep.load_range_keyspace(txn, &paths, 204, 1).await?,
            vec![Keyspace::default()]
        );
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    for raw in [
        b"\xff".as_slice(),
        b"\x08",
        b"\x12\x05x",
        b"\x00",
        b"\x18\x80",
    ] {
        base.save(paths.keyspace_meta_path(204).as_bytes(), raw)
            .await
            .unwrap();
        let point = base
            .run_in_txn(async |txn| ep.load_keyspace_meta(txn, &paths, 204).await)
            .await;
        let range = base
            .run_in_txn(async |txn| ep.load_range_keyspace(txn, &paths, 200, 0).await)
            .await;
        assert!(matches!(
            point,
            Err(CallbackTxnError::Callback(
                KeyspaceMetaError::ProtoUnmarshal(_)
            ))
        ));
        assert!(matches!(
            range,
            Err(CallbackTxnError::Callback(KeyspaceMetaError::Decode(_)))
        ));
    }
    let mut unknown = GO_WIRE.to_vec();
    unknown.extend([0x48, 0x01]);
    base.save(paths.keyspace_meta_path(204).as_bytes(), &unknown)
        .await
        .unwrap();
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_keyspace_meta(txn, &paths, 204).await?,
            Some(go_meta)
        );
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    // Revision-protected metadata rejects identical-byte rewrites (ABA).
    base.save(paths.keyspace_id_path("preserve").as_bytes(), b"1")
        .await
        .unwrap();
    let original = base
        .load(paths.keyspace_meta_path(1).as_bytes())
        .await
        .unwrap();
    let conflict = base
        .run_in_txn(async |txn| {
            ep.load_keyspace_meta(txn, &paths, 1).await?;
            ep.remove_keyspace(txn, &paths, 1, "preserve")?;
            base.save(paths.keyspace_meta_path(1).as_bytes(), &original)
                .await?;
            Ok::<_, KeyspaceMetaError>(())
        })
        .await;
    assert!(matches!(conflict, Err(CallbackTxnError::Conflict)));
    assert_eq!(
        base.load(paths.keyspace_meta_path(1).as_bytes())
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        base.load(paths.keyspace_id_path("preserve").as_bytes())
            .await
            .unwrap(),
        b"1"
    );
    // A concurrent name-index update rejects the metadata and index deletion together.
    let conflict = base
        .run_in_txn(async |txn| {
            assert_eq!(ep.load_keyspace_id(txn, &paths, "preserve").await?, Some(1));
            ep.remove_keyspace(txn, &paths, 1, "preserve")?;
            base.save(paths.keyspace_id_path("preserve").as_bytes(), b"2")
                .await?;
            Ok::<_, KeyspaceMetaError>(())
        })
        .await;
    assert!(matches!(conflict, Err(CallbackTxnError::Conflict)));
    assert_eq!(
        base.load(paths.keyspace_meta_path(1).as_bytes())
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        base.load(paths.keyspace_id_path("preserve").as_bytes())
            .await
            .unwrap(),
        b"2"
    );
    // An all-default message encodes empty and collapses to absent on point read.
    base.run_in_txn(async |txn| ep.save_keyspace_meta(txn, &paths, &Keyspace::default()))
        .await
        .unwrap();
    assert!(
        base.load(paths.keyspace_meta_path(0).as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_meta(txn, &paths, 0).await?, None);
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    // Both oneof variants and absence are retained; Go GetId selects path zero for V3.
    for identity in [
        None,
        Some(KeyspaceIdentity::Id(0)),
        Some(KeyspaceIdentity::Scoped {
            namespace_id: u32::MAX,
            keyspace_id: 7,
        }),
        Some(KeyspaceIdentity::Id(u32::MAX)),
    ] {
        let value = Keyspace {
            identity,
            name: "oneof".into(),
            state: 99,
            created_at: i64::MIN,
            state_changed_at: i64::MAX,
            config: [("a".into(), "b".into()), ("c".into(), "d".into())].into(),
        };
        base.run_in_txn(async |txn| ep.save_keyspace_meta(txn, &paths, &value))
            .await
            .unwrap();
        base.run_in_txn(async |txn| {
            assert_eq!(
                ep.load_keyspace_meta(txn, &paths, value.id()).await?,
                Some(value)
            );
            Ok::<_, KeyspaceMetaError>(())
        })
        .await
        .unwrap();
    }
}
#[test]
fn keyspace_meta_values() {
    assert_eq!(Keyspace::default().id(), 0);
    assert_ne!(meta(0, ""), Keyspace::default());
}
#[tokio::test]
async fn keyspace_meta_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn keyspace_meta_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let store = Arc::new(fixture.store().await);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(99);
    let value = meta(0, "restart");
    base.run_in_txn(async |txn| {
        ep.save_keyspace_meta(txn, &paths, &value)?;
        ep.save_keyspace_id(txn, &paths, 0, "restart")
    })
    .await
    .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store = Arc::new(fixture.store().await);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    base.run_in_txn(async |txn| {
        assert_eq!(ep.load_keyspace_meta(txn, &paths, 0).await?, Some(value));
        assert_eq!(ep.load_keyspace_id(txn, &paths, "restart").await?, Some(0));
        Ok::<_, KeyspaceMetaError>(())
    })
    .await
    .unwrap();
    drop(fixture);
    assert!(matches!(
        base.run_in_txn(async |txn| { ep.load_keyspace_meta(txn, &paths, 0).await })
            .await,
        Err(CallbackTxnError::Callback(KeyspaceMetaError::Store(_)))
    ));
    assert!(matches!(
        base.run_in_txn(async |txn| { ep.remove_keyspace(txn, &paths, 0, "restart") })
            .await,
        Err(CallbackTxnError::Store(_))
    ));
}

const GO_WIRE: &[u8] = &[
    0x12, 0x0f, 0x67, 0x72, 0x70, 0x63, 0x5f, 0x6c, 0x6f, 0x61, 0x64, 0x5f, 0x62, 0x79, 0x5f, 0x69,
    0x64, 0x18, 0x01, 0x28, 0x01, 0x3a, 0x24, 0x0a, 0x0a, 0x65, 0x6e, 0x63, 0x72, 0x79, 0x70, 0x74,
    0x69, 0x6f, 0x6e, 0x12, 0x16, 0x74, 0x65, 0x73, 0x74, 0x2d, 0x65, 0x6e, 0x63, 0x72, 0x79, 0x70,
    0x74, 0x69, 0x6f, 0x6e, 0x2d, 0x63, 0x6f, 0x6e, 0x66, 0x69, 0x67, 0x08, 0x04,
];
