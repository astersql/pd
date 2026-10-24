// Copyright 2026 AsterSQL.
use super::*;
use prometheus::Registry;

fn values(registry: &Registry, family: &str) -> Vec<(Vec<(String, String)>, f64)> {
    registry
        .gather()
        .iter()
        .filter(|f| f.name() == family)
        .flat_map(|f| f.get_metric())
        .map(|m| {
            (
                m.get_label()
                    .iter()
                    .map(|l| (l.name().into(), l.value().into()))
                    .collect(),
                m.get_gauge().get_value(),
            )
        })
        .collect()
}
#[test]
fn state_task_28_metrics_store_reset() {
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    for address in ["old", "new"] {
        m.set_gauge(
            "pd_scheduler_store_status",
            &[address, "1", "region_count"],
            5.,
        )
        .unwrap();
    }
    for name in ["zone:z1", "host:h1"] {
        m.set_gauge("pd_cluster_placement_status", &["label", name, "1"], 1.)
            .unwrap();
    }
    for typ in ["add-peer", "remove-peer", "transfer-leader-in"] {
        m.set_gauge("pd_cluster_store_limit", &["1", typ], 30.)
            .unwrap();
    }
    m.set_gauge("pd_cluster_store_limit", &["2", "add-peer"], 40.)
        .unwrap();
    m.set_gauge(
        "pd_cluster_status",
        &["store_removed_count", "tikv", "1"],
        1.,
    )
    .unwrap();
    for engine in ["tikv", "tiflash"] {
        for typ in [
            "region_count",
            "leader_count",
            "witness_count",
            "learner_count",
            "storage_size",
            "storage_capacity",
        ] {
            m.set_gauge("pd_cluster_status", &[typ, engine, "1"], 5.)
                .unwrap();
        }
    }
    m.reset_store_statistics("1");
    m.reset_store_statistics("1");
    assert!(
        values(&registry, "pd_cluster_placement_status").is_empty(),
        "placement series survived reset"
    );
    assert!(values(&registry, "pd_scheduler_store_status").is_empty());
    assert_eq!(values(&registry, "pd_cluster_store_limit").len(), 1);
    assert_eq!(values(&registry, "pd_cluster_status").len(), 1);
}
#[test]
fn state_task_28_metrics_independent_registry_and_duplicate() {
    let a = Registry::new();
    let b = Registry::new();
    let m = StatisticsMetrics::register(&a).unwrap();
    let n = StatisticsMetrics::register(&b).unwrap();
    m.set_gauge("pd_cluster_store_limit", &["1", "transfer-leader-in"], 30.)
        .unwrap();
    assert!(values(&b, "pd_cluster_store_limit").is_empty());
    assert!(StatisticsMetrics::register(&a).is_err());
    n.set_gauge("pd_cluster_store_limit", &["1", "transfer-leader-in"], 99.)
        .unwrap();
    assert_eq!(values(&a, "pd_cluster_store_limit")[0].1, 30.);
    assert_eq!(values(&b, "pd_cluster_store_limit")[0].1, 99.);
}
#[test]
fn state_task_28_metrics_resets() {
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    m.set_gauge("pd_regions_status", &["miss-peer-region-count"], 7.)
        .unwrap();
    m.set_gauge("pd_regions_label_level", &["zone"], 3.)
        .unwrap();
    m.set_gauge("pd_hotcache_status", &["add_item", "store-1", "write"], 2.)
        .unwrap();
    m.reset_region_stats_metrics();
    m.reset_label_stats_metrics();
    m.reset_hot_cache_status_metrics();
    assert_eq!(
        values(&registry, "pd_regions_status")
            .iter()
            .map(|x| x.1)
            .sum::<f64>(),
        0.,
        "region reset must zero cached gauges"
    );
    assert!(values(&registry, "pd_regions_label_level").is_empty());
    assert!(values(&registry, "pd_hotcache_status").is_empty());
}

#[test]
fn state_task_28_metrics_registration_rollback_and_schema() {
    use prometheus::{GaugeVec, Opts, proto::MetricType};
    let registry = Registry::new();
    let conflict = GaugeVec::new(
        Opts::new(
            "pd_config_status",
            "Status of the scheduling configurations.",
        ),
        &["type"],
    )
    .unwrap();
    registry.register(Box::new(conflict.clone())).unwrap();
    assert!(StatisticsMetrics::register(&registry).is_err());
    // Even families registered before the conflicting config family must be gone.
    registry.unregister(Box::new(conflict)).unwrap();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let source = include_str!("metrics.go");
    // Read the independent Go declaration, rather than the Rust descriptor table.
    let quoted = |block: &str, key: &str| -> String {
        block
            .split(&format!("{key}:"))
            .nth(1)
            .unwrap()
            .split('"')
            .nth(1)
            .unwrap()
            .to_owned()
    };
    let mut expected = std::collections::BTreeMap::new();
    for block in source.split("prometheus.NewGaugeVec(").skip(1) {
        let block = block.split("})").next().unwrap();
        let name = format!(
            "{}_{}_{}",
            quoted(block, "Namespace"),
            quoted(block, "Subsystem"),
            quoted(block, "Name")
        );
        let labels: Vec<String> = block
            .split("[]string{")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap()
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_owned())
            .collect();
        m.set_gauge(&name, &vec!["test"; labels.len()], 9.).unwrap();
        expected.insert(name, (quoted(block, "Help"), labels));
    }
    assert_eq!(expected.len(), 9);
    let families = registry.gather();
    assert_eq!(families.len(), 10);
    for f in &families {
        if f.name() == "pd_regions_abnormal_peer_duration_seconds" {
            assert_eq!(f.get_field_type(), MetricType::HISTOGRAM);
            assert_eq!(f.get_metric().len(), 2);
            let h = f.get_metric()[0].get_histogram();
            assert_eq!(h.get_bucket().len(), 30);
            assert_eq!(h.get_bucket()[0].upper_bound(), 1.);
            assert!((h.get_bucket()[29].upper_bound() - 1.4_f64.powi(29)).abs() < 1e-8);
        } else {
            assert_eq!(f.get_field_type(), MetricType::GAUGE);
            let (help, labels) = &expected[f.name()];
            assert_eq!(f.help(), help, "{}", f.name());
            let mut names = labels.clone();
            names.sort_unstable();
            assert_eq!(
                f.get_metric()
                    .last()
                    .unwrap()
                    .get_label()
                    .iter()
                    .map(|l| l.name())
                    .collect::<Vec<_>>(),
                names
            );
        }
    }
    assert!(m.set_gauge("unknown", &[], 1.).is_err());
    assert!(m.set_gauge("pd_cluster_store_limit", &["1"], 1.).is_err());
}

#[test]
fn state_task_28_metrics_delete_cluster_status_and_global_reset() {
    use astersql_pd_core::{StoreInfo, StoreMeta};
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let store = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    for engine in ["tikv", "tiflash"] {
        for typ in ["store_up_count", "store_removed_count", "region_count"] {
            m.set_gauge("pd_cluster_status", &[typ, engine, "1"], 1.)
                .unwrap();
        }
    }
    m.delete_cluster_status_metrics(&store);
    m.delete_cluster_status_metrics(&store);
    assert_eq!(values(&registry, "pd_cluster_status").len(), 4);
    m.set_gauge("pd_config_status", &["leader-schedule-limit"], 4.)
        .unwrap();
    m.set_gauge("pd_scheduler_hot_peers_summary", &["read", "1"], 2.)
        .unwrap();
    m.observe_region_durations(&RegionAnomalyDurations {
        miss_voter_seconds: Some(2),
        down_peer_seconds: Some(3),
    });
    m.reset();
    m.reset();
    assert!(values(&registry, "pd_cluster_status").is_empty());
    assert_eq!(values(&registry, "pd_regions_status").len(), 10);
    assert_eq!(values(&registry, "pd_config_status")[0].1, 4.);
    assert_eq!(values(&registry, "pd_scheduler_hot_peers_summary")[0].1, 2.);
    let f = registry
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_regions_abnormal_peer_duration_seconds")
        .unwrap();
    assert_eq!(
        f.get_metric()
            .iter()
            .map(|m| m.get_histogram().get_sample_count())
            .sum::<u64>(),
        2
    );
    assert_eq!(
        f.get_metric()
            .iter()
            .map(|m| m.get_histogram().get_sample_sum())
            .sum::<f64>(),
        5.
    );
    m.set_gauge("pd_cluster_status", &["region_count", "tikv", "1"], 8.)
        .unwrap();
    assert_eq!(values(&registry, "pd_cluster_status")[0].1, 8.);
}

#[test]
fn state_task_28_metrics_real_store_collect_transfer_limit_and_recreate() {
    use astersql_pd_core::{NodeState, StoreInfo, StoreLabel, StoreMeta};
    use astersql_pd_schedule_config::{ProviderSnapshot, StoreLimitConfig};
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let mut conf: ProviderSnapshot<()> = ProviderSnapshot::default();
    conf.replication.location_labels = vec!["zone".into()].into();
    conf.schedule.leader_schedule_policy = "count".into();
    conf.schedule.region_score_formula_version = "v2".into();
    conf.schedule.high_space_ratio = 0.7;
    conf.schedule.low_space_ratio = 0.8;
    conf.schedule.store_limit.get_or_insert_default().insert(
        1,
        StoreLimitConfig {
            transfer_leader_in: 30.,
            ..Default::default()
        },
    );
    let store = StoreInfo::new(StoreMeta {
        id: 1,
        address: "old".into(),
        node_state: NodeState::Serving,
        labels: vec![StoreLabel {
            key: "zone".into(),
            value: "z1".into(),
        }],
        ..Default::default()
    });
    let mut stats = StoreStatisticsMap::new(&conf);
    stats.observe(&store, 0);
    let samples = stats.collect();
    m.collect_store_statistics(&samples).unwrap();
    assert!(
        values(&registry, "pd_cluster_store_limit")
            .iter()
            .any(
                |(labels, v)| labels.contains(&("type".into(), "transfer-leader-in".into()))
                    && *v == 30.
            )
    );
    assert_eq!(values(&registry, "pd_cluster_placement_status").len(), 1);
    m.reset_store_statistics("1");
    assert!(values(&registry, "pd_cluster_store_limit").is_empty());
    assert!(values(&registry, "pd_cluster_placement_status").is_empty());
    m.collect_store_statistics(&samples).unwrap();
    assert_eq!(values(&registry, "pd_cluster_store_limit").len(), 3);
    m.collect_store_statistics(&StoreStatisticsMap::new(&conf).collect())
        .unwrap();
    assert!(values(&registry, "pd_cluster_placement_status").is_empty());
}

#[test]
fn state_task_28_metrics_region_label_and_flows() {
    use astersql_pd_core::{NodeState, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
    use astersql_pd_schedule_config::ProviderSnapshot;
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let mut conf: ProviderSnapshot<()> = ProviderSnapshot::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 3;
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    let store = StoreInfo::new(StoreMeta {
        id: 1,
        address: "a".into(),
        ..Default::default()
    });
    let mut regions = RegionStatistics::new();
    let duration = regions
        .observe(&region, std::slice::from_ref(&store), &conf, 100)
        .unwrap();
    m.collect_region_statistics(&regions);
    m.observe_region_durations(&duration);
    assert!(
        values(&registry, "pd_regions_status")
            .iter()
            .any(
                |(labels, v)| labels.contains(&("type".into(), "miss-peer-region-count".into()))
                    && *v == 1.
            )
    );
    let mut labels = LabelStatistics::new();
    labels.observe(&region, &[&store], &["zone"]);
    m.collect_label_statistics(&labels);
    assert_eq!(values(&registry, "pd_regions_label_level")[0].1, 1.);
    m.reset_label_stats_metrics();
    assert!(values(&registry, "pd_regions_label_level").is_empty());
    m.collect_label_statistics(&labels);
    assert_eq!(values(&registry, "pd_regions_label_level").len(), 1);
    let stats = StoresStats::new();
    m.observe_hot_stat(&store, &stats);
    assert!(values(&registry, "pd_scheduler_store_status").is_empty());
    stats.get_or_create_rolling_store_stats(1);
    stats.set_regions_stats(&[(1, 512., 25.)]);
    m.observe_hot_stat(&store, &stats);
    assert_eq!(values(&registry, "pd_scheduler_store_status").len(), 21);
    assert!(
        values(&registry, "pd_scheduler_store_status")
            .iter()
            .any(|(labels, v)| labels
                .contains(&("type".into(), "store_regions_write_rate_bytes".into()))
                && *v == 512.)
    );
    m.reset_store_statistics("1");
    let removed = StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Removed,
        ..Default::default()
    });
    m.observe_hot_stat(&removed, &stats);
    assert!(values(&registry, "pd_scheduler_store_status").is_empty());
    m.observe_hot_stat(&store, &stats);
    assert_eq!(values(&registry, "pd_scheduler_store_status").len(), 21);
}

#[test]
fn state_task_28_metrics_hot_threshold_actions_gc_and_reset() {
    use astersql_pd_statistics_utils::{ActionType, RWType};
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let mut cache = HotPeerCache::new(RWType::WRITE);
    cache.set_peer_ttl(10);
    cache.calc_hot_thresholds(9, 0); // threshold-only store must also be cleaned.
    cache.update_stat_at(
        HotPeerStat {
            region_id: 1,
            store_id: 1,
            action_type: ActionType::ADD,
            ..Default::default()
        },
        0,
    );
    cache.calc_hot_thresholds(1, 0);
    m.collect_hot_cache_metrics(&cache.collect_metrics(0, &[1, 9].into()));
    assert_eq!(values(&registry, "pd_hotcache_status").len(), 13);
    assert!(
        values(&registry, "pd_hotcache_status")
            .iter()
            .any(
                |(l, v)| l.contains(&("name".into(), "byte-rate-threshold".into())) && *v == 1024.
            )
    );
    assert!(
        values(&registry, "pd_hotcache_status")
            .iter()
            .any(|(l, v)| l.contains(&("name".into(), "add_item".into())) && *v == 1.)
    );
    m.reset_hot_cache_status_metrics();
    assert!(values(&registry, "pd_hotcache_status").is_empty());
    m.collect_hot_cache_metrics(&cache.collect_metrics(1, &[1, 9].into()));
    assert_eq!(values(&registry, "pd_hotcache_status").len(), 13);
    // Throttled GC retains labels until the exact boundary, without new traffic.
    m.collect_hot_cache_metrics(&cache.collect_metrics(9, &[].into()));
    assert_eq!(values(&registry, "pd_hotcache_status").len(), 13);
    m.collect_hot_cache_metrics(&cache.collect_metrics(10, &[].into()));
    assert!(values(&registry, "pd_hotcache_status").is_empty());
    assert!(cache.get(1, 1).is_none());
    // Unknown action must not create a counter; expired peers do not count as a
    // report-driven remove action, matching Go's direct GC index cleanup.
    cache.update_stat_at(
        HotPeerStat {
            store_id: 2,
            action_type: ActionType::from_i32(9),
            ..Default::default()
        },
        11,
    );
    m.collect_hot_cache_metrics(&cache.collect_metrics(20, &[2].into()));
    assert!(values(&registry, "pd_hotcache_status").is_empty());
}

#[tokio::test]
async fn state_task_28_metrics_idle_worker_gc_and_shutdown() {
    use astersql_pd_core::{Peer, RegionMeta};
    use astersql_pd_statistics_utils::RWType;
    let registry = Registry::new();
    let metrics = StatisticsMetrics::register(&registry).unwrap();
    let mut cache = HotCache::new(&tokio::runtime::Handle::current(), 10);
    let task = HotTask::Read {
        meta: RegionMeta {
            id: 1,
            peers: vec![Peer {
                id: 1,
                store_id: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        leader_store: 1,
        store: 1,
        loads: [1_000_000.; 8],
        interval: 10,
    };
    cache
        .submit(RWType::READ, task, 0)
        .unwrap()
        .wait()
        .await
        .unwrap();
    cache
        .collect_metrics(&metrics, 0, &[1].into())
        .await
        .unwrap();
    assert!(!values(&registry, "pd_hotcache_status").is_empty());
    cache
        .collect_metrics(&metrics, 180_000_000_000, &[].into())
        .await
        .unwrap();
    assert!(values(&registry, "pd_hotcache_status").is_empty());
    cache.cancel();
    let final_cache = cache.shutdown().await.unwrap().unwrap();
    assert!(final_cache.iter().all(|c| c.regions_of_store(1).is_empty()));
    assert!(cache.shutdown().await.unwrap().is_none());
    assert_eq!(
        cache
            .collect_metrics(&metrics, 180_000_000_001, &[].into())
            .await,
        Err(HotTaskError::Closed)
    );
}

#[test]
fn state_task_28_metrics_explicit_gc_and_reactivated_store() {
    use astersql_pd_statistics_utils::{ActionType, RWType};
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let mut cache = HotPeerCache::new(RWType::READ);
    cache.set_peer_ttl(10);
    cache.calc_hot_thresholds(1, 0);
    m.collect_hot_cache_metrics(&cache.collect_metrics(0, &[1].into()));
    assert_eq!(values(&registry, "pd_hotcache_status").len(), 5);
    cache.gc(10, &[].into()); // host-driven GC before the next collection
    m.collect_hot_cache_metrics(&cache.collect_metrics(11, &[].into()));
    assert!(values(&registry, "pd_hotcache_status").is_empty());
    cache.calc_hot_thresholds(1, 12);
    m.collect_hot_cache_metrics(&cache.collect_metrics(12, &[1].into()));
    cache.gc(20, &[].into());
    cache.update_stat_at(
        HotPeerStat {
            region_id: 2,
            store_id: 1,
            action_type: ActionType::ADD,
            ..Default::default()
        },
        21,
    );
    m.collect_hot_cache_metrics(&cache.collect_metrics(21, &[1].into()));
    let items = values(&registry, "pd_hotcache_status");
    assert_eq!(items.len(), 3);
    assert!(
        items
            .iter()
            .any(|(l, v)| l.contains(&("name".into(), "add_item".into())) && *v == 1.)
    );
    // Live-store TTL expiry removes peers without counting a Remove report.
    m.collect_hot_cache_metrics(&cache.collect_metrics(31, &[1].into()));
    assert!(cache.get(2, 1).is_none());
    assert!(
        values(&registry, "pd_hotcache_status")
            .iter()
            .any(|(l, v)| l.contains(&("name".into(), "remove_item".into())) && *v == 0.)
    );
}

#[test]
fn state_task_28_metrics_go_reset_snapshot() {
    use astersql_pd_statistics_utils::{ActionType, RWType};
    let go: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/state/task-28.json"
    ))
    .unwrap();
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let mut cache = HotPeerCache::new(RWType::WRITE);
    for store in 0..5 {
        cache.update_stat_at(
            HotPeerStat {
                store_id: store,
                region_id: store,
                action_type: ActionType::ADD,
                ..Default::default()
            },
            0,
        );
    }
    m.collect_hot_cache_metrics(&cache.collect_metrics(0, &[0, 1, 2, 3, 4].into()));
    assert_eq!(
        values(&registry, "pd_hotcache_status").len(),
        go["hot"]["before"].as_u64().unwrap() as usize
    );
    m.set_gauge(
        "pd_regions_status",
        &["miss-peer-region-count"],
        go["region"]["input"].as_f64().unwrap(),
    )
    .unwrap();
    m.set_gauge("pd_regions_label_level", &["zone"], 1.)
        .unwrap();
    assert_eq!(
        values(&registry, "pd_regions_label_level").len(),
        go["label"]["before"].as_u64().unwrap() as usize
    );
    m.reset();
    assert_eq!(
        values(&registry, "pd_hotcache_status").len(),
        go["hot"]["after"].as_u64().unwrap() as usize
    );
    assert_eq!(
        values(&registry, "pd_regions_label_level").len(),
        go["label"]["after"].as_u64().unwrap() as usize
    );
    assert_eq!(
        values(&registry, "pd_regions_status")
            .iter()
            .map(|v| v.1)
            .sum::<f64>(),
        go["region"]["after"].as_f64().unwrap()
    );
}

#[test]
fn state_task_28_metrics_discarded_collection_reconciles_labels() {
    use astersql_pd_statistics_utils::RWType;
    let registry = Registry::new();
    let m = StatisticsMetrics::register(&registry).unwrap();
    let mut cache = HotPeerCache::new(RWType::READ);
    cache.set_peer_ttl(10);
    cache.calc_hot_thresholds(1, 0);
    m.collect_hot_cache_metrics(&cache.collect_metrics(0, &[1].into()));
    assert_eq!(values(&registry, "pd_hotcache_status").len(), 5);
    // A dropped waiter can discard a completed worker result. A later collection
    // must still reconcile the registry with the actual cache after GC.
    drop(cache.collect_metrics(10, &[].into()));
    m.collect_hot_cache_metrics(&cache.collect_metrics(11, &[].into()));
    assert!(
        values(&registry, "pd_hotcache_status").is_empty(),
        "dropped collection lost store-label cleanup"
    );
}
