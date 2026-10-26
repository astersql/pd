// Copyright 2026 AsterSQL.
use super::*;
use prometheus::{Registry, proto::MetricType};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_110.json"
    ))
    .unwrap()
}
fn value(r: &Registry, name: &str) -> Option<f64> {
    r.gather()
        .into_iter()
        .find(|f| f.name() == name)
        .and_then(|f| {
            if name == "pd_scheduler_event_count" {
                return Some(
                    f.get_metric()
                        .iter()
                        .map(|m| m.get_counter().get_value())
                        .sum(),
                );
            }
            f.get_metric().first().map(|m| {
                if f.get_field_type() == MetricType::COUNTER {
                    m.get_counter().get_value()
                } else {
                    m.get_gauge().get_value()
                }
            })
        })
}
#[test]
fn task_110_behavior_cases() {
    let f = fixture();
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    for row in f["families"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let labels: Vec<_> = row["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|_| "1")
            .collect();
        m.record_metrics(name, &labels, 7.).unwrap();
        let family = r.gather().into_iter().find(|x| x.name() == name).unwrap();
        assert_eq!(family.help(), row["help"].as_str().unwrap());
        assert_eq!(
            format!("{:?}", family.get_field_type()).to_lowercase(),
            row["kind"]
        );
        let mut names: Vec<_> = family.get_metric()[0]
            .get_label()
            .iter()
            .map(|l| l.name())
            .collect();
        names.sort();
        let mut expected: Vec<_> = row["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect();
        expected.sort();
        assert_eq!(names, expected);
    }
    for row in f["families"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let variable = row["variable"].as_str().unwrap();
        let reset = f["reset"].as_array().unwrap().iter().any(|x| x == variable);
        m.reset_scheduler_metrics();
        assert_eq!(
            r.gather().iter().any(|x| x.name() == name),
            !reset,
            "{variable}"
        );
    }
    println!("20 family descriptor cases; 20 reset membership cases");
}
#[test]
fn task_110_store_cleanup_and_faults() {
    let f = fixture();
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    for row in f["families"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let labs = row["labels"].as_array().unwrap();
        for store in ["1", "10"] {
            let values: Vec<_> = labs
                .iter()
                .map(|l| if l == "store" { store } else { "x" })
                .collect();
            m.record_metrics(name, &values, 2.).unwrap();
        }
    }
    for dir in ["1-in", "1-out", "10-in", "10-out"] {
        m.record_metrics("pd_scheduler_balance_witness", &["move-witness", dir], 1.)
            .unwrap();
    }
    m.delete_store_metrics("1");
    m.delete_store_metrics("1");
    for family in r.gather() {
        if f["families"].as_array().unwrap().iter().any(|row| {
            row["name"] == family.name()
                && f["delete_store"]
                    .as_array()
                    .unwrap()
                    .contains(&row["variable"])
        }) {
            assert!(
                family
                    .get_metric()
                    .iter()
                    .flat_map(|x| x.get_label())
                    .all(|l| l.name() != "store" || l.value() != "1")
            );
            assert!(
                family
                    .get_metric()
                    .iter()
                    .flat_map(|x| x.get_label())
                    .any(|l| l.name() == "store" && l.value() == "10")
            );
        }
        if family.name() == "pd_scheduler_balance_witness" {
            assert!(
                family
                    .get_metric()
                    .iter()
                    .flat_map(|x| x.get_label())
                    .all(|l| !["1-in", "1-out"].contains(&l.value()))
            );
        }
    }
    assert!(m.record_metrics("unknown", &[], 1.).is_err());
    assert!(m.record_metrics("pd_scheduler_status", &[], 1.).is_err());
    assert!(
        m.record_metrics("pd_scheduler_event_count", &["x", "y"], -1.)
            .is_err()
    );
    assert!(
        m.record_metrics("pd_scheduler_status", &["x", "y"], f64::NAN)
            .is_err()
    );
    assert!(SchedulerMetrics::register(&r).is_err());
    // A late registration collision rolls back earlier additions and preserves host ownership.
    let r2 = Registry::new();
    let host = prometheus::Gauge::new(
        "pd_balance_range_job",
        "job status for balance range schedule",
    )
    .unwrap();
    r2.register(Box::new(host.clone())).unwrap();
    assert!(SchedulerMetrics::register(&r2).is_err());
    r2.unregister(Box::new(host)).unwrap();
    assert!(SchedulerMetrics::register(&r2).is_ok());
}
#[test]
fn task_110_events_status_and_histogram() {
    use astersql_pd_schedule_types::SCHEDULERS;
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    let handles: Vec<_> = SCHEDULERS
        .iter()
        .map(|typ| m.event_metrics(typ.as_str(), "schedule").unwrap())
        .collect();
    for h in &handles {
        h.inc();
        h.inc();
    }
    m.reset_scheduler_metrics();
    for h in &handles {
        h.inc();
        assert_eq!(h.get(), 3.);
    }
    let family = r
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_scheduler_event_count")
        .unwrap();
    assert_eq!(
        family
            .get_metric()
            .iter()
            .filter(|m| m
                .get_label()
                .iter()
                .any(|l| l.name() == "name" && l.value() == "schedule"))
            .count(),
        18
    );
    m.collect_scheduler_metrics([("a", false), ("b", true)], false, None, None, 0);
    let status = r
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_scheduler_status")
        .unwrap();
    for metric in status.get_metric() {
        let name = metric
            .get_label()
            .iter()
            .find(|l| l.name() == "kind")
            .unwrap()
            .value();
        assert_eq!(
            metric.get_gauge().get_value(),
            if name == "a" { 1. } else { 0. }
        );
    }
    m.collect_scheduler_metrics([("a", false)], true, None, None, 0);
    assert_eq!(value(&r, "pd_scheduler_status"), Some(0.));
    m.remove_scheduler_metrics("a");
    m.remove_scheduler_metrics("b");
    assert_eq!(value(&r, "pd_scheduler_status"), None);
    m.record_metrics("pd_scheduler_hot_peer", &["peer", "read", "byte"], 3.)
        .unwrap();
    let family = r
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_scheduler_hot_peer")
        .unwrap();
    let h = family.get_metric()[0].get_histogram();
    assert_eq!(h.get_bucket().len(), 30);
    assert_eq!(h.get_bucket()[0].upper_bound(), 1.);
    assert_eq!(h.get_bucket()[29].upper_bound(), 2_f64.powi(29));
    assert_eq!(h.get_sample_sum(), 3.);
    assert_eq!(h.get_sample_count(), 1);
    let r2 = Registry::new();
    let isolated = SchedulerMetrics::register(&r2).unwrap();
    isolated
        .event_metrics("balance-leader-scheduler", "schedule")
        .unwrap()
        .inc();
    assert_eq!(handles[0].get(), 3.);
    println!(
        "18 cached event counter cases; pause/halt/remove; 30 histogram boundaries; isolated registries"
    );
}

use astersql_pd_core::{BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_schedule_operator::ControllerAdmission;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
fn now() -> i64 {
    1_000_000_000_000
}
fn region(id: u64, witness: bool) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            is_witness: id == 1 && witness,
            ..Default::default()
        })
        .collect();
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: astersql_pd_core::KeyRange {
                start_key: vec![id as u8],
                end_key: vec![(id + 1) as u8],
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ))
}
fn cluster() -> Arc<BasicCluster> {
    let mut c = BasicCluster::new();
    for id in 1..=3 {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(now());
        c.stores.put_store(Arc::new(s));
    }
    for id in 1..=4 {
        c.regions.put_region(region(id, true));
    }
    Arc::new(c)
}
fn policy() -> WitnessLoopScheduler {
    WitnessLoopScheduler {
        policy: TransferWitnessLeaderScheduler::new(),
        config: Default::default(),
        rules: astersql_pd_schedule_placement::RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            astersql_pd_keypath::ClusterPaths::new(108),
        ),
        now_nanos: now,
        random_index: |_| 0,
        faults: vec![],
    }
}

#[tokio::test]
async fn task_110_real_loop_and_cleanup() {
    use astersql_pd_schedule_types::TRANSFER_WITNESS_LEADER_SCHEDULER;
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    m.collect_scheduler_metrics([("witness", false)], false, None, None, 0);
    let instrumented = MetricsScheduler::new(
        policy(),
        m.clone(),
        &TRANSFER_WITNESS_LEADER_SCHEDULER,
        "witness".into(),
    )
    .unwrap();
    let mut c = ScheduleController::new(instrumented, cluster(), LoopControl::default(), now);
    assert!(c.schedule(false).await.unwrap().is_empty());
    assert_eq!(value(&r, "pd_scheduler_event_count"), Some(10.));
    let sender = c.scheduler.scheduler.policy.recv_region_info();
    assert!(send_witness_event(&sender, region(1, true)));
    assert_eq!(c.schedule(false).await.unwrap().len(), 1);
    assert_eq!(value(&r, "pd_scheduler_event_count"), Some(11.));
    c.control.stop();
    assert!(c.schedule(false).await.unwrap().is_empty());
    assert_eq!(value(&r, "pd_scheduler_event_count"), Some(11.));
    let admission = Arc::new(ControllerAdmission::new(Default::default()));
    let exit = c.run(admission).await;
    assert!(exit.cleanup.is_ok());
    assert_eq!(value(&r, "pd_scheduler_status"), None);
    // Drop is also a cleanup boundary on an unwind, without resetting event counters.
    m.collect_scheduler_metrics([("witness", false)], false, None, None, 0);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _policy = MetricsScheduler::new(
            policy(),
            m.clone(),
            &TRANSFER_WITNESS_LEADER_SCHEDULER,
            "witness".into(),
        )
        .unwrap();
        panic!("task110 host boundary fault");
    }));
    assert!(panicked.is_err());
    assert_eq!(value(&r, "pd_scheduler_status"), None);
    assert_eq!(value(&r, "pd_scheduler_event_count"), Some(11.));
}
#[tokio::test]
async fn task_110_real_counts_and_hot_pending() {
    use astersql_pd_schedule_core::BasicCluster as _;
    use astersql_pd_statistics::{Influence, StoreSummaryInfo};
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut rules = astersql_pd_schedule_placement::RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(110),
    );
    rules
        .initialize(
            3,
            &[],
            "",
            false,
            &astersql_pd_schedule_config::ProviderSnapshot::<()>::default(),
            &[],
        )
        .await
        .unwrap();
    let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
        endpoint,
        astersql_pd_keypath::ClusterPaths::new(110),
        0,
    )
    .await
    .unwrap();
    m.collect_scheduler_metrics([], false, Some(&rules), Some(&labels), 0);
    assert_eq!(value(&r, "pd_rule_manager_status"), Some(1.));
    assert_eq!(value(&r, "pd_region_label_status"), Some(0.));
    let c = cluster();
    let mut stores = std::collections::HashMap::from([(
        1,
        StoreSummaryInfo {
            store_info: (*c.get_store(1).unwrap()).clone(),
            pending_sum: Some(Influence {
                loads: vec![-1., 2., 3., 4., 5., 6., 7., 8.],
                hot_peer_count: 0.,
            }),
        },
    )]);
    m.collect_hot_pending_metrics(c.as_ref(), &stores).unwrap();
    let family = r
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_scheduler_hot_pending_sum")
        .unwrap();
    assert_eq!(family.get_metric().len(), 8);
    for metric in family.get_metric() {
        let label = |name| {
            metric
                .get_label()
                .iter()
                .find(|l| l.name() == name)
                .unwrap()
                .value()
        };
        let expected = match (label("rw"), label("dim")) {
            ("read", "byte") => -1.,
            ("read", "key") => 2.,
            ("read", "query") => 3.,
            ("write", "byte") => 4.,
            ("write", "key") => 5.,
            ("write", "query") => 6.,
            ("read", "cpu") => 7.,
            ("write", "cpu") => 8.,
            _ => panic!("unexpected labels"),
        };
        assert_eq!(metric.get_gauge().get_value(), expected);
    }
    m.delete_store_metrics("1");
    stores
        .get_mut(&1)
        .unwrap()
        .pending_sum
        .as_mut()
        .unwrap()
        .loads = vec![1.];
    assert!(m.collect_hot_pending_metrics(c.as_ref(), &stores).is_err());
    assert_eq!(value(&r, "pd_scheduler_hot_pending_sum"), None);
    stores
        .get_mut(&1)
        .unwrap()
        .pending_sum
        .as_mut()
        .unwrap()
        .loads = vec![1.; 8];
    // The store snapshot remains live; the canonical registry has now buried it.
    let mut buried = (*c.get_store(1).unwrap()).clone();
    buried.set_store_state(astersql_pd_core::StoreState::Tombstone, Some(false));
    c.stores.put_store(Arc::new(buried));
    m.collect_hot_pending_metrics(c.as_ref(), &stores).unwrap();
    assert_eq!(value(&r, "pd_scheduler_hot_pending_sum"), None);
    stores.insert(
        999,
        StoreSummaryInfo {
            pending_sum: Some(Influence {
                loads: vec![1.; 8],
                hot_peer_count: 0.,
            }),
            ..Default::default()
        },
    );
    m.collect_hot_pending_metrics(c.as_ref(), &stores).unwrap();
    assert_eq!(value(&r, "pd_scheduler_hot_pending_sum"), None);
}
#[test]
fn task_110_bury_during_publication() {
    struct BuryAfterRead {
        c: Arc<BasicCluster>,
        reads: std::sync::atomic::AtomicUsize,
    }
    impl astersql_pd_schedule_core::BasicCluster for BuryAfterRead {
        fn stores(&self) -> &astersql_pd_core::StoresInfo {
            &self.c.stores
        }
        fn regions(&self) -> &astersql_pd_core::RegionsInfo {
            &self.c.regions
        }
        fn get_store(&self, id: u64) -> Option<Arc<StoreInfo>> {
            let before = self.c.get_store(id);
            if self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                let mut buried = (**before.as_ref().unwrap()).clone();
                buried.set_store_state(astersql_pd_core::StoreState::Tombstone, Some(false));
                self.c.stores.put_store(Arc::new(buried));
            }
            before
        }
    }
    let c = BuryAfterRead {
        c: cluster(),
        reads: std::sync::atomic::AtomicUsize::new(0),
    };
    let stores = std::collections::HashMap::from([(
        1,
        astersql_pd_statistics::StoreSummaryInfo {
            pending_sum: Some(astersql_pd_statistics::Influence {
                loads: vec![1.; 8],
                hot_peer_count: 0.,
            }),
            ..Default::default()
        },
    )]);
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    m.record_metrics("pd_scheduler_evicted_stopping_store_status", &["1"], 1.)
        .unwrap();
    m.collect_hot_pending_metrics(&c, &stores).unwrap();
    assert_eq!(c.reads.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(value(&r, "pd_scheduler_hot_pending_sum"), None);
    assert_eq!(
        value(&r, "pd_scheduler_evicted_stopping_store_status"),
        Some(1.)
    );
    // The recheck only clears hot pending, matching the source's narrow cleanup.
}
#[test]
fn task_110_source_event_labels() {
    use astersql_pd_schedule_types::*;
    let r = Registry::new();
    let m = SchedulerMetrics::register(&r).unwrap();
    for typ in SCHEDULERS {
        let handle = m
            .scheduler_event_metrics(&typ, "instance-name", "schedule")
            .unwrap();
        handle.inc();
        let expected = if typ == GRANT_LEADER_SCHEDULER {
            "grant-hot-region-scheduler"
        } else if typ == BALANCE_WITNESS_SCHEDULER || typ == EVICT_SLOW_TREND_SCHEDULER {
            "instance-name"
        } else {
            typ.as_str()
        };
        let family = r
            .gather()
            .into_iter()
            .find(|f| f.name() == "pd_scheduler_event_count")
            .unwrap();
        assert!(family.get_metric().iter().any(|metric| {
            metric.get_counter().get_value() > 0.
                && metric
                    .get_label()
                    .iter()
                    .any(|l| l.name() == "type" && l.value() == expected)
        }));
    }
}

#[test]
fn task_110_eager_source_events() {
    let r = Registry::new();
    let _metrics = SchedulerMetrics::register(&r).unwrap();
    let f = fixture();
    let expected: std::collections::BTreeSet<(String, String)> = f["initial_events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap().into(),
                row[1].as_str().unwrap().into(),
            )
        })
        .collect();
    let family = r
        .gather()
        .into_iter()
        .find(|f| f.name() == "pd_scheduler_event_count")
        .unwrap();
    let actual: std::collections::BTreeSet<_> = family
        .get_metric()
        .iter()
        .map(|m| {
            assert_eq!(m.get_counter().get_value(), 0.);
            let label = |name| {
                m.get_label()
                    .iter()
                    .find(|l| l.name() == name)
                    .unwrap()
                    .value()
                    .to_owned()
            };
            (label("type"), label("name"))
        })
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 101);
}
