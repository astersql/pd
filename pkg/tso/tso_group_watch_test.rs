// Copyright 2026 AsterSQL.
use super::*;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
pub(super) mod fixture;
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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_073_deleted_group_cannot_issue_before_watch_delivery() {
    let f = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let address = "http://handoff-source".to_owned();
    write_group(store.as_ref(), 1, vec![101], vec![address.clone()]).await;
    let w = GroupAllocatorWatch::load(config(store.clone(), address))
        .await
        .unwrap();
    let before = timestamp(&w, 1).await;
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
    let stale = w.allocator(1).unwrap().allocate(1).await;
    w.shutdown().await.unwrap();
    assert!(
        stale.is_err(),
        "removed source issued {stale:?} after {before:?} before its watch was polled"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_076_cancelled_apply_replays_batch() {
    let mut f = fixture::Fixture::start();
    let gate = Arc::new(gated_revoke::Store {
        inner: Arc::new(f.store().await),
        blocked: false.into(),
        entered: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    let store: Arc<dyn MetaStore> = gate.clone();
    let address = "http://apply-replay".to_owned();
    write_group(store.as_ref(), 1, vec![101], vec![address.clone()]).await;
    let mut w = GroupAllocatorWatch::load(config(store.clone(), address.clone()))
        .await
        .unwrap();
    timestamp(&w, 1).await;
    // Establish the real watch before the event; then make lease revocation wait on the actual network.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), w.next())
            .await
            .is_err()
    );
    gate.blocked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let value = serde_json::json!({"id":1,"keyspaces":[101],"members":[{"address":address,"priority":0}],"merge-state":{"merge-list":[2]}});
    let revision = store
        .transact(
            vec![],
            vec![Mutation::put(
                ClusterPaths::new(760).keyspace_group_id_path(1).as_bytes(),
                &serde_json::to_vec(&value).unwrap(),
            )],
        )
        .await
        .unwrap()
        .revision;
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .unwrap();
    let outage = f.suspend();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), w.next())
            .await
            .is_err()
    );
    drop(outage);
    gate.blocked
        .store(false, std::sync::atomic::Ordering::SeqCst);
    gate.release.notify_waiters();
    tokio::time::timeout(Duration::from_secs(2), w.next())
        .await
        .expect("a cancelled resource application must retain its consumed batch")
        .unwrap();
    assert_eq!(w.revision(), revision);
    assert!(w.group(1).unwrap().is_merge_target());
    assert!(w.allocator(1).unwrap().allocate(1).await.is_err());
    w.shutdown().await.unwrap();
}
mod gated_revoke {
    use astersql_pd_storage_domain::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    pub struct Store {
        pub inner: Arc<dyn MetaStore>,
        pub blocked: AtomicBool,
        pub entered: tokio::sync::Notify,
        pub release: tokio::sync::Notify,
    }
    #[async_trait::async_trait]
    impl MetaStore for Store {
        async fn read(&self, r: Read) -> Result<Snapshot> {
            self.inner.read(r).await
        }
        async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> {
            self.inner.transact(c, w).await
        }
        async fn transact_branches(
            &self,
            c: Vec<Compare>,
            t: Vec<Mutation>,
            e: Vec<Mutation>,
        ) -> Result<Commit> {
            self.inner.transact_branches(c, t, e).await
        }
        async fn transact_raw(
            &self,
            c: Vec<Compare>,
            t: Vec<RawTxnOp>,
            e: Vec<RawTxnOp>,
        ) -> Result<RawTxnResponse> {
            self.inner.transact_raw(c, t, e).await
        }
        async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> {
            self.inner.watch(p, r).await
        }
        async fn grant(&self, t: i64) -> Result<Lease> {
            self.inner.grant(t).await
        }
        async fn keep_alive(&self, l: LeaseId) -> Result<Lease> {
            self.inner.keep_alive(l).await
        }
        async fn revoke(&self, l: LeaseId) -> Result<()> {
            while self.blocked.load(Ordering::SeqCst) {
                let resume = self.release.notified();
                self.entered.notify_one();
                resume.await;
            }
            self.inner.revoke(l).await
        }
        async fn ttl(&self, l: LeaseId) -> Result<i64> {
            self.inner.ttl(l).await
        }
        async fn compact(&self, r: Revision) -> Result<()> {
            self.inner.compact(r).await
        }
    }
}
