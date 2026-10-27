// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
use astersql_pd_statistics::{HotCache, HotTaskError, HotTaskOutput};
use astersql_pd_statistics_utils::RWType;
use std::sync::{Arc, mpsc};

fn region(witness: bool, leader: bool) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            is_witness: id == 1 && witness,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 127,
            peers: peers.clone(),
            ..Default::default()
        },
        leader.then(|| peers[0].clone()),
        vec![],
        vec![],
    );
    r.set_report_interval(0, 60);
    r.stats_mut().written_bytes = 60_000_000;
    r.stats_mut().written_keys = 60_000;
    Arc::new(r)
}
async fn applied(results: AsyncStatsReceipt) {
    for task in results.hot {
        assert!(matches!(
            task.unwrap().wait().await.unwrap(),
            HotTaskOutput::Applied
        ));
    }
}
#[tokio::test]
async fn task_127_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_127.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let mut hot = HotCache::new(&tokio::runtime::Handle::current(), 16);
        let (tx, rx) = mpsc::sync_channel(8);
        let r = region(case["witness"] == true, case["leader"] == true);
        let out = handle_stats_async(
            &hot,
            (case["installed"] == true).then_some(&tx),
            r.clone(),
            0,
        );
        assert_eq!(
            out.witness,
            if case["queued"] == true {
                WitnessAdmission::Queued
            } else {
                WitnessAdmission::Skipped
            },
            "{case}"
        );
        applied(out).await;
        if case["queued"] == true {
            assert!(Arc::ptr_eq(&rx.try_recv().unwrap(), &r));
        }
        assert!(rx.try_recv().is_err());
        assert!(
            hot.get_hot_peer_stat(RWType::WRITE, 127, 2)
                .await
                .unwrap()
                .is_some()
        );
        hot.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn task_127_backpressure_and_stop() {
    // Current-thread runtime: no worker can drain before this synchronous call.
    let mut hot = HotCache::new(&tokio::runtime::Handle::current(), 1);
    let (tx, rx) = mpsc::sync_channel(1);
    let r = region(true, true);
    tx.send(r.clone()).unwrap();
    let out = handle_stats_async(&hot, Some(&tx), r.clone(), 0);
    assert_eq!(out.witness, WitnessAdmission::Full);
    let [write_expire, read_expire, write_flow] = out.hot;
    assert!(matches!(write_flow, Err(HotTaskError::Full)));
    write_expire.unwrap().wait().await.unwrap();
    read_expire.unwrap().wait().await.unwrap();
    assert!(Arc::ptr_eq(&rx.try_recv().unwrap(), &r));
    hot.shutdown().await.unwrap();
    let out = handle_stats_async(&hot, Some(&tx), r.clone(), 0);
    assert_eq!(out.witness, WitnessAdmission::Queued);
    assert!(
        out.hot
            .into_iter()
            .all(|x| matches!(x, Err(HotTaskError::Closed)))
    );
    assert!(Arc::ptr_eq(&rx.try_recv().unwrap(), &r));
    drop(rx);
    let out = handle_stats_async(&hot, Some(&tx), r, 0);
    assert_eq!(out.witness, WitnessAdmission::Closed);
}
#[tokio::test]
async fn task_127_duplicate_and_immutable_view() {
    let mut hot = HotCache::new(&tokio::runtime::Handle::current(), 32);
    let (tx, rx) = mpsc::sync_channel(4);
    let r = region(true, true);
    for _ in 0..2 {
        applied(handle_stats_async(&hot, Some(&tx), r.clone(), 0)).await;
    }
    let updated = region(false, true);
    applied(handle_stats_async(&hot, Some(&tx), updated, 60_000_000_000)).await;
    for _ in 0..2 {
        assert!(Arc::ptr_eq(&rx.try_recv().unwrap(), &r));
    }
    assert!(rx.try_recv().is_err());
    assert!(
        hot.get_hot_peer_stat(RWType::WRITE, 127, 1)
            .await
            .unwrap()
            .is_some()
    );
    hot.shutdown().await.unwrap();
}

#[tokio::test]
async fn task_127_canonical_scheduler_consumes_event() {
    use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::{EvictLeaderContext, TransferWitnessLeaderScheduler};
    let cluster = BasicCluster::new();
    for id in 1..=3 {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        cluster.stores.put_store(Arc::new(store));
    }
    let config = ProviderSnapshot::<()>::default();
    let mut rules = RuleManager::new(
        Arc::new(astersql_pd_storage_domain::StorageEndpoint::new(Arc::new(
            astersql_pd_storage_domain::MemoryStore::default(),
        ))),
        astersql_pd_keypath::ClusterPaths::new(127),
    );
    let mut hot = HotCache::new(&tokio::runtime::Handle::current(), 16);
    let mut scheduler = TransferWitnessLeaderScheduler::new();
    applied(handle_stats_async(
        &hot,
        Some(&scheduler.recv_region_info()),
        region(true, true),
        0,
    ))
    .await;
    let mut pick = |_| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut ctx = EvictLeaderContext {
        cluster: &cluster,
        config: &config,
        rules: &mut rules,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut pick,
        shuffle: &mut shuffle,
    };
    let out = scheduler.schedule(&mut ctx).await;
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert_eq!(out.operators.len(), 1);
    let op = &out.operators[0];
    assert_eq!(op.region_id(), 127);
    assert_eq!(op.len(), 1);
    assert_eq!(op.desc(), "transfer-witness-leader-scheduler");
    let command = op.step(0).unwrap().get_cmd(&region(true, true), true);
    assert_eq!(
        command
            .unwrap()
            .transfer_leader
            .unwrap()
            .peer
            .unwrap()
            .store_id,
        2
    );
    assert!(scheduler.schedule(&mut ctx).await.operators.is_empty());
    assert!(
        hot.get_hot_peer_stat(RWType::WRITE, 127, 2)
            .await
            .unwrap()
            .is_some()
    );
    hot.shutdown().await.unwrap();
}

#[tokio::test]
async fn task_127_witness_faults_do_not_suppress_hot_flow() {
    let mut hot = HotCache::new(&tokio::runtime::Handle::current(), 16);
    let (tx, rx) = mpsc::sync_channel(1);
    let r = region(true, true);
    tx.send(r.clone()).unwrap();
    let out = handle_stats_async(&hot, Some(&tx), r.clone(), 0);
    assert_eq!(out.witness, WitnessAdmission::Full);
    applied(out).await;
    assert!(
        hot.get_hot_peer_stat(RWType::WRITE, 127, 2)
            .await
            .unwrap()
            .is_some()
    );
    drop(rx);
    let out = handle_stats_async(&hot, Some(&tx), r, 60_000_000_000);
    assert_eq!(out.witness, WitnessAdmission::Closed);
    applied(out).await;
    assert!(
        hot.get_hot_peer_stat(RWType::WRITE, 127, 2)
            .await
            .unwrap()
            .is_some()
    );
    hot.shutdown().await.unwrap();
}
