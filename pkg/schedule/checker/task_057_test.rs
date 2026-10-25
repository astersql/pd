// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLString, TTLUint64, TtlDuration};
use astersql_pd_core::{BasicCluster, KeyRange, RegionInfo, RegionMeta};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn cache<K: Eq + std::hash::Hash + Clone + Send + 'static, V: Clone + Send + 'static>()
-> Arc<astersql_pd_cache::TtlCache<K, V>> {
    Arc::new(astersql_pd_cache::TtlCache::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ))
}
fn key(n: u32) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}
fn cluster(count: u32, terminal: bool) -> BasicCluster {
    let mut c = BasicCluster::new();
    for id in 1..=count {
        c.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id: id as u64,
                key_range: KeyRange {
                    start_key: key(id),
                    end_key: if terminal && id == count {
                        vec![]
                    } else {
                        key(id + 1)
                    },
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    c
}
#[tokio::test(start_paused = true)]
async fn task_057_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_057.json"
    ))
    .unwrap();
    let ranges: Arc<TTLString<[Vec<u8>; 2]>> = cache();
    let pending: Arc<TTLUint64<()>> = cache();
    let q = SuspectRanges::new(ranges.clone());
    let dispatcher = CheckerDispatcher::new(pending.clone(), 0, 0);
    for case in fixture["scans"].as_array().unwrap() {
        q.clear();
        dispatcher.clear_pending_processed_regions();
        let c = cluster(
            case["count"].as_u64().unwrap() as u32,
            case["terminal"].as_bool().unwrap(),
        );
        let end = case["end"]
            .as_u64()
            .map(|v| key(v as u32))
            .unwrap_or_default();
        q.add(&key(case["start"].as_u64().unwrap() as u32), &end);
        let ids = q.check_once(&c, &dispatcher);
        assert_eq!(
            ids.len(),
            case["expected_count"].as_u64().unwrap() as usize,
            "{}",
            case["name"]
        );
        assert_eq!(dispatcher.pending_processed_regions().len(), ids.len());
        if let Some(n) = case["remainder"].as_u64() {
            assert_eq!(q.pop(), Some([key(n as u32), end]));
        } else {
            assert!(q.pop().is_none(), "{}", case["name"]);
        }
    }
    ranges.shutdown().await;
    pending.shutdown().await;
    println!(
        "task_057: 7 frozen bounded scan cases; real registry and shared dispatcher pending state"
    );
}
#[tokio::test(start_paused = true)]
async fn task_057_identity_clear_and_expiration() {
    let ranges = cache();
    let q = SuspectRanges::new(ranges.clone());
    q.add(b"a", b"c");
    q.add(b"a", b"c");
    assert_eq!(q.pop(), Some([b"a".to_vec(), b"c".to_vec()]));
    assert_eq!(q.pop(), None);
    q.add(b"a", b"c");
    q.add(b"b", b"d");
    assert!(q.pop().is_some());
    assert!(q.pop().is_some());
    assert!(q.pop().is_none());
    q.add(b"", b"");
    q.clear();
    assert!(q.pop().is_none());
    q.add(b"a", b"z");
    tokio::time::advance(Duration::from_secs(180)).await;
    assert!(q.pop().is_none());
    ranges.shutdown().await;
}
#[tokio::test(start_paused = true)]
async fn task_057_tick_and_stop() {
    let ranges = cache();
    let pending = cache();
    let q = SuspectRanges::new(ranges.clone());
    let d = CheckerDispatcher::new(pending.clone(), 0, 0);
    let c = cluster(1, true);
    q.add(&key(1), b"");
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(
        q.run(
            || async {
                q.check_once(&c, &d);
                Ok::<_, ()>(())
            },
            cancel.clone()
        ),
        async {
            tokio::time::sleep(Duration::from_millis(99)).await;
            assert!(d.pending_processed_regions().is_empty());
            tokio::time::sleep(Duration::from_millis(2)).await;
            assert_eq!(d.pending_processed_regions(), vec![1]);
            q.add(&key(1), b"");
            cancel.cancel();
        }
    );
    assert_eq!(result, Ok(()));
    assert!(q.pop().is_some());
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        q.run(
            || async {
                panic!("canceled worker must not check");
                #[allow(unreachable_code)]
                Ok::<_, ()>(())
            },
            cancel
        )
        .await,
        Ok(())
    );
    ranges.shutdown().await;
    pending.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn task_057_continuation_and_shared_patrol() {
    let ranges = cache();
    let q = SuspectRanges::new(ranges.clone());
    let pending = cache();
    let d = CheckerDispatcher::new(pending.clone(), 0, 0);
    let c = cluster(1030, false);
    q.add(&key(1), &key(1031));
    assert_eq!(q.check_once(&c, &d), (1..=1024).collect::<Vec<u64>>());
    assert_eq!(q.check_once(&c, &d), (1025..=1030).collect::<Vec<u64>>());
    assert!(q.check_once(&c, &d).is_empty());
    assert_eq!(d.pending_processed_regions().len(), 1030);
    q.add(&key(1), &key(2));
    q.check_once(&c, &d);
    q.clear();
    assert_eq!(d.pending_processed_regions().len(), 1030);
    pending.shutdown().await;
    let h = super::task_056_test::Host::new(1, 0, true).await;
    q.add(&key(1), b"");
    let stop = CancellationToken::new();
    let (result, ()) = tokio::join!(q.run(|| h.suspect_tick(&q), stop.clone()), async {
        tokio::time::sleep(Duration::from_millis(101)).await;
        assert_eq!(h.pending_ids().await, vec![1]);
        stop.cancel();
    });
    assert_eq!(result, Ok(()));
    let mut patrol = Patrol::new();
    let mut status = patrol.subscribe();
    let stop = CancellationToken::new();
    let (result, ()) = tokio::join!(patrol.run(&h, stop.clone()), async {
        while status.borrow().ticks < 1 {
            status.changed().await.unwrap();
        }
        assert!(h.pending_ids().await.is_empty());
        assert_eq!(h.learner_waiting().await, 1);
        stop.cancel();
    });
    assert_eq!(result, Ok(()));
    h.shutdown().await;
    ranges.shutdown().await;
}
#[tokio::test(start_paused = true)]
async fn task_057_host_fault_and_inflight_join() {
    let ranges = cache();
    let q = SuspectRanges::new(ranges.clone());
    let result = q
        .run(
            || async { Err::<(), _>("host I/O boundary fault") },
            CancellationToken::new(),
        )
        .await;
    assert_eq!(result, Err("host I/O boundary fault"));
    let entered = tokio::sync::Notify::new();
    let release = tokio::sync::Notify::new();
    let stop = CancellationToken::new();
    let finished = std::cell::Cell::new(false);
    let (result, ()) = tokio::join!(
        async {
            let r = q
                .run(
                    || async {
                        entered.notify_one();
                        release.notified().await;
                        Ok::<_, ()>(())
                    },
                    stop.clone(),
                )
                .await;
            finished.set(true);
            r
        },
        async {
            entered.notified().await;
            stop.cancel();
            tokio::task::yield_now().await;
            assert!(!finished.get());
            release.notify_one();
        }
    );
    assert_eq!(result, Ok(()));
    assert!(finished.get());
    ranges.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn task_057_pending_cap_bypass() {
    let ranges = cache();
    let pending = cache();
    let q = SuspectRanges::new(ranges.clone());
    let d = CheckerDispatcher::new(pending.clone(), 0, 0);
    let ids: Vec<_> = (2..=100_003).collect();
    d.add_pending_processed_regions(false, &ids);
    assert_eq!(d.pending_processed_regions().len(), 100_002);
    q.add(&key(1), b"");
    assert_eq!(q.check_once(&cluster(1, true), &d), vec![1]);
    assert!(d.pending_processed_regions().contains(&1));
    assert_eq!(d.pending_processed_regions().len(), 100_003);
    ranges.shutdown().await;
    pending.shutdown().await;
}
