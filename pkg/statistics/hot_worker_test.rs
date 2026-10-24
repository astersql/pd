// Copyright 2026 AsterSQL.
use crate::{HotCache, HotPeerCache, HotTask, HotTaskError, HotTaskOutput, new_region_flow_tasks};
use astersql_pd_core::{Peer, QueryStats, RegionInfo, RegionMeta};
use astersql_pd_statistics_utils::RWType;

fn region(n: u64) -> RegionInfo {
    let peers: Vec<_> = (1..=n)
        .map(|store_id| Peer {
            id: 10 + store_id,
            store_id,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        vec![],
        vec![],
    );
    r.stats_mut().written_bytes = 1 << 20;
    r.stats_mut().written_keys = 1024;
    r.set_report_interval(0, 60);
    r
}
fn snapshot(cache: &HotPeerCache) -> String {
    format!("{:?}", cache.peers)
}
fn original(cache: &mut HotPeerCache, r: &RegionInfo, now: i64) {
    for p in cache.collect_expired_items(r) {
        cache.update_stat_at(p, now);
    }
    for p in cache.check_peer_flow_dynamic(r, None, &r.write_loads(), 60, now) {
        cache.update_stat_at(p, now);
    }
}
#[tokio::test]
async fn state_task_23_hot_worker_flow_matches_original_all_peer_counts() {
    for n in [0, 1, 2, 3, 5, 9] {
        let mut worker = HotCache::new(&tokio::runtime::Handle::current(), 16);
        let mut sync = HotPeerCache::new(RWType::WRITE);
        for (time, r) in [(0, region(n + 1)), (60_000_000_000, region(n))] {
            original(&mut sync, &r, time);
            let (expired, write) = new_region_flow_tasks(&r);
            for task in [expired, write] {
                assert!(matches!(
                    worker
                        .submit(RWType::WRITE, task, time)
                        .unwrap()
                        .wait()
                        .await
                        .unwrap(),
                    HotTaskOutput::Applied
                ));
            }
        }
        let [read, write] = worker.shutdown().await.unwrap().unwrap();
        assert_eq!(snapshot(&write), snapshot(&sync), "{n} peers");
        assert!(write.get(1, n + 1).is_none());
        assert!(read.peers.is_empty());
        assert!(worker.shutdown().await.unwrap().is_none());
    }
}
#[tokio::test]
async fn state_task_23_hot_worker_read_queries_and_cold() {
    let r = region(3);
    let mut worker = HotCache::new(&tokio::runtime::Handle::current(), 16);
    let mut sync = HotPeerCache::new(RWType::READ);
    let mut loads = [0.; 8];
    loads[0] = 100000000.;
    loads[1] = 1000.;
    loads[2] = 1000.;
    for p in sync.check_peer_flow_dynamic(&r, Some(&[1]), &loads, 60, 0) {
        sync.update_stat_at(p, 0);
    }
    worker
        .submit(RWType::READ, HotTask::read_peer(&r, 1, loads, 60), 0)
        .unwrap()
        .wait()
        .await
        .unwrap();
    for _ in 0..100 {
        assert!(worker.is_region_hot(&r, 1).await.unwrap());
    }
    let mut peer = worker
        .get_hot_peer_stat(RWType::READ, 1, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        format!("{peer:?}"),
        format!("{:?}", sync.get(1, 1).unwrap().clone())
    );
    peer.loads[0] = 0.;
    assert_ne!(
        worker
            .get_hot_peer_stat(RWType::READ, 1, 1)
            .await
            .unwrap()
            .unwrap()
            .loads[0],
        0.
    );
    assert_eq!(
        worker.get_hot_peer_stats(RWType::READ, 1).await.unwrap()[&1].len(),
        1
    );
    assert!(
        worker
            .get_hot_peer_stat(RWType::WRITE, 1, 1)
            .await
            .unwrap()
            .is_none()
    );
    for p in sync.check_cold_peer(1, &Default::default(), 10, 1) {
        sync.update_stat_at(p, 1);
    }
    worker
        .submit(
            RWType::READ,
            HotTask::Cold {
                store: 1,
                reported: Default::default(),
                interval: 10,
            },
            1,
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    let [read, _] = worker.shutdown().await.unwrap().unwrap();
    assert_eq!(snapshot(&read), snapshot(&sync));
}
#[tokio::test]
async fn state_task_23_hot_worker_admission_cancel_shutdown_join() {
    // current-thread runtime does not poll workers before we yield: deterministic queue boundary.
    let mut worker = HotCache::new(&tokio::runtime::Handle::current(), 1);
    let (_, write) = new_region_flow_tasks(&region(3));
    let pending = worker.submit(RWType::WRITE, write.clone(), 0).unwrap();
    assert!(matches!(
        worker.submit(RWType::WRITE, write.clone(), 0),
        Err(HotTaskError::Full)
    ));
    assert!(pending.cancel());
    assert!(!pending.cancel());
    assert!(matches!(pending.wait().await, Err(HotTaskError::Canceled)));
    let queued = worker.submit(RWType::WRITE, write.clone(), 0).unwrap();
    worker.cancel();
    assert!(matches!(
        worker.submit(RWType::WRITE, write, 0),
        Err(HotTaskError::Closed)
    ));
    let caches = worker.shutdown().await.unwrap().unwrap();
    assert!(matches!(queued.wait().await, Err(HotTaskError::Canceled)));
    assert!(caches.iter().all(|c| c.peers.is_empty()));
}
#[test]
fn state_task_23_hot_worker_captured_fields() {
    let mut r = region(3);
    r.set_report_interval(u64::MAX - 10, 49);
    r.set_query_stats(Some(QueryStats {
        put: 123,
        ..Default::default()
    }));
    let (_, task) = new_region_flow_tasks(&r);
    r.stats_mut().written_bytes = 0;
    match task {
        HotTask::Write {
            meta,
            leader_store,
            interval,
            counters,
        } => {
            assert_eq!(meta, r.meta().clone());
            assert_eq!(leader_store, 1);
            assert_eq!(interval, 60);
            assert_eq!(counters, [1 << 20, 1024, 123]);
        }
        _ => panic!("write task"),
    }
}

#[tokio::test]
async fn state_task_23_hot_worker_fifo_partial_admission_and_snapshot_filters() {
    let mut worker = HotCache::new(&tokio::runtime::Handle::current(), 1);
    let r = region(3);
    let [write_expire, read_expire, flow] = worker.check_region_flow_async(&r, 0);
    assert!(matches!(flow, Err(HotTaskError::Full)));
    write_expire.unwrap().wait().await.unwrap();
    read_expire.unwrap().wait().await.unwrap();
    assert!(
        worker
            .get_hot_peer_stat(RWType::WRITE, 1, 1)
            .await
            .unwrap()
            .is_none()
    );
    let (_, write) = new_region_flow_tasks(&r);
    worker
        .submit(RWType::WRITE, write.clone(), 0)
        .unwrap()
        .wait()
        .await
        .unwrap();
    for _ in 0..100 {
        assert!(worker.is_region_hot(&r, 1).await.unwrap());
    }
    assert!(!worker.is_region_hot(&r, 2).await.unwrap());
    assert!(!worker.is_region_hot(&region(0), 0).await.unwrap());
    assert!(
        worker
            .get_hot_peer_stats(RWType::WRITE, 2)
            .await
            .unwrap()
            .values()
            .all(Vec::is_empty)
    );
    let bad = worker.submit(RWType::READ, write, 0).unwrap();
    assert!(matches!(
        bad.wait().await,
        Err(HotTaskError::WrongDirection)
    ));
    let caches = worker.shutdown().await.unwrap().unwrap();
    assert!(caches[0].peers.is_empty());
    assert_eq!(caches[1].peers.len(), 3);
}

#[tokio::test]
async fn state_task_23_hot_worker_queued_read_cold_expiry_fifo_and_owned_fields() {
    let mut worker = HotCache::new(&tokio::runtime::Handle::current(), 64);
    let mut sync = HotPeerCache::new(RWType::READ);
    let mut receipts = vec![];
    for step in 0..16 {
        let mut r = region(if step < 8 { 3 } else { 2 });
        let mut loads = [0.; 8];
        loads[0] = 1e7 + step as f64;
        loads[1] = 20000. + step as f64;
        loads[2] = 3000. + step as f64;
        loads[6] = 20. + step as f64;
        let now = step * 1_000_000_000;
        let (expired, _) = new_region_flow_tasks(&r);
        for p in sync.collect_expired_items(&r) {
            sync.update_stat_at(p, now);
        }
        receipts.push(worker.submit(RWType::READ, expired, now).unwrap());
        for p in sync.check_peer_flow_dynamic(&r, Some(&[1]), &loads, 4, now) {
            sync.update_stat_at(p, now);
        }
        receipts.push(
            worker
                .submit(RWType::READ, HotTask::read_peer(&r, 1, loads, 4), now)
                .unwrap(),
        );
        let reported = if step % 2 == 0 {
            [1].into_iter().collect()
        } else {
            Default::default()
        };
        for p in sync.check_cold_peer(1, &reported, 6, now) {
            sync.update_stat_at(p, now);
        }
        receipts.push(
            worker
                .submit(
                    RWType::READ,
                    HotTask::Cold {
                        store: 1,
                        reported,
                        interval: 6,
                    },
                    now,
                )
                .unwrap(),
        );
        // Caller mutation after admission must not rewrite queued payloads.
        r.stats_mut().read_bytes = 0;
        loads.fill(0.);
    }
    // No yielding before this point: all 48 reports are queued, in FIFO order.
    for receipt in receipts {
        assert!(matches!(
            receipt.wait().await.unwrap(),
            HotTaskOutput::Applied
        ));
    }
    let [mut read, _] = worker.shutdown().await.unwrap().unwrap();
    assert_eq!(snapshot(&read), snapshot(&sync));
    // Compare TopN and TTL side effects as well as every peer/window field.
    assert_eq!(
        read.calc_hot_thresholds(1, 30_000_000_000),
        sync.calc_hot_thresholds(1, 30_000_000_000)
    );
    read.gc(200_000_000_000, &[1, 2].into_iter().collect());
    sync.gc(200_000_000_000, &[1, 2].into_iter().collect());
    assert_eq!(snapshot(&read), snapshot(&sync));
    assert!(read.peers.is_empty());
}

#[test]
fn state_task_23_hot_worker_runtime_loss_and_drop_are_not_applied() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let worker = HotCache::new(runtime.handle(), 1);
    let (_, task) = new_region_flow_tasks(&region(3));
    let receipt = worker.submit(RWType::WRITE, task, 0).unwrap();
    drop(runtime); // host cancellation destroys the worker before it executes
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async move {
        assert!(matches!(
            receipt.wait().await,
            Err(HotTaskError::WorkerStopped)
        ));
        let mut worker = worker;
        assert!(matches!(
            worker.shutdown().await,
            Err(HotTaskError::WorkerStopped)
        ));
    });
    runtime.block_on(async {
        let worker = HotCache::new(&tokio::runtime::Handle::current(), 1);
        let (_, task) = new_region_flow_tasks(&region(3));
        let receipt = worker.submit(RWType::WRITE, task, 0).unwrap();
        drop(worker);
        assert!(matches!(receipt.wait().await, Err(HotTaskError::Canceled)));
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_task_23_hot_worker_shared_host_cancel_race() {
    let mut worker = HotCache::new(&tokio::runtime::Handle::current(), 256);
    let mut receipts = vec![];
    for _ in 0..128 {
        let (_, task) = new_region_flow_tasks(&region(1));
        let receipt = worker.submit(RWType::WRITE, task, 0).unwrap();
        let canceled = receipt.cancel();
        receipts.push((receipt, canceled));
    }
    let mut applied = 0;
    for (receipt, canceled) in receipts {
        match receipt.wait().await {
            Ok(HotTaskOutput::Applied) => {
                assert!(!canceled);
                applied += 1;
            }
            Err(HotTaskError::Canceled) => assert!(canceled),
            other => panic!("unexpected result: {other:?}"),
        }
    }
    let [read, write] = worker.shutdown().await.unwrap().unwrap();
    assert!(read.peers.is_empty());
    if applied == 0 {
        assert!(write.peers.is_empty());
    } else {
        assert_eq!(write.get(1, 1).unwrap().hot_degree, applied);
    }
    // The same host handle remains usable after join.
    assert_eq!(tokio::spawn(async { 42 }).await.unwrap(), 42);
}
