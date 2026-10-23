// Copyright 2026 AsterSQL.

use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;
use tokio::sync::Barrier;

// Actual Go service PUT -> TryStartMaintenanceTaskAtomic -> etcd bytes.
const GO_TASK: &[u8] = r#"{"type":"upgrade","id":"task-26","start_timestamp":1792738044,"description":"维护 \u003c\u003e\u0026 \u2028 end"}"#.as_bytes();

fn task(id: &str) -> MaintenanceTask {
    MaintenanceTask {
        task_type: "upgrade".into(),
        id: id.into(),
        start_timestamp: 1792738044,
        description: "维护 <>& \u{2028} end".into(),
    }
}

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(26);
    let base = Base::new(store.as_ref());
    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];
    for i in 0..10 {
        let store = store.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let candidate = task(&format!("task-{i}"));
            barrier.wait().await;
            let result = StorageEndpoint::new(store)
                .try_start_maintenance_task_atomic(&ClusterPaths::new(26), &candidate)
                .await
                .unwrap();
            (candidate, result)
        }));
    }
    let mut results = vec![];
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    assert_eq!(
        results.iter().filter(|(_, r)| r.0).count(),
        1,
        "one atomic winner"
    );
    let winner = &results.iter().find(|(_, r)| r.0).unwrap().0;
    for (_, (started, existing)) in &results {
        assert_eq!(
            existing.as_ref(),
            if *started { None } else { Some(winner) }
        );
    }
    let snapshot = store
        .read(Read::prefix(
            paths.maintenance_task_path_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(
        snapshot.entries[0].mod_revision,
        snapshot.entries[1].mod_revision
    );
    assert_eq!(
        base.load(
            paths
                .maintenance_task_path(MAINTENANCE_LOCK_NAME)
                .as_bytes()
        )
        .await
        .unwrap(),
        format!("upgrade/{}", winner.id).as_bytes()
    );
    assert_eq!(
        base.load(paths.maintenance_task_path("upgrade").as_bytes())
            .await
            .unwrap(),
        winner.go_json().unwrap()
    );
    let ep = StorageEndpoint::new(store.clone());
    // Repeated ID and different task types conflict with the same global holder.
    for candidate in [
        winner.clone(),
        MaintenanceTask {
            task_type: "repair".into(),
            ..task("other")
        },
    ] {
        assert_eq!(
            ep.try_start_maintenance_task_atomic(&paths, &candidate)
                .await
                .unwrap(),
            (false, Some(winner.clone()))
        );
    }
    assert!(
        base.load(paths.maintenance_task_path("repair").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    // Cluster isolation and byte-for-byte Go JSON persistence.
    let fresh = ClusterPaths::new(27);
    assert_eq!(
        ep.try_start_maintenance_task_atomic(&fresh, &task("task-26"))
            .await
            .unwrap(),
        (true, None)
    );
    assert_eq!(
        base.load(fresh.maintenance_task_path("upgrade").as_bytes())
            .await
            .unwrap(),
        GO_TASK
    );

    let corrupt = ClusterPaths::new(28);
    let lock = corrupt.maintenance_task_path(MAINTENANCE_LOCK_NAME);
    base.save(lock.as_bytes(), b"").await.unwrap(); // exists, even with empty value
    assert_eq!(
        ep.try_start_maintenance_task_atomic(&corrupt, &task("x"))
            .await
            .unwrap(),
        (false, None)
    );
    let key = corrupt.maintenance_task_path("bad");
    for raw in [
        b"{bad".as_slice(),
        b"[]",
        b"{\"start_timestamp\":1.5}",
        b"{\"id\":3}",
        b"{\"start_timestamp\":9223372036854775808}",
    ] {
        base.save(key.as_bytes(), raw).await.unwrap();
        assert_eq!(
            ep.try_start_maintenance_task_atomic(&corrupt, &task("x"))
                .await
                .unwrap(),
            (false, None)
        );
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), raw);
    }
    for raw in [b"null".as_slice(), b"{}", b"{\"id\":null}"] {
        base.save(key.as_bytes(), raw).await.unwrap();
        assert_eq!(
            ep.try_start_maintenance_task_atomic(&corrupt, &task("x"))
                .await
                .unwrap(),
            (false, Some(MaintenanceTask::default()))
        );
    }
    base.save(
        key.as_bytes(),
        br#"{"TYPE":"repair","ID":"old","id":null,"start_timestamp":-9,"unknown":true}"#,
    )
    .await
    .unwrap();
    let previous = MaintenanceTask {
        task_type: "repair".into(),
        id: "old".into(),
        start_timestamp: -9,
        description: String::new(),
    };
    assert_eq!(
        ep.try_start_maintenance_task_atomic(&corrupt, &task("x"))
            .await
            .unwrap(),
        (false, Some(previous))
    );
    base.save(corrupt.maintenance_task_path("other").as_bytes(), GO_TASK)
        .await
        .unwrap();
    let err = ep
        .try_start_maintenance_task_atomic(&corrupt, &task("x"))
        .await
        .unwrap_err();
    assert!(matches!(err, MaintenanceStartError::MultipleTasks(2)));
    assert_eq!(
        err.to_string(),
        "multiple maintenance tasks running simultaneously: found 2 tasks"
    );
    assert_eq!(base.load(lock.as_bytes()).await.unwrap(), b"");

    // Go bounds the conflict snapshot to the first 100 keys, including the lock.
    let capped = ClusterPaths::new(29);
    base.save(
        capped
            .maintenance_task_path(MAINTENANCE_LOCK_NAME)
            .as_bytes(),
        b"held",
    )
    .await
    .unwrap();
    for i in 0..101 {
        base.save(
            capped.maintenance_task_path(&format!("a{i:03}")).as_bytes(),
            b"bad",
        )
        .await
        .unwrap();
    }
    base.save(capped.maintenance_task_path("zvalid").as_bytes(), GO_TASK)
        .await
        .unwrap();
    assert_eq!(
        ep.try_start_maintenance_task_atomic(&capped, &task("x"))
            .await
            .unwrap(),
        (false, None)
    );
}

#[tokio::test]
async fn maintenance_start_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}

#[tokio::test]
async fn maintenance_start_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let store = Arc::new(fixture.store().await);
    let ep = StorageEndpoint::new(store);
    let paths = ClusterPaths::new(30);
    assert_eq!(
        ep.try_start_maintenance_task_atomic(&paths, &task("task-26"))
            .await
            .unwrap(),
        (true, None)
    );
    let began = std::time::Instant::now();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let restarted = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        restarted
            .try_start_maintenance_task_atomic(&paths, &task("loser"))
            .await
            .unwrap(),
        (false, Some(task("task-26")))
    );
    eprintln!(
        "maintenance_start_etcd: same-WAL restart retains holder at {:?}",
        began.elapsed()
    );
    drop(fixture);
    assert!(matches!(
        restarted
            .try_start_maintenance_task_atomic(&paths, &task("offline"))
            .await,
        Err(MaintenanceStartError::Endpoint(EndpointError::Store(_)))
    ));
    eprintln!(
        "maintenance_start_etcd: stopped backend returns error at {:?}",
        began.elapsed()
    );
}

#[test]
fn maintenance_start_go_wire() {
    let value: MaintenanceTask = serde_json::from_slice(GO_TASK).unwrap();
    assert_eq!(value, task("task-26"));
    assert_eq!(value.go_json().unwrap(), GO_TASK);
}
