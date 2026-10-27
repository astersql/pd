// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::FactoryContext;
use astersql_pd_statistics::HotPeerStat;
use astersql_pd_statistics_utils::RWType;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_116.json"
    ))
    .unwrap()
}
fn setup() -> Coordinator<ProviderSnapshot<()>> {
    setup_at(42)
}
fn setup_at(end: u64) -> Coordinator<ProviderSnapshot<()>> {
    setup_peer(end, 1, 1)
}
fn setup_peer(end: u64, peer_id: u64, peer_store: u64) -> Coordinator<ProviderSnapshot<()>> {
    let mut cluster = BasicCluster::new();
    for id in 1..=2 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            address: format!("store-{id}"),
            last_heartbeat: 7_000_000_123,
            ..Default::default()
        })));
    }
    let peer = Peer {
        id: peer_id,
        store_id: peer_store,
        role: PeerRole::Learner,
        ..Default::default()
    };
    let mut region = RegionInfo::new(
        RegionMeta {
            id: 10,
            peers: vec![peer],
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    region.set_report_interval(1, end);
    cluster.regions.set_region(Arc::new(region));
    let (heartbeat, _) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    Coordinator::new(CoordinatorInputs {
        cluster: Arc::new(cluster),
        config: Arc::new(ProviderSnapshot::default()),
        heartbeat,
        pending: Arc::new(TTLUint64::new(
            tokio::runtime::Handle::current(),
            CancellationToken::new(),
            Duration::from_secs(60),
            TtlDuration::from_nanos(100),
        )),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(116),
            astersql_pd_schedule_schedulers::JobTime(0),
        ),
        scheduler_configuration: serde_json::json!({}),
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    })
}
fn peers(bytes: f64) -> HashMap<u64, Vec<HotPeerStat>> {
    HashMap::from([(
        1,
        vec![HotPeerStat {
            store_id: 1,
            region_id: 10,
            hot_degree: 2,
            is_leader: true,
            loads: vec![bytes, 2., 3., 4.],
            ..Default::default()
        }],
    )])
}
#[tokio::test]
async fn task_116_behavior_cases() {
    let c = setup();
    let loads = HashMap::from([(1, [0.; 12]), (2, [0.; 12])]);
    let read = peers(21.);
    let write = peers(11.);
    let input = HotObservationInputs {
        loads: &loads,
        read: &read,
        write: &write,
    };
    for case in fixture()["query_cases"].as_array().unwrap() {
        let rw = if case["rw"] == "write" {
            RWType::WRITE
        } else {
            RWType::READ
        };
        let all = c.get_hot_regions_by_type(rw, &input);
        let entry = &all.as_peer[&1].stats.as_ref().unwrap()[0];
        assert_eq!(entry.byte_rate, case["bytes"].as_f64().unwrap());
        assert!(entry.is_learner);
        assert_eq!(entry.last_update_time, case["time"].as_str().unwrap());
        assert_eq!(
            all.as_leader[&1].stats.as_ref().unwrap()[0].last_update_time,
            entry.last_update_time
        );
        assert_eq!(
            serde_json::to_value(c.get_hot_regions(rw, &[], &input)).unwrap(),
            serde_json::to_value(&all).unwrap()
        );
        let subset = c.get_hot_regions(rw, &[1, 99, 1], &input);
        assert_eq!(subset.as_peer.len(), 2);
        assert_eq!(subset.as_peer[&1].as_ref().unwrap(), &all.as_peer[&1]);
        assert_eq!(subset.as_peer[&99], None);
        assert_eq!(subset.as_leader[&99], None);
        assert!(serde_json::to_value(&subset).unwrap()["as_peer"]["99"].is_null());
    }
}

fn samples(registry: &prometheus::Registry, family: &str) -> Vec<(Vec<(String, String)>, f64)> {
    registry
        .gather()
        .into_iter()
        .filter(|f| f.name() == family)
        .flat_map(|f| {
            f.get_metric()
                .iter()
                .map(|m| {
                    (
                        m.get_label()
                            .iter()
                            .map(|p| (p.name().to_owned(), p.value().to_owned()))
                            .collect(),
                        m.get_gauge().get_value(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
#[tokio::test]
async fn task_116_metrics_cleanup_and_reset() {
    use astersql_pd_schedule_schedulers::SchedulerMetrics;
    let c = setup();
    let registry = prometheus::Registry::new();
    let scheduler = SchedulerMetrics::register(&registry).unwrap();
    let metrics = HotSpotMetrics::register(&registry, scheduler.clone()).unwrap();
    assert!(HotSpotMetrics::register(&registry, scheduler.clone()).is_err());
    let loads = HashMap::new();
    let read = peers(21.);
    let write = peers(11.);
    let input = HotObservationInputs {
        loads: &loads,
        read: &read,
        write: &write,
    };
    c.collect_hot_spot_metrics(&metrics, &input);
    let values = samples(&registry, "pd_hotspot_status");
    assert_eq!(values.len(), 20);
    for (labels, value) in values {
        let typ = labels.iter().find(|p| p.0 == "type").unwrap().1.as_str();
        let expected = if typ.starts_with("hot_") {
            1.
        } else if typ.contains("bytes") {
            if typ.contains("write") { 11. } else { 21. }
        } else if typ.contains("keys") {
            2.
        } else if typ.contains("query") {
            3.
        } else {
            4.
        };
        assert_eq!(value, expected, "{typ}");
    }
    scheduler
        .record_metrics("pd_scheduler_hot_pending_sum", &["1", "read", "byte"], 5.)
        .unwrap();
    scheduler
        .record_metrics("pd_scheduler_hot_pending_sum", &["2", "write", "cpu"], 6.)
        .unwrap();
    scheduler
        .record_metrics("pd_scheduler_status", &["balance", "allow"], 1.)
        .unwrap();
    // Store2 has no population: clear its pending series, retaining store1.
    c.collect_hot_spot_metrics(&metrics, &input);
    assert_eq!(samples(&registry, "pd_scheduler_hot_pending_sum").len(), 1);
    // Missing READ removes both READ/WRITE pending; an empty present WRITE
    // population still publishes zero-valued metrics, as in Go.
    let empty = HashMap::new();
    let present_empty = HashMap::from([(1, vec![])]);
    c.collect_hot_spot_metrics(
        &metrics,
        &HotObservationInputs {
            loads: &loads,
            read: &empty,
            write: &present_empty,
        },
    );
    assert!(samples(&registry, "pd_scheduler_hot_pending_sum").is_empty());
    assert_eq!(samples(&registry, "pd_hotspot_status").len(), 10);
    assert!(
        samples(&registry, "pd_hotspot_status")
            .iter()
            .all(|(_, v)| *v == 0.)
    );
    c.collect_hot_spot_metrics(&metrics, &input);
    scheduler
        .record_metrics("pd_scheduler_hot_pending_sum", &["1", "write", "cpu"], 7.)
        .unwrap();
    metrics.clone().reset_hot_spot_metrics();
    assert!(samples(&registry, "pd_scheduler_hot_pending_sum").is_empty());
    assert!(samples(&registry, "pd_hotspot_status").is_empty());
    assert_eq!(samples(&registry, "pd_scheduler_status").len(), 1);
    // Cached handles must be recreated after reset and explicit store deletion.
    c.collect_hot_spot_metrics(&metrics, &input);
    metrics.delete_store_metrics("1");
    assert!(samples(&registry, "pd_hotspot_status").is_empty());
    c.collect_hot_spot_metrics(&metrics, &input);
    assert_eq!(samples(&registry, "pd_hotspot_status").len(), 20);
    let snapshot: Vec<_> = c
        .get_cluster()
        .stores
        .get_stores()
        .iter()
        .map(|s| s.as_ref().clone())
        .collect();
    c.get_cluster().stores.patch_store(1, |s| {
        s.edit_meta(|m| m.node_state = astersql_pd_core::NodeState::Removed)
    });
    // An old live snapshot cannot resurrect a tombstoned/deleted store.
    metrics.collect_hot_spot_metrics(c.get_cluster(), &snapshot, &input);
    assert!(samples(&registry, "pd_hotspot_status").is_empty());
    c.collect_hot_spot_metrics(&metrics, &input);
    assert!(samples(&registry, "pd_hotspot_status").is_empty());
    c.get_cluster().stores.delete_store(1);
    metrics.collect_hot_spot_metrics(c.get_cluster(), &snapshot, &input);
    assert!(samples(&registry, "pd_hotspot_status").is_empty());
}
#[tokio::test]
async fn task_116_pending_weights_use_real_operator_status() {
    use astersql_pd_schedule_operator::*;
    use astersql_pd_schedule_schedulers::*;
    use astersql_pd_statistics::{Influence, summary_store_infos};
    struct Now(std::sync::Mutex<std::time::Instant>);
    impl Clock for Now {
        fn now(&self) -> std::time::Instant {
            *self.0.lock().unwrap()
        }
    }
    let c = setup();
    for case in fixture()["pending_cases"].as_array().unwrap() {
        let clock = Arc::new(Now(std::sync::Mutex::new(std::time::Instant::now())));
        let op = Arc::new(Operator::with_clock(
            "hot",
            "",
            10,
            None,
            OpKind::HOT_REGION | OpKind::LEADER,
            1,
            vec![OpStep::TransferLeader(TransferLeader {
                from_store: 1,
                to_store: 2,
                to_stores: vec![],
            })],
            clock.clone(),
        ));
        assert!(op.start());
        if case["cancel"] == true {
            assert!(op.cancel());
        }
        if case["success"] == true {
            op.check(&RegionInfo::new(
                Default::default(),
                Some(Peer {
                    store_id: 2,
                    ..Default::default()
                }),
                vec![],
                vec![],
            ));
            assert_eq!(op.status(), SUCCESS);
        }
        *clock.0.lock().unwrap() += Duration::from_secs(case["elapsed"].as_u64().unwrap_or(0));
        let mut projection = HotLoadProjection::default();
        projection
            .try_add_pending_influence(
                op,
                vec![1],
                2,
                Influence {
                    loads: vec![10.; 8],
                    hot_peer_count: 1.,
                },
                Duration::from_secs(30),
            )
            .unwrap();
        let stores: Vec<_> = c
            .get_cluster()
            .stores
            .get_stores()
            .iter()
            .map(|s| s.as_ref().clone())
            .collect();
        let mut summary = summary_store_infos(&stores);
        let config = HotRegionParams {
            pending_weight: case["weight"].as_f64().unwrap(),
            ..Default::default()
        };
        projection.summary_pending_influence(&mut summary, Some(&config), clock.as_ref());
        let registry = prometheus::Registry::new();
        let scheduler = SchedulerMetrics::register(&registry).unwrap();
        scheduler
            .collect_hot_pending_metrics(c.get_cluster().as_ref(), &summary)
            .unwrap();
        let values = samples(&registry, "pd_scheduler_hot_pending_sum");
        if case["source"] == 0 {
            assert!(values.is_empty());
        } else {
            assert_eq!(values.len(), 16);
            for (labels, value) in values {
                let source = labels.iter().any(|p| p.0 == "store" && p.1 == "1");
                assert_eq!(
                    value,
                    case[if source { "source" } else { "target" }]
                        .as_f64()
                        .unwrap()
                );
            }
        }
        // Observed hotspot totals are raw statistics, never pending-weighted.
        let loads = HashMap::from([(1, [0.; 12])]);
        let peers = peers(11.);
        let result = c.get_hot_regions_by_type(
            RWType::WRITE,
            &HotObservationInputs {
                loads: &loads,
                read: &peers,
                write: &peers,
            },
        );
        assert_eq!(result.as_peer[&1].total_bytes_rate, 11.);
    }
}
#[tokio::test]
async fn task_116_missing_region_and_negative_read_time() {
    let c = setup();
    let loads = HashMap::from([(1, [0.; 12])]);
    let mut peers = peers(11.);
    peers.get_mut(&1).unwrap()[0].region_id = 999;
    c.get_cluster()
        .stores
        .patch_store(1, |s| s.set_last_heartbeat_unix_nanos(-1));
    let input = HotObservationInputs {
        loads: &loads,
        read: &peers,
        write: &peers,
    };
    let write = c.get_hot_regions_by_type(RWType::WRITE, &input);
    let p = &write.as_peer[&1].stats.as_ref().unwrap()[0];
    assert!(!p.is_learner);
    assert_eq!(p.last_update_time, "0001-01-01T00:00:00Z");
    let read = c.get_hot_regions_by_type(RWType::READ, &input);
    assert_eq!(
        read.as_peer[&1].stats.as_ref().unwrap()[0].last_update_time,
        "1969-12-31T23:59:59.999999999Z"
    );
}

#[tokio::test]
async fn task_116_timestamp_cases() {
    for case in fixture()["time_cases"].as_array().unwrap() {
        let c = setup_at(case["seconds"].as_i64().unwrap() as u64);
        c.get_cluster().stores.patch_store(1, |s| {
            s.set_last_heartbeat_unix_nanos(case["read_nanos"].as_i64().unwrap())
        });
        let loads = HashMap::from([(1, [0.; 12])]);
        let peers = peers(11.);
        let input = HotObservationInputs {
            loads: &loads,
            read: &peers,
            write: &peers,
        };
        for (rw, key) in [(RWType::READ, "read"), (RWType::WRITE, "write")] {
            let result = c.get_hot_regions_by_type(rw, &input);
            assert_eq!(
                result.as_peer[&1].stats.as_ref().unwrap()[0].last_update_time,
                case[key].as_str().unwrap()
            );
        }
    }
}

#[tokio::test]
async fn task_116_learner_lookup_uses_go_peer_id() {
    for case in fixture()["learner_cases"].as_array().unwrap() {
        let c = setup_peer(
            42,
            case["peer_id"].as_u64().unwrap(),
            case["peer_store"].as_u64().unwrap(),
        );
        let loads = HashMap::from([(1, [0.; 12])]);
        let peers = peers(11.);
        let input = HotObservationInputs {
            loads: &loads,
            read: &peers,
            write: &peers,
        };
        for rw in [RWType::READ, RWType::WRITE] {
            let result = c.get_hot_regions_by_type(rw, &input);
            for population in [result.as_peer, result.as_leader] {
                assert_eq!(
                    population[&1].stats.as_ref().unwrap()[0].is_learner,
                    case["expected"].as_bool().unwrap()
                );
            }
        }
    }
}
