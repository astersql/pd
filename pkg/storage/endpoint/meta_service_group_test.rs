// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, MemoryStore, MetaStore};
use std::{collections::BTreeMap, sync::Arc};

// Actual Go SaveMetaServiceGroupStatus bytes captured from isolated PD's etcd.
const GO_ENABLED: &[u8] = br#"{"assignment_count":0,"enabled":true}"#;
const GO_DISABLED: &[u8] = br#"{"assignment_count":0,"enabled":false}"#;

fn ids() -> BTreeMap<String, String> {
    [
        ("group1".into(), "ignored-address".into()),
        ("absent".into(), "".into()),
    ]
    .into()
}

async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let key = paths.meta_service_group_status_path("group1");
    let enabled = MetaServiceGroupStatus {
        assignment_count: 0,
        enabled: true,
    };
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_meta_service_group_status(txn, &paths, &ids())
                .await?["group1"],
            MetaServiceGroupStatus::default()
        );
        ep.save_meta_service_group_status(txn, &paths, "group1", Some(&enabled))?;
        assert_eq!(
            ep.load_meta_service_group_status(txn, &paths, &ids())
                .await?["group1"],
            MetaServiceGroupStatus::default()
        );
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_ENABLED);
    base.run_in_txn(async |txn| {
        let loaded = ep
            .load_meta_service_group_status(txn, &paths, &ids())
            .await?;
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded["group1"], enabled);
        assert_eq!(loaded["absent"], MetaServiceGroupStatus::default());
        ep.remove_meta_service_group_status(txn, &paths, "group1")
    })
    .await
    .unwrap();
    assert!(
        base.load(key.as_bytes()).await.unwrap().is_empty(),
        "deleted group must not retain its status"
    );
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_meta_service_group_status(txn, &paths, &ids())
                .await?["group1"],
            MetaServiceGroupStatus::default()
        );
        ep.save_meta_service_group_status(
            txn,
            &paths,
            "group1",
            Some(&MetaServiceGroupStatus::default()),
        )
    })
    .await
    .unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_DISABLED);
    // Removal and overwrite are atomic and rolled back on callback errors.
    let aborted = base
        .run_in_txn(async |txn| {
            ep.remove_meta_service_group_status(txn, &paths, "group1")?;
            ep.save_meta_service_group_status(txn, &paths, "other", Some(&enabled))?;
            Err::<(), _>(EndpointError::ParseFloat)
        })
        .await;
    assert!(matches!(
        aborted,
        Err(CallbackTxnError::Callback(EndpointError::ParseFloat))
    ));
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_DISABLED);
    assert!(
        base.load(paths.meta_service_group_status_path("other").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    // Batches include missing IDs but never return partial results after bad JSON.
    for raw in [
        b"{bad".as_slice(),
        b"[]",
        b"{\"assignment_count\":1.5}",
        b"{\"enabled\":0}",
        b"{\"assignment_count\":9223372036854775808}",
    ] {
        base.save(key.as_bytes(), raw).await.unwrap();
        assert!(matches!(
            base.run_in_txn(async |txn| ep
                .load_meta_service_group_status(txn, &paths, &ids())
                .await)
                .await,
            Err(CallbackTxnError::Callback(EndpointError::JsonUnmarshal(_)))
        ));
    }
    for raw in [
        b"".as_slice(),
        b"null",
        b"{}",
        b"{\"assignment_count\":null,\"enabled\":null}",
    ] {
        base.save(key.as_bytes(), raw).await.unwrap();
        base.run_in_txn(async |txn| {
            assert_eq!(
                ep.load_meta_service_group_status(txn, &paths, &ids())
                    .await?["group1"],
                MetaServiceGroupStatus::default()
            );
            Ok::<_, EndpointError>(())
        })
        .await
        .unwrap();
    }
    // Go's signed int, case-insensitive field names, duplicate fields and nulls.
    base.save(
        key.as_bytes(),
        br#"{"ASSIGNMENT_COUNT":-9,"enabled":true,"enabled":null,"unknown":{"x":1}}"#,
    )
    .await
    .unwrap();
    let negative = MetaServiceGroupStatus {
        assignment_count: -9,
        enabled: true,
    };
    base.run_in_txn(async |txn| {
        assert_eq!(
            ep.load_meta_service_group_status(txn, &paths, &ids())
                .await?["group1"],
            negative
        );
        assert!(
            ep.load_meta_service_group_status(txn, &paths, &BTreeMap::new())
                .await?
                .is_empty()
        );
        assert_eq!(
            ep.load_meta_service_group_status(txn, &ClusterPaths::new(43), &ids())
                .await?["group1"],
            MetaServiceGroupStatus::default()
        );
        ep.save_meta_service_group_status(txn, &paths, "group1", None)
    })
    .await
    .unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"null");
    // A missing-key read conflicts with concurrent creation, rolling back removal.
    let conflicted = base
        .run_in_txn(async |txn| {
            ep.load_meta_service_group_status(txn, &paths, &ids())
                .await?;
            ep.remove_meta_service_group_status(txn, &paths, "group1")?;
            base.save(
                paths.meta_service_group_status_path("absent").as_bytes(),
                GO_ENABLED,
            )
            .await?;
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(conflicted, Err(CallbackTxnError::Conflict)));
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"null");
    // Deleting missing status is idempotent, and only the designated ID is removed.
    for _ in 0..2 {
        base.run_in_txn(async |txn| ep.remove_meta_service_group_status(txn, &paths, "group1"))
            .await
            .unwrap();
    }
    assert_eq!(
        base.load(paths.meta_service_group_status_path("absent").as_bytes())
            .await
            .unwrap(),
        GO_ENABLED
    );
    // IDs are literal path inputs, matching the canonical Go keypath formatter.
    let unusual = "/a//../键/";
    base.run_in_txn(async |txn| {
        ep.save_meta_service_group_status(txn, &paths, unusual, Some(&negative))
    })
    .await
    .unwrap();
    base.run_in_txn(async |txn| {
        let requested = [(unusual.to_owned(), "ignored".to_owned())].into();
        assert_eq!(
            ep.load_meta_service_group_status(txn, &paths, &requested)
                .await?[unusual],
            negative
        );
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn meta_service_group_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn meta_service_group_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let store = Arc::new(fixture.store().await);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(77);
    let status = MetaServiceGroupStatus {
        assignment_count: 99,
        enabled: true,
    };
    base.run_in_txn(async |txn| {
        ep.save_meta_service_group_status(txn, &paths, "group1", Some(&status))
    })
    .await
    .unwrap();
    let began = std::time::Instant::now();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let recovered_store = fixture.store().await;
    let recovered = Base::new(&recovered_store);
    recovered
        .run_in_txn(async |txn| {
            assert_eq!(
                ep.load_meta_service_group_status(txn, &paths, &ids())
                    .await?["group1"],
                status
            );
            Ok::<_, EndpointError>(())
        })
        .await
        .unwrap();
    eprintln!(
        "meta_service_group_etcd: same-WAL restart restored count=99/enabled=true at {:?}",
        began.elapsed()
    );
    drop(fixture);
    assert!(matches!(
        base.run_in_txn(async |txn| ep.load_meta_service_group_status(txn, &paths, &ids()).await)
            .await,
        Err(CallbackTxnError::Callback(EndpointError::Store(_)))
    ));
    assert!(matches!(
        base.run_in_txn(async |txn| ep.remove_meta_service_group_status(txn, &paths, "group1"))
            .await,
        Err(CallbackTxnError::Store(_))
    ));
    eprintln!(
        "meta_service_group_etcd: stopped backend produces read and deletion commit errors at {:?}",
        began.elapsed()
    );
}

#[test]
fn meta_service_group_go_wire() {
    for raw in [GO_ENABLED, GO_DISABLED] {
        let status: MetaServiceGroupStatus = serde_json::from_slice(raw).unwrap();
        assert_eq!(serde_json::to_vec(&status).unwrap(), raw);
    }
}
