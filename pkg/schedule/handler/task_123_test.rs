// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_schedule::{Coordinator, HotObservationInputs};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_statistics::{HotPeerStat, StoreStatInformer, StoresStats};
use astersql_pd_statistics_buckets::*;
use astersql_pd_statistics_utils::RWType;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio_util::sync::CancellationToken;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_123.json"
    ))
    .unwrap()
}
async fn setup() -> (
    Coordinator<ProviderSnapshot<()>>,
    Arc<astersql_pd_cache::TTLUint64<()>>,
) {
    setup_admin(
        3,
        Arc::new(astersql_pd_storage_domain::MemoryStore::default()),
    )
}
#[tokio::test]
async fn task_123_behavior_cases() {
    let (c, pending) = setup().await;
    let f = fixture();
    for (id, engine) in [
        (1, "tikv"),
        (2, "tiflash"),
        (3, "tiflash_compute"),
        (4, ""),
        (5, "TiFlash"),
    ] {
        c.get_cluster()
            .stores
            .put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                labels: vec![StoreLabel {
                    key: "engine".into(),
                    value: engine.into(),
                }],
                ..Default::default()
            })));
    }
    let h = Handler::new(Some(&c));
    let loads = HashMap::from([
        (1, [1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.]),
        (2, [1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.]),
        (3, [1.; 12]),
        (99, [1.; 12]),
        (5, [1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.]),
    ]);
    let snapshot = astersql_pd_statistics::StatisticsSnapshot {
        stores: loads.clone(),
        ..Default::default()
    };
    assert_eq!(h.get_stores().unwrap().len(), 5);
    assert!(Arc::ptr_eq(
        &h.get_store(1).unwrap(),
        &c.get_cluster().stores.get_store(1).unwrap()
    ));
    assert_eq!(
        h.get_store(99).unwrap_err(),
        HotQueryError::StoreNotFound(99)
    );
    assert_eq!(h.get_stores_loads(&snapshot).unwrap(), loads);
    assert_eq!(
        serde_json::to_value(h.get_hot_stores(&snapshot).unwrap()).unwrap(),
        f["hot_stores"]
    );
    let mut detached = h.get_stores_loads(&snapshot).unwrap();
    detached.clear();
    assert_eq!(h.get_stores_loads(&snapshot).unwrap().len(), 5);
    c.get_cluster().stores.delete_store(1);
    assert_eq!(h.get_stores().unwrap().len(), 4);
    assert_eq!(h.get_store(1).unwrap_err(), HotQueryError::StoreNotFound(1));
    assert!(
        !h.get_hot_stores(&snapshot)
            .unwrap()
            .bytes_write_stats
            .contains_key(&1)
    );
    pending.shutdown().await;
}
#[tokio::test]
async fn task_123_current_loads_and_hot_regions() {
    let (c, pending) = setup().await;
    c.get_cluster()
        .stores
        .put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 1,
            ..Default::default()
        })));
    let h = Handler::new(Some(&c));
    let stores = StoresStats::new();
    stores.set_regions_stats(&[(1, 100., 20.)]);
    assert_eq!(
        h.get_stores_loads(&stores).unwrap(),
        stores.get_stores_loads()
    );
    stores.set_regions_stats(&[(1, 200., 40.)]);
    assert_eq!(h.get_stores_loads(&stores).unwrap()[&1][9], 200.);
    let loads = stores.get_stores_loads();
    let mut read = HashMap::from([(
        1,
        vec![HotPeerStat {
            store_id: 1,
            region_id: 42,
            is_leader: true,
            hot_degree: 4,
            loads: vec![21., 2., 3., 4.],
            ..Default::default()
        }],
    )]);
    let write = HashMap::from([(
        1,
        vec![HotPeerStat {
            store_id: 1,
            region_id: 99,
            is_leader: false,
            hot_degree: 4,
            loads: vec![11., 2., 3., 4.],
            ..Default::default()
        }],
    )]);
    for (rw, region, bytes) in [(RWType::READ, 42, 21.), (RWType::WRITE, 99, 11.)] {
        let input = HotObservationInputs {
            loads: &loads,
            read: &read,
            write: &write,
        };
        let result = h.get_hot_regions(rw, &[1, 999, 1], &input).unwrap();
        let stat = result.as_peer[&1].as_ref().unwrap();
        let peer = &stat.stats.as_ref().unwrap()[0];
        assert_eq!(peer.region_id, region);
        assert_eq!(peer.byte_rate, bytes);
        assert!(result.as_peer[&999].is_none());
        assert!(result.as_leader[&999].is_none());
        assert_eq!(result.as_peer.len(), 2);
        assert_eq!(
            serde_json::to_value(h.get_hot_regions(rw, &[], &input).unwrap()).unwrap(),
            serde_json::to_value(c.get_hot_regions(rw, &[], &input)).unwrap()
        );
    }
    read.get_mut(&1).unwrap()[0].loads[0] = 31.;
    let input = HotObservationInputs {
        loads: &loads,
        read: &read,
        write: &write,
    };
    assert_eq!(
        h.get_hot_regions(RWType::READ, &[], &input)
            .unwrap()
            .as_peer[&1]
            .as_ref()
            .unwrap()
            .stats
            .as_ref()
            .unwrap()[0]
            .byte_rate,
        31.
    );
    read.clear();
    let input = HotObservationInputs {
        loads: &loads,
        read: &read,
        write: &write,
    };
    assert_eq!(
        h.get_hot_regions(RWType::READ, &[], &input)
            .unwrap()
            .as_peer[&1]
            .as_ref()
            .unwrap()
            .count,
        0
    );
    pending.shutdown().await;
}
#[tokio::test]
async fn task_123_buckets_and_faults() {
    let (c, pending) = setup().await;
    let h = Handler::new(Some(&c));
    let cancel = CancellationToken::new();
    let mut worker = BucketWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        &prometheus::Registry::new(),
        BucketRangeIndex::new(),
    )
    .unwrap();
    let client = worker.client();
    let report = BucketReport {
        region_id: 42,
        version: 1,
        keys: vec![vec![0, 171], vec![255]],
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: vec![100000],
            read_keys: vec![2000],
            read_qps: vec![7],
            write_bytes: vec![300000],
            write_keys: vec![4000],
            write_qps: vec![9],
        },
    };
    assert_eq!(
        json!(h.get_hot_buckets(&client, &[], &cancel).await.unwrap()),
        json!({})
    );
    for _ in 0..2 {
        client.check_async(report.clone()).unwrap();
    }
    assert!(
        h.get_hot_buckets(&client, &[], &cancel)
            .await
            .unwrap()
            .is_empty()
    );
    client.check_async(report.clone()).unwrap();
    assert_eq!(
        json!(
            h.get_hot_buckets(&client, &[42, 999, 42], &cancel)
                .await
                .unwrap()
        ),
        fixture()["buckets"]
    );
    assert_eq!(
        json!(h.get_hot_buckets(&client, &[], &cancel).await.unwrap()),
        fixture()["buckets"]
    );
    assert_eq!(
        json!(h.get_hot_buckets(&client, &[999], &cancel).await.unwrap()),
        json!({})
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(
        h.get_hot_buckets(&client, &[], &cancelled)
            .await
            .unwrap_err(),
        HotQueryError::BucketWait(BucketWaitError::Cancelled)
    );
    assert_eq!(
        json!(h.get_hot_buckets(&client, &[], &cancel).await.unwrap()),
        fixture()["buckets"]
    );
    worker.shutdown().await.unwrap();
    assert_eq!(
        h.get_hot_buckets(&client, &[], &cancel).await.unwrap_err(),
        HotQueryError::BucketSubmit(BucketSubmitError::Closed)
    );
    pending.shutdown().await;
}
#[tokio::test]
async fn task_123_unbootstrapped_and_empty() {
    let h = Handler::<ProviderSnapshot<()>>::new(None);
    let stats = StoresStats::new();
    let empty = HashMap::new();
    let peers = HashMap::new();
    let input = HotObservationInputs {
        loads: &empty,
        read: &peers,
        write: &peers,
    };
    let expected = HotQueryError::Handler(HandlerError::NotBootstrapped);
    assert_eq!(h.get_store(1).unwrap_err(), expected);
    assert_eq!(h.get_stores().unwrap_err(), expected);
    assert_eq!(h.get_stores_loads(&stats).unwrap_err(), expected);
    assert_eq!(h.get_hot_stores(&stats).unwrap_err(), expected);
    assert_eq!(
        h.get_hot_regions(RWType::READ, &[], &input).unwrap_err(),
        expected
    );
    let token = CancellationToken::new();
    let mut worker = BucketWorker::start(
        &tokio::runtime::Handle::current(),
        &token,
        &prometheus::Registry::new(),
        BucketRangeIndex::new(),
    )
    .unwrap();
    assert_eq!(
        h.get_hot_buckets(&worker.client(), &[], &token)
            .await
            .unwrap_err(),
        expected
    );
    worker.shutdown().await.unwrap();
    let (c, pending) = setup().await;
    let h = Handler::new(Some(&c));
    assert!(h.get_stores().unwrap().is_empty());
    assert!(h.get_stores_loads(&stats).unwrap().is_empty());
    assert_eq!(json!(h.get_hot_stores(&stats).unwrap()), json!({}));
    pending.shutdown().await;
}

use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::BasicCluster;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::CoordinatorInputs;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_domain::StorageEndpoint;
use std::time::Duration;
fn setup_admin(
    threshold: u64,
    backend: Arc<dyn astersql_pd_storage_domain::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let cluster = Arc::new(BasicCluster::new());
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.scheduler_max_waiting_operator = 10;
            config.schedule.enable_diagnostic = false;
            config.schedule.hot_region_cache_hits_threshold = threshold;
            config.schedule.switch_witness_interval = TtlDuration::from_nanos(10);
            config
        }),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(114),
            JobTime(0),
        ),
        scheduler_configuration: json!({}),
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}

#[tokio::test]
async fn task_123_bucket_snapshot_boundaries() {
    let (c, pending) = setup().await;
    let h = Handler::new(Some(&c));
    let cancel = CancellationToken::new();
    let mut index = BucketRangeIndex::new();
    index.put_item(BucketTreeItem {
        region_id: 7,
        start_key: vec![],
        end_key: vec![],
        stats: vec![
            BucketStat {
                region_id: 7,
                start_key: vec![],
                end_key: vec![1],
                hot_degree: 2,
                interval: 1000,
                loads: [1; 6],
            },
            BucketStat {
                region_id: 7,
                start_key: vec![1],
                end_key: vec![2],
                hot_degree: 3,
                interval: 1000,
                loads: [u64::MAX, 2, 999, 4, 5, 888],
            },
            BucketStat {
                region_id: 7,
                start_key: vec![2],
                end_key: vec![],
                hot_degree: 4,
                interval: 1000,
                loads: [10, 20, 30, 40, 50, 60],
            },
        ],
        ..Default::default()
    });
    let mut worker = BucketWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        &prometheus::Registry::new(),
        index,
    )
    .unwrap();
    let result = h
        .get_hot_buckets(&worker.client(), &[7], &cancel)
        .await
        .unwrap();
    assert_eq!(
        json!(result),
        json!({"7":[
            {"start_key":"01","end_key":"02","hot_degree":3,"read_bytes":u64::MAX,"read_keys":2,"write_bytes":4,"write_keys":5},
            {"start_key":"02","end_key":"","hot_degree":4,"read_bytes":10,"read_keys":20,"write_bytes":40,"write_keys":50}
        ]})
    );
    worker.shutdown().await.unwrap();
    pending.shutdown().await;
}

#[tokio::test]
async fn task_123_wide_threshold() {
    let token = CancellationToken::new();
    let mut index = BucketRangeIndex::new();
    index.put_item(BucketTreeItem {
        region_id: 7,
        start_key: vec![],
        end_key: vec![],
        stats: vec![BucketStat {
            region_id: 7,
            start_key: vec![],
            end_key: vec![],
            hot_degree: 3,
            interval: 1000,
            loads: [1; 6],
        }],
        ..Default::default()
    });
    let mut worker = BucketWorker::start(
        &tokio::runtime::Handle::current(),
        &token,
        &prometheus::Registry::new(),
        index,
    )
    .unwrap();
    for (threshold, expected) in [(3, 1), (1u64 << 32, 0), (u64::MAX, 1)] {
        let (c, pending) = setup_admin(
            threshold,
            Arc::new(astersql_pd_storage_domain::MemoryStore::default()),
        );
        let result = Handler::new(Some(&c))
            .get_hot_buckets(&worker.client(), &[], &token)
            .await
            .unwrap();
        assert_eq!(result.len(), expected, "threshold={threshold}");
        pending.shutdown().await;
    }
    worker.shutdown().await.unwrap();
}
