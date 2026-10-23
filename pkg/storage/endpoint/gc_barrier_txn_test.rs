// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

// Actual Go output from /tmp/pd-gc28-go-wire.json, recorded by task 28.
const GO_WIRE: &[&str] = &[
    r#"{"barrier_id":"屏障 \u003c\u003e\u0026 \u2028 end3600","barrier_ts":18446744073709551614,"expiration_time":1792742540}"#,
    r#"{"barrier_id":"屏障 \u003c\u003e\u0026 \u2028 end9223372036854775807","barrier_ts":18446744073709551614,"expiration_time":0}"#,
    r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end03600","expired_at":1792742540,"safe_point":18446744073709551614}"#,
    r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end09223372036854775807","expired_at":9223372036854775807,"safe_point":18446744073709551614}"#,
    r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 endNone3600","expired_at":1792742540,"safe_point":18446744073709551614}"#,
    r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 endNone9223372036854775807","expired_at":9223372036854775807,"safe_point":18446744073709551614}"#,
    r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end13600","expired_at":1792742540,"safe_point":18446744073709551614,"keyspace_id":1}"#,
    r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end19223372036854775807","expired_at":9223372036854775807,"safe_point":18446744073709551614,"keyspace_id":1}"#,
];

async fn snapshot(store: &dyn MetaStore) -> Vec<astersql_pd_storage_kv::Entry> {
    store
        .read(Read::prefix(b"/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
}

async fn seed(ep: &StorageEndpoint, paths: &ClusterPaths) {
    ep.run_in_gc_state_transaction(paths, async |wb| {
        wb.set_global_gc_barrier(&GlobalGCBarrier::new("remove", 1, None))?;
        for scope in [NULL_KEYSPACE_ID, 0, 1] {
            wb.set_gc_barrier(scope, &GCBarrier::new("remove", 1, None))?;
        }
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
}

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(32);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    seed(&ep, &paths).await;
    assert!(
        ep.load_global_gc_barrier(&paths, "remove")
            .await
            .unwrap()
            .is_some()
    );
    ep.run_in_gc_state_transaction(&paths, async |wb| {
        wb.delete_global_gc_barrier("remove");
        for scope in [NULL_KEYSPACE_ID, 0, 1] {
            wb.delete_gc_barrier(scope, "remove");
        }
        // Each saved Go value is decoded independently and persisted via the new batch API.
        for (i, wire) in GO_WIRE.iter().enumerate() {
            if i < 2 {
                let barrier = GlobalGCBarrier::from_json(wire.as_bytes()).unwrap();
                wb.set_global_gc_barrier(&barrier)?;
            } else {
                let ssp = ServiceSafePoint::from_json(wire.as_bytes()).unwrap();
                wb.set_gc_barrier(ssp.keyspace_id, &GCBarrier::from_service_safe_point(&ssp))?;
            }
        }
        wb.set_gc_safe_point(1111, 456139133457530881);
        wb.set_txn_safe_point(NULL_KEYSPACE_ID, 456139133457530882);
        // No write in this callback is visible before commit.
        assert!(
            ep.load_global_gc_barrier(&paths, "remove")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(ep.load_gc_safe_point(&paths, 1111).await.unwrap(), 0);
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    assert!(
        ep.load_global_gc_barrier(&paths, "remove")
            .await
            .unwrap()
            .is_none()
    );
    for scope in [NULL_KEYSPACE_ID, 0, 1] {
        assert!(
            ep.load_gc_barrier(&paths, scope, "remove")
                .await
                .unwrap()
                .is_none()
        );
    }
    for (i, wire) in GO_WIRE.iter().enumerate() {
        let key = if i < 2 {
            paths.global_gc_barrier_path(
                &GlobalGCBarrier::from_json(wire.as_bytes())
                    .unwrap()
                    .barrier_id,
            )
        } else {
            let ssp = ServiceSafePoint::from_json(wire.as_bytes()).unwrap();
            paths.gc_barrier_path(ssp.keyspace_id, &ssp.service_id)
        };
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), wire.as_bytes());
    }
    assert_eq!(
        base.load(paths.gc_safe_point_path(1111).as_bytes())
            .await
            .unwrap(),
        br#"{"keyspace_id":1111,"safe_point":456139133457530881}"#
    );
    assert_eq!(
        base.load(paths.txn_safe_point_path(NULL_KEYSPACE_ID).as_bytes())
            .await
            .unwrap(),
        b"456139133457530882"
    );
    let before = snapshot(store.as_ref()).await;
    assert_eq!(before.len(), 11);
    assert!(before.iter().all(|e| e.lease.is_none()));
    assert!(
        before
            .iter()
            .all(|e| e.mod_revision == before[0].mod_revision)
    );
    assert_eq!(
        base.load(paths.gc_state_revision_path().as_bytes())
            .await
            .unwrap(),
        b"2"
    );
    let err = ep
        .run_in_gc_state_transaction(&paths, async |wb| {
            wb.delete_global_gc_barrier("屏障 <>& \u{2028} end3600");
            wb.delete_gc_barrier(1, "屏障 <>& \u{2028} end13600");
            wb.set_gc_barrier(0, &GCBarrier::new("rollback", 9, None))?;
            wb.set_gc_safe_point(1111, 0);
            Err::<(), _>(EndpointError::JsonMarshal(
                serde_json::from_str::<u64>("sentinel").unwrap_err(),
            ))
        })
        .await
        .unwrap_err();
    assert!(matches!(err, GCStateTxnError::Callback(_)));
    assert_eq!(snapshot(store.as_ref()).await, before);
    let err = ep
        .run_in_gc_state_transaction(&paths, async |wb| {
            wb.delete_global_gc_barrier("屏障 <>& \u{2028} end3600");
            wb.delete_gc_barrier(1, "屏障 <>& \u{2028} end13600");
            wb.set_global_gc_barrier(&GlobalGCBarrier::new("loser", 99, None))?;
            wb.set_txn_safe_point(NULL_KEYSPACE_ID, 99);
            ep.run_in_gc_state_transaction(&paths, async |inner| {
                inner.set_gc_barrier(0, &GCBarrier::new("winner", 7, None))?;
                Ok::<_, EndpointError>(())
            })
            .await
            .unwrap();
            Ok::<_, EndpointError>(())
        })
        .await
        .unwrap_err();
    assert!(matches!(err, GCStateTxnError::Conflict));
    assert!(
        ep.load_global_gc_barrier(&paths, "loser")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ep.load_gc_barrier(&paths, 0, "winner")
            .await
            .unwrap()
            .is_some()
    );
    for e in before
        .iter()
        .filter(|e| e.key != paths.gc_state_revision_path().as_bytes())
    {
        assert_eq!(base.load(&e.key).await.unwrap(), e.value);
    }
    // Expiration is persisted metadata: expired entries have no lease or eager cleanup.
    let mut expiration = BarrierTime::from_unix(1, 0);
    let expired = GCBarrier::new("gc_worker", 0, Some(&mut expiration));
    ep.run_in_gc_state_transaction(&paths, async |wb| {
        wb.set_gc_barrier(0, &expired)?;
        wb.set_global_gc_barrier(&GlobalGCBarrier::new("expired", 0, Some(&mut expiration)))?;
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    assert!(
        ep.load_gc_barrier(&paths, 0, "gc_worker")
            .await
            .unwrap()
            .unwrap()
            .is_expired(BarrierTime::from_unix(2, 0))
    );
    assert!(
        ep.load_global_gc_barrier(&paths, "expired")
            .await
            .unwrap()
            .unwrap()
            .is_expired(BarrierTime::from_unix(2, 0))
    );
    // A delete of an absent key still makes a nonempty batch and increments once.
    ep.run_in_gc_state_transaction(&paths, async |wb| {
        wb.delete_gc_barrier(0, "absent");
        wb.delete_global_gc_barrier("absent");
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    assert_eq!(
        base.load(paths.gc_state_revision_path().as_bytes())
            .await
            .unwrap(),
        b"5"
    );
}

#[tokio::test]
async fn gc_barrier_txn_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn gc_barrier_txn_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    contract(store.clone()).await;
    let paths = ClusterPaths::new(32);
    let ep = StorageEndpoint::new(store.clone());
    let before = snapshot(store.as_ref()).await;
    let began = std::time::Instant::now();
    let result = ep
        .run_in_gc_state_transaction(&paths, async |wb| {
            wb.delete_gc_barrier(0, "gc_worker");
            wb.delete_global_gc_barrier("expired");
            wb.set_global_gc_barrier(&GlobalGCBarrier::new("offline", 9, None))?;
            wb.set_gc_safe_point(1111, 0);
            fixture.command("stop", "stopped");
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(result, Err(GCStateTxnError::Store(_))));
    fixture.command("start", "restarted");
    assert_eq!(snapshot(store.as_ref()).await, before);
    eprintln!(
        "gc_barrier_txn: stopped after revision read before mixed commit, backend error, same-WAL restart unchanged (including expired barriers); elapsed {:?}",
        began.elapsed()
    );
}

#[test]
fn gc_barrier_txn_go_values() {
    for (i, wire) in GO_WIRE.iter().enumerate() {
        let actual = if i < 2 {
            GlobalGCBarrier::from_json(wire.as_bytes())
                .unwrap()
                .to_json()
                .unwrap()
        } else {
            ServiceSafePoint::from_json(wire.as_bytes())
                .unwrap()
                .to_json()
                .unwrap()
        };
        assert_eq!(actual, wire.as_bytes());
    }
}
