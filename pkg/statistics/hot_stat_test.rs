// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreStats, TimeInterval};
use astersql_pd_statistics_buckets::{BucketFlows, BucketReport, BucketSubmitError};
use astersql_pd_statistics_utils::{RWType, StoreLoadKind};
use prometheus::Registry;
use tokio::{
    runtime::Handle,
    time::{Duration, timeout},
};

fn region() -> RegionInfo {
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    RegionInfo::new(
        RegionMeta {
            id: 7,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    )
}
fn report() -> BucketReport {
    BucketReport {
        region_id: 7,
        version: 1,
        keys: vec![b"a".to_vec(), b"b".to_vec()],
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: vec![100000],
            read_keys: vec![10000],
            read_qps: vec![10000],
            write_bytes: vec![0],
            write_keys: vec![0],
            write_qps: vec![0],
        },
    }
}
#[tokio::test]
async fn state_task_29_hot_stat_reports_and_isolated_snapshot() {
    let mut stat = HotStat::new(&Handle::current(), &Registry::new(), 32).unwrap();
    stat.stores_stats.set(
        1,
        &StoreStats {
            interval: Some(TimeInterval {
                start_timestamp: 0,
                end_timestamp: 10,
            }),
            bytes_read: 100,
            ..Default::default()
        },
    );
    stat.slow_stat.observe_slow_store_status(1, true);
    stat.bucket_client().check_async(report()).unwrap();
    for t in 0..6 {
        let mut loads = [0.; 8];
        loads[0] = 1024. * 1024. * 60.;
        loads[1] = 1024. * 60.;
        stat.hot_cache
            .submit(
                RWType::READ,
                HotTask::read_peer(&region(), 1, loads, 60),
                t * 60_000_000_000,
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
        stat.hot_cache
            .submit(
                RWType::WRITE,
                HotTask::Write {
                    meta: region().meta().clone(),
                    leader_store: 1,
                    interval: 60,
                    counters: [1024 * 1024 * 60, 1024 * 60, 0],
                },
                t * 60_000_000_000,
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
    }
    let mut first = stat.snapshot(-20, vec![]).await.unwrap();
    assert_eq!(
        first.stores[&1][StoreLoadKind::READ_BYTES.as_i32() as usize],
        10.
    );
    assert!(first.exists_slow_stores);
    assert_eq!(first.buckets[&7][0].loads[0], 100000);
    assert!(first.is_region_hot(&region(), 1));
    assert_eq!(
        first.get_hot_peer_stat(RWType::READ, 7, 1).unwrap().loads[0],
        1048576.
    );
    assert_eq!(
        first.get_hot_peer_stat(RWType::WRITE, 7, 1).unwrap().loads[0],
        1048576.
    );
    assert_eq!(first.get_hot_peer_stats(RWType::READ, 1)[&1].len(), 1);
    first.stores.clear();
    first.read.clear();
    first.buckets.get_mut(&7).unwrap()[0].start_key.clear();
    stat.slow_stat.observe_slow_store_status(1, true);
    let second = stat.snapshot(-20, vec![]).await.unwrap();
    assert_eq!(second.stores.len(), 1);
    assert_eq!(second.read[&1].len(), 1);
    assert_eq!(second.buckets[&7][0].start_key, b"a");
    assert!(!second.exists_slow_stores);
    stat.stores_stats.remove_rolling_store_stats(1);
    assert_eq!(second.stores.len(), 1);
    timeout(Duration::from_secs(5), stat.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.read[&1].len(), 1);
}
#[tokio::test]
async fn state_task_29_hot_stat_cancel_joins_all_children() {
    let mut stat = HotStat::new(&Handle::current(), &Registry::new(), 16).unwrap();
    let client = stat.bucket_client();
    let pending = stat
        .hot_cache
        .submit(RWType::READ, HotTask::Stats { min_degree: 0 }, 0)
        .unwrap();
    client.check_async(report()).unwrap();
    stat.cancel();
    assert!(matches!(pending.wait().await, Err(HotTaskError::Canceled)));
    timeout(Duration::from_secs(5), stat.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(stat.hot_cache.shutdown().await.unwrap().is_none());
    stat.shutdown().await.unwrap();
    assert_eq!(client.check_async(report()), Err(BucketSubmitError::Closed));
    assert!(matches!(stat.snapshot(0, vec![]).await, Err(_)));
}

#[tokio::test]
async fn state_task_29_hot_stat_interrupted_shutdown_retains_joins() {
    use std::future::Future;
    let mut stat = HotStat::new(&Handle::current(), &Registry::new(), 8).unwrap();
    {
        let shutdown = stat.shutdown();
        tokio::pin!(shutdown);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(shutdown.as_mut().poll(cx)))
                .await
                .is_pending()
        );
    }
    // No runtime turn has let these children exit yet. An interrupted shutdown
    // must retain both hot join handles, not detach and report completion.
    assert!(
        stat.hot_cache.shutdown().await.unwrap().is_some(),
        "interrupted shutdown lost hot worker joins"
    );
    stat.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_29_hot_stat_empty_and_registration_conflict() {
    let registry = Registry::new();
    let mut stat = HotStat::new(&Handle::current(), &registry, 16).unwrap();
    assert!(HotStat::new(&Handle::current(), &registry, 16).is_err());
    let snapshot = stat.snapshot(0, vec![]).await.unwrap();
    assert!(snapshot.stores.is_empty());
    assert!(snapshot.read.is_empty());
    assert!(snapshot.write.is_empty());
    assert!(snapshot.buckets.is_empty());
    assert!(!snapshot.exists_slow_stores);
    assert!(
        snapshot
            .get_hot_peer_stat(RWType::from_i32(99), 7, 1)
            .is_none()
    );
    assert!(!snapshot.is_region_hot(&region(), 0));
    stat.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_29_hot_stat_full_and_abandoned_snapshot() {
    use std::future::Future;
    let mut stat = HotStat::new(&Handle::current(), &Registry::new(), 1).unwrap();
    let pending = stat
        .hot_cache
        .submit(RWType::READ, HotTask::Stats { min_degree: 0 }, 0)
        .unwrap();
    assert!(matches!(
        stat.snapshot(0, vec![]).await,
        Err(StatisticsSnapshotError::Hot(HotTaskError::Full))
    ));
    pending.wait().await.unwrap();
    {
        let snapshot = stat.snapshot(0, vec![]);
        tokio::pin!(snapshot);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(snapshot.as_mut().poll(cx)))
                .await
                .is_pending()
        );
    }
    tokio::task::yield_now().await;
    assert!(
        timeout(Duration::from_secs(5), stat.snapshot(0, vec![]))
            .await
            .unwrap()
            .is_ok()
    );
    stat.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_29_hot_stat_pending_snapshot_cancel_and_bucket_selection() {
    use std::future::Future;
    let mut stat = HotStat::new(&Handle::current(), &Registry::new(), 16).unwrap();
    stat.bucket_client().check_async(report()).unwrap();
    assert!(
        stat.snapshot(-20, vec![999])
            .await
            .unwrap()
            .buckets
            .is_empty()
    );
    assert!(
        stat.snapshot(100, vec![7])
            .await
            .unwrap()
            .buckets
            .is_empty()
    );
    assert_eq!(
        stat.snapshot(-20, vec![7]).await.unwrap().buckets[&7].len(),
        1
    );
    {
        let snapshot = stat.snapshot(0, vec![]);
        tokio::pin!(snapshot);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(snapshot.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        stat.cancel();
        assert!(snapshot.await.is_err());
    }
    stat.shutdown().await.unwrap();
}

#[test]
fn state_task_29_hot_stat_unfiltered_informer_and_filter_contract() {
    let mut snapshot = StatisticsSnapshot::default();
    snapshot.read.insert(
        1,
        vec![HotPeerStat {
            store_id: 1,
            region_id: 7,
            loads: vec![12., 3., 4., 0.],
            hot_degree: 3,
            anti_count: 0,
            in_cold: true,
            ..Default::default()
        }],
    );
    assert_eq!(
        snapshot
            .get_hot_peer_stat(RWType::READ, 7, 1)
            .unwrap()
            .loads,
        [12., 3., 4., 0.]
    );
    assert!(snapshot.is_region_hot(&region(), 3));
    assert!(!snapshot.is_region_hot(&region(), 4));
    assert!(snapshot.get_hot_peer_stats(RWType::READ, 0)[&1].is_empty());
    let peer = &mut snapshot.read.get_mut(&1).unwrap()[0];
    peer.in_cold = false;
    peer.anti_count = RWType::READ.default_anti_count() as i64;
    assert_eq!(snapshot.get_hot_peer_stats(RWType::READ, 3)[&1].len(), 1);
    let other = RegionInfo::new(
        RegionMeta {
            id: 7,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    assert!(!snapshot.is_region_hot(&other, 0));
}
