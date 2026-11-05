// Copyright 2026 AsterSQL.
use super::*;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;
fn config(store: Arc<dyn astersql_pd_storage_kv::MetaStore>, address: String) -> GroupWatchConfig {
    GroupWatchConfig {
        store,
        runtime: tokio::runtime::Handle::current(),
        cancel: tokio_util::sync::CancellationToken::new(),
        cluster_id: 760,
        address,
        bootstrap_keyspace: 0,
        participant: Arc::new(|id| format!("service-{id}").into_bytes()),
        clock: Arc::new(SystemClock),
        lease_seconds: 3,
        save_interval_millis: 1000,
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_076_behavior() {
    let f = fixture::Fixture::start();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let watch = GroupAllocatorWatch::load(config(
        Arc::new(f.store().await),
        format!("http://{}", listener.local_addr().unwrap()),
    ))
    .await
    .unwrap();
    assert_eq!(
        watch.assigned_ids(),
        vec![0],
        "absent default metadata must create a real default allocator"
    );
    watch.shutdown().await.unwrap();
}

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Consistency, MetaStore, Mutation, Read};
use std::time::Duration;
async fn timestamp(w: &GroupAllocatorWatch, id: u32) -> Timestamp {
    for _ in 0..200 {
        if let Ok(t) = w.allocator(id).unwrap().allocate(1).await {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("allocator {id} did not become serving");
}
async fn write_group(store: &dyn MetaStore, id: u32, keys: Vec<u32>, nodes: Vec<String>) -> i64 {
    let value = serde_json::json!({"id":id,"keyspaces":keys,"members":nodes.iter().map(|n|serde_json::json!({"address":n,"priority":0})).collect::<Vec<_>>()});
    store
        .transact(
            vec![],
            vec![Mutation::put(
                ClusterPaths::new(760).keyspace_group_id_path(id).as_bytes(),
                &serde_json::to_vec(&value).unwrap(),
            )],
        )
        .await
        .unwrap()
        .revision
}
async fn catch_up(w: &mut GroupAllocatorWatch, revision: i64) {
    while w.revision() < revision {
        match tokio::time::timeout(Duration::from_secs(5), w.next())
            .await
            .unwrap()
        {
            Ok(_) => {}
            Err(e) => panic!("watch failed: {e:?}"),
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_072_real_allocators_and_go_watch_sequence() {
    let f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let mut w = GroupAllocatorWatch::load(config(store.clone(), address.clone()))
        .await
        .unwrap();
    assert_eq!(w.assigned_ids(), vec![0]);
    let t0 = timestamp(&w, 0).await;
    // Exact assignment/keyspace sequence in Go TestWatchAndDynamicallyApplyChanges.
    let events = vec![
        (0, Some(vec![address.clone()])),
        (1, Some(vec!["unknown".into(), address.clone()])),
        (2, Some(vec!["unknown".into()])),
        (3, Some(vec![address.clone()])),
        (0, None),
        (4, Some(vec![])),
        (5, Some(vec![])),
        (2, Some(vec![address.clone()])),
        (3, Some(vec![])),
        (4, Some(vec![address.clone()])),
        (2, None),
    ];
    let mut last = 0;
    for (id, nodes) in events {
        last = if let Some(nodes) = nodes {
            write_group(store.as_ref(), id, vec![id], nodes).await
        } else {
            store
                .transact(
                    vec![],
                    vec![Mutation::Delete {
                        key: ClusterPaths::new(760)
                            .keyspace_group_id_path(id)
                            .into_bytes(),
                    }],
                )
                .await
                .unwrap()
                .revision
        };
    }
    catch_up(&mut w, last).await;
    assert_eq!(w.assigned_ids(), vec![0, 1, 4]);
    assert_eq!(w.loaded_ids(), vec![0, 1, 3, 4, 5]);
    assert!(timestamp(&w, 0).await > t0);
    timestamp(&w, 1).await;
    timestamp(&w, 4).await;
    // Separate groups reserve separate durable timestamp paths, rather than aliasing classic TSO.
    for id in [0, 1, 4] {
        assert_eq!(
            store
                .read(Read::range(
                    ClusterPaths::new(760).timestamp_path(id).as_bytes(),
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
                .len(),
            1
        );
    }
    let id = 1;
    let key = ClusterPaths::new(760).election_path(Some(&astersql_pd_keypath::MsParam {
        service_name: "tso".into(),
        group_id: id,
    }));
    assert_eq!(
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .len(),
        1
    );
    let rev = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: ClusterPaths::new(760)
                    .keyspace_group_id_path(id)
                    .into_bytes(),
            }],
        )
        .await
        .unwrap()
        .revision;
    catch_up(&mut w, rev).await;
    assert!(w.allocator(id).is_none());
    assert!(
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty(),
        "deleted allocator must revoke its real election lease before publication"
    );
    assert_eq!(
        w.group(0).unwrap().keyspaces.as_deref(),
        Some([0].as_slice())
    );
    w.shutdown().await.unwrap();
    let elections = store
        .read(Read::prefix(
            format!("/ms/760/tso/").as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert!(
        elections
            .entries
            .iter()
            .all(|e| !e.key.ends_with(b"/primary"))
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_073_lease_handoff_and_wal() {
    let mut f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let a = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let a = format!("http://{}", a.local_addr().unwrap());
    let b = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let b = format!("http://{}", b.local_addr().unwrap());
    write_group(store.as_ref(), 1, vec![11], vec![a.clone(), b.clone()]).await;
    let first = GroupAllocatorWatch::load(config(store.clone(), a.clone()))
        .await
        .unwrap();
    let t = timestamp(&first, 1).await;
    let mut second_config = config(store.clone(), b);
    second_config.participant = Arc::new(|id| format!("second-{id}").into_bytes());
    let second = GroupAllocatorWatch::load(second_config).await.unwrap();
    assert!(second.allocator(1).unwrap().allocate(1).await.is_err());
    first.shutdown().await.unwrap();
    let next = timestamp(&second, 1).await;
    assert!(next > t);
    // Delete the live term's real lease at the network boundary; cached timestamps must fail closed.
    let key = ClusterPaths::new(760).election_path(Some(&astersql_pd_keypath::MsParam {
        service_name: "tso".into(),
        group_id: 1,
    }));
    let row = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
        .remove(0);
    store.revoke(row.lease.unwrap()).await.unwrap();
    assert!(second.allocator(1).unwrap().allocate(1).await.is_err());
    second.shutdown().await.unwrap();
    f.command("stop", "stopped");
    f.command("start", "ready");
    let restored = GroupAllocatorWatch::load(config(store.clone(), a))
        .await
        .unwrap();
    let recovered = timestamp(&restored, 1).await;
    assert!(recovered > next);
    println!("mcs073 timestamps before={t:?} handoff={next:?} after_wal={recovered:?}");
    restored.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_076_outage_compaction_and_invalid_repair() {
    let mut f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    write_group(store.as_ref(), 1, vec![11], vec![address.clone()]).await;
    let mut w = GroupAllocatorWatch::load(config(store.clone(), address.clone()))
        .await
        .unwrap();
    let before = timestamp(&w, 1).await;
    f.command("stop", "stopped");
    assert!(w.allocator(1).unwrap().allocate(1).await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(8), w.next())
            .await
            .unwrap()
            .is_err()
    );
    f.command("start", "ready");
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: ClusterPaths::new(760)
                    .keyspace_group_id_path(1)
                    .into_bytes(),
            }],
        )
        .await
        .unwrap();
    let rev = write_group(store.as_ref(), 2, vec![22], vec![address.clone()]).await;
    store.compact(rev).await.unwrap();
    for _ in 0..10 {
        if w.revision() >= rev {
            break;
        }
        let _ = tokio::time::timeout(Duration::from_secs(8), w.next())
            .await
            .unwrap();
    }
    assert!(w.revision() >= rev);
    assert_eq!(w.assigned_ids(), vec![0, 2]);
    println!(
        "mcs076 compacted revision={rev} published={} assigned={:?} loaded={:?}",
        w.revision(),
        w.assigned_ids(),
        w.loaded_ids()
    );
    assert!(w.allocator(1).is_none());
    timestamp(&w, 2).await;
    let old_revision = w.revision();
    store
        .transact(
            vec![],
            vec![Mutation::put(
                ClusterPaths::new(760).keyspace_group_id_path(3).as_bytes(),
                b"broken",
            )],
        )
        .await
        .unwrap();
    assert!(w.next().await.is_err());
    assert_eq!(w.revision(), old_revision);
    assert_eq!(w.assigned_ids(), vec![0, 2]);
    write_group(store.as_ref(), 3, vec![33], vec![address]).await;
    w.reload().await.unwrap();
    assert_eq!(w.assigned_ids(), vec![0, 2, 3]);
    assert!(timestamp(&w, 3).await.physical >= before.physical);
    w.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_072_reserved_keyspace_and_assignment_recovery() {
    let f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    write_group(store.as_ref(), 0, vec![0, 1, 2], vec![address.clone()]).await;
    write_group(store.as_ref(), 3, vec![3, 4], vec![address.clone()]).await;
    let mut w = GroupAllocatorWatch::load(config(store.clone(), address.clone()))
        .await
        .unwrap();
    write_group(store.as_ref(), 0, vec![1, 2], vec![address.clone()]).await;
    let revision = write_group(store.as_ref(), 3, vec![0, 3, 4], vec![address.clone()]).await;
    catch_up(&mut w, revision).await;
    assert_eq!(
        w.group(0).unwrap().keyspaces.as_deref(),
        Some([0, 1, 2].as_slice())
    );
    assert_eq!(
        w.group(3).unwrap().keyspaces.as_deref(),
        Some([3, 4].as_slice())
    );
    let revision = write_group(store.as_ref(), 0, vec![0], vec!["unknown".into()]).await;
    catch_up(&mut w, revision).await;
    assert!(w.allocator(0).is_none());
    let revision = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: ClusterPaths::new(760)
                    .keyspace_group_id_path(0)
                    .into_bytes(),
            }],
        )
        .await
        .unwrap()
        .revision;
    catch_up(&mut w, revision).await;
    assert!(w.allocator(0).is_some());
    timestamp(&w, 0).await;
    w.shutdown().await.unwrap();
}
