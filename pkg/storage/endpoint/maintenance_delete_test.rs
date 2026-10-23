// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;
use tokio::sync::Barrier;

// Actual Go service persisted bytes from batch 45, reused without a new Go oracle.
const GO_TASK: &[u8] = r#"{"type":"upgrade","id":"task-26","start_timestamp":1792738044,"description":"维护 \u003c\u003e\u0026 \u2028 end"}"#.as_bytes();
fn task(id: &str) -> MaintenanceTask {
    MaintenanceTask {
        id: id.into(),
        ..serde_json::from_slice(GO_TASK).unwrap()
    }
}

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(270);
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let lock = paths.maintenance_task_path(MAINTENANCE_LOCK_NAME);
    let key = paths.maintenance_task_path("upgrade");
    assert_eq!(
        ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
        None
    );
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "missing")
            .await
            .unwrap(),
        (false, None)
    );
    ep.try_start_maintenance_task_atomic(&paths, &task("task-26"))
        .await
        .unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_TASK);
    let before = store
        .read(Read::prefix(
            paths.maintenance_task_path_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "stale")
            .await
            .unwrap(),
        (false, Some(task("task-26"))),
        "old ID cannot delete current task"
    );
    let after = store
        .read(Read::prefix(
            paths.maintenance_task_path_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(
        before.entries, after.entries,
        "failed delete preserves both values and revisions"
    );
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "repair", "task-26")
            .await
            .unwrap(),
        (false, None)
    );
    let mut all = vec![];
    ep.load_all_maintenance_tasks(&paths, |k, v| all.push((k.to_vec(), v.to_vec())))
        .await
        .unwrap();
    assert_eq!(
        all,
        vec![
            (b"maintenance_lock".to_vec(), b"upgrade/task-26".to_vec()),
            (b"upgrade".to_vec(), GO_TASK.to_vec())
        ]
    );
    assert_eq!(
        ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
        Some(task("task-26"))
    );
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "task-26")
            .await
            .unwrap(),
        (true, None)
    );
    assert!(base.load(lock.as_bytes()).await.unwrap().is_empty());
    assert_eq!(
        ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
        None
    );
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "task-26")
            .await
            .unwrap(),
        (false, None)
    );
    ep.try_start_maintenance_task_atomic(&paths, &task("new"))
        .await
        .unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "task-26")
            .await
            .unwrap(),
        (false, Some(task("new")))
    );
    ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "new")
        .await
        .unwrap();

    // Missing lock cannot authorize deletion of an orphan. Conflict matching uses JSON type.
    base.save(key.as_bytes(), GO_TASK).await.unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "task-26")
            .await
            .unwrap(),
        (false, Some(task("task-26")))
    );
    for raw in [b"".as_slice(), b"bad", b"[]", br#"{"id":3}"#] {
        base.save(key.as_bytes(), raw).await.unwrap();
        assert_eq!(
            ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "x")
                .await
                .unwrap(),
            (false, None)
        );
        if raw.is_empty() {
            assert_eq!(
                ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
                None
            );
        } else {
            assert!(matches!(
                ep.load_maintenance_task(&paths, "upgrade").await,
                Err(EndpointError::JsonUnmarshal(_))
            ));
        }
    }
    base.save(key.as_bytes(), b"null").await.unwrap();
    assert_eq!(
        ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
        Some(MaintenanceTask::default())
    );
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "", "x")
            .await
            .unwrap(),
        (false, Some(MaintenanceTask::default()))
    );
    base.save(key.as_bytes(), GO_TASK).await.unwrap();
    base.save(paths.maintenance_task_path("aaa").as_bytes(), GO_TASK)
        .await
        .unwrap();
    // Multiple valid tasks do not produce the start API's MultipleTasks error.
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "x")
            .await
            .unwrap(),
        (false, Some(task("task-26")))
    );
    base.save(lock.as_bytes(), b"upgrade/owner").await.unwrap();
    base.save(key.as_bytes(), b"bad").await.unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "owner")
            .await
            .unwrap(),
        (true, None),
        "lock value alone authorizes deletion, even with corrupt task JSON"
    );
    assert_eq!(
        base.load(paths.maintenance_task_path("aaa").as_bytes())
            .await
            .unwrap(),
        GO_TASK
    );

    // The exact lock is authoritative even when the task key is absent.
    base.save(lock.as_bytes(), b"upgrade/absent").await.unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "absent")
            .await
            .unwrap(),
        (true, None)
    );
    base.save(lock.as_bytes(), b"").await.unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "")
            .await
            .unwrap(),
        (false, Some(task("task-26")))
    );
    assert_eq!(base.load(lock.as_bytes()).await.unwrap(), b"");

    // Read-all paginates, retaining raw malformed values and the lock; conflict reads cap at 100.
    let capped = ClusterPaths::new(271);
    for i in 0..205 {
        base.save(
            capped.maintenance_task_path(&format!("a{i:03}")).as_bytes(),
            b"bad",
        )
        .await
        .unwrap();
    }
    base.save(capped.maintenance_task_path("z").as_bytes(), GO_TASK)
        .await
        .unwrap();
    base.save(
        capped
            .maintenance_task_path(MAINTENANCE_LOCK_NAME)
            .as_bytes(),
        b"held",
    )
    .await
    .unwrap();
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&capped, "upgrade", "x")
            .await
            .unwrap(),
        (false, None)
    );
    let mut all = vec![];
    ep.load_all_maintenance_tasks(&capped, |k, v| all.push((k.to_vec(), v.to_vec())))
        .await
        .unwrap();
    assert_eq!(all.len(), 207);
    assert_eq!(all.last().unwrap(), &(b"z".to_vec(), GO_TASK.to_vec()));
    assert_eq!(
        ep.load_maintenance_task(&ClusterPaths::new(999), "z")
            .await
            .unwrap(),
        None
    );

    // Matching delete and replacement start share a transaction serialization order.
    for i in 0..10 {
        let paths = ClusterPaths::new(300 + i);
        ep.try_start_maintenance_task_atomic(&paths, &task("old"))
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let s = store.clone();
        let b = barrier.clone();
        let p = paths.clone();
        let start = tokio::spawn(async move {
            b.wait().await;
            StorageEndpoint::new(s)
                .try_start_maintenance_task_atomic(&p, &task("new"))
                .await
                .unwrap()
        });
        barrier.wait().await;
        assert_eq!(
            ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "old")
                .await
                .unwrap(),
            (true, None)
        );
        let (started, holder) = start.await.unwrap();
        let snapshot = store
            .read(Read::prefix(
                paths.maintenance_task_path_prefix().as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        if started {
            assert_eq!(holder, None);
            assert_eq!(snapshot.entries.len(), 2);
            assert_eq!(
                snapshot.entries[0].mod_revision,
                snapshot.entries[1].mod_revision
            );
            assert_eq!(
                ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
                Some(task("new"))
            );
        } else {
            assert_eq!(holder, Some(task("old")));
            assert!(snapshot.entries.is_empty());
        }
    }

    // A stale delete races a start after release: it must never delete the new holder.
    for i in 0..10 {
        let paths = ClusterPaths::new(280 + i);
        ep.try_start_maintenance_task_atomic(&paths, &task("old"))
            .await
            .unwrap();
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "old")
            .await
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let s = store.clone();
        let b = barrier.clone();
        let p = paths.clone();
        let start = tokio::spawn(async move {
            b.wait().await;
            StorageEndpoint::new(s)
                .try_start_maintenance_task_atomic(&p, &task("new"))
                .await
                .unwrap()
        });
        barrier.wait().await;
        let deleted = ep
            .try_delete_maintenance_task_atomic(&paths, "upgrade", "old")
            .await
            .unwrap();
        assert!(!deleted.0);
        if let Some(holder) = deleted.1 {
            assert_eq!(holder, task("new"));
        }
        assert_eq!(start.await.unwrap(), (true, None));
        assert_eq!(
            ep.load_maintenance_task(&paths, "upgrade").await.unwrap(),
            Some(task("new"))
        );
        assert_eq!(
            base.load(
                paths
                    .maintenance_task_path(MAINTENANCE_LOCK_NAME)
                    .as_bytes()
            )
            .await
            .unwrap(),
            b"upgrade/new"
        );
    }
}

#[tokio::test]
async fn maintenance_delete_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn maintenance_delete_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(299);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    ep.try_start_maintenance_task_atomic(&paths, &task("restart"))
        .await
        .unwrap();
    let began = std::time::Instant::now();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "stale")
            .await
            .unwrap(),
        (false, Some(task("restart")))
    );
    assert_eq!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "restart")
            .await
            .unwrap(),
        (true, None)
    );
    eprintln!(
        "maintenance_delete_etcd: same WAL retained owner, then deleted at {:?}",
        began.elapsed()
    );
    drop(fixture);
    assert!(matches!(
        ep.try_delete_maintenance_task_atomic(&paths, "upgrade", "restart")
            .await,
        Err(EndpointError::Store(_))
    ));
    assert!(ep.load_maintenance_task(&paths, "upgrade").await.is_err());
    assert!(
        ep.load_all_maintenance_tasks(&paths, |_, _| {})
            .await
            .is_err()
    );
    eprintln!(
        "maintenance_delete_etcd: stopped backend returns errors at {:?}",
        began.elapsed()
    );
}
#[test]
fn maintenance_delete_go_wire() {
    let value: MaintenanceTask = serde_json::from_slice(GO_TASK).unwrap();
    assert_eq!(value, task("task-26"));
    assert_eq!(value.go_json().unwrap(), GO_TASK);
}
