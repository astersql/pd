// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, KeyRange, Peer, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_operator::{CancelReason, ControllerAdmission, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{cell::RefCell, sync::Arc};

fn region(id: u64, start: &str, end: &str) -> Arc<RegionInfo> {
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            peers: vec![peer.clone()],
            epoch: Some(RegionEpoch {
                version: 2,
                conf_ver: 1,
            }),
            key_range: KeyRange {
                start_key: start.as_bytes().to_vec(),
                end_key: end.as_bytes().to_vec(),
            },
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    ))
}
fn observe(cluster: &mut BasicCluster, starts: &[&str]) {
    let mut boundaries = vec!["a"];
    boundaries.extend_from_slice(starts);
    boundaries.push("z");
    for (i, pair) in boundaries.windows(2).enumerate() {
        cluster
            .regions
            .set_region(region(i as u64 + 2, pair[0], pair[1]));
    }
}
fn keys(values: &[&str]) -> Vec<Vec<u8>> {
    values.iter().map(|s| s.as_bytes().to_vec()).collect()
}
fn config() -> ProviderSnapshot<()> {
    let mut c = ProviderSnapshot::default();
    c.replication.enable_placement_rules = false;
    c.replication.max_replicas = 1;
    c
}
fn rules() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(64),
    )
}
#[test]
fn task_064_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_064.json"
    ))
    .unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let strings = |field: &str| {
            c[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap())
                .collect::<Vec<_>>()
        };
        let conf = config();
        let mut manager = rules();
        let context = RegionReplicationContext {
            conf: &conf,
            stores: &[],
            manager: RefCell::new(&mut manager),
            now: 0,
        };
        let mut cluster = BasicCluster::new();
        cluster.regions.set_region(region(1, "a", "z"));
        let ctl = ControllerAdmission::new(10);
        let mut retry = SplitRetry::new(keys(&strings("keys")), 0);
        let submissions = retry
            .start_attempt(0, &cluster, &ctl, &context, |_, _| panic!("replicated"))
            .unwrap();
        assert!(submissions.rejected.is_empty());
        observe(&mut cluster, &strings("observe"));
        retry.advance(99, false, &cluster.regions);
        assert_eq!(retry.phase(), SplitRetryPhase::Watching);
        retry.advance(100, false, &cluster.regions);
        retry.advance(
            if c["cancel"] == true { 150 } else { 60_000 },
            c["cancel"].as_bool().unwrap(),
            &cluster.regions,
        );
        if let SplitRetryPhase::Backoff { until_ms } = retry.phase() {
            retry.advance(until_ms - 1, true, &cluster.regions);
            assert!(matches!(retry.phase(), SplitRetryPhase::Backoff { .. }));
            retry.advance(until_ms, true, &cluster.regions);
        }
        assert_eq!(retry.phase(), SplitRetryPhase::Done, "{}", c["id"]);
        let result = retry.result();
        assert_eq!(
            result.percentage as u64,
            c["pct"].as_u64().unwrap(),
            "{}",
            c["id"]
        );
        assert_eq!(
            serde_json::to_value(result.region_ids).unwrap(),
            c["ids"],
            "{}",
            c["id"]
        );
        assert_eq!(
            result.remaining_keys,
            keys(&strings("remaining")),
            "{}",
            c["id"]
        );
        assert!(
            retry
                .start_attempt(100_000, &cluster, &ctl, &context, |_, _| {})
                .is_none()
        );
    }
    println!("task_064: 10 frozen source-derived cases passed");
}
#[test]
fn task_064_retries_remaining_and_deduplicates_ids() {
    let conf = config();
    let mut manager = rules();
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut cluster = BasicCluster::new();
    cluster.regions.set_region(region(1, "a", "z"));
    let ctl = ControllerAdmission::new(10);
    let mut retry = SplitRetry::new(keys(&["m", "t", "t"]), 1);
    retry
        .start_attempt(0, &cluster, &ctl, &context, |_, _| {})
        .unwrap();
    observe(&mut cluster, &["m"]);
    retry.advance(100, false, &cluster.regions);
    retry.advance(60_000, false, &cluster.regions);
    assert_eq!(retry.result().percentage, 34);
    assert_eq!(retry.remaining_keys(), keys(&["t", "t"]));
    retry.advance(60_100, false, &cluster.regions);
    ctl.remove_operators(CancelReason::AdminStop);
    let submissions = retry
        .start_attempt(60_100, &cluster, &ctl, &context, |_, _| {})
        .unwrap();
    assert_eq!(submissions.admitted.len(), 1);
    let Some(OpStep::SplitRegion(step)) = submissions.admitted[0].admitted[0].step(0) else {
        panic!("split command")
    };
    assert_eq!(step.split_keys, keys(&["t", "t"]));
    // Retain original right-region ID across attempts; result IDs must be a set.
    cluster.regions.set_region(region(4, "m", "t"));
    cluster.regions.set_region(region(3, "t", "z"));
    retry.advance(60_200, false, &cluster.regions);
    retry.advance(120_100, false, &cluster.regions);
    assert_eq!(retry.phase(), SplitRetryPhase::Done);
    assert_eq!(
        retry.result(),
        SplitRetryResult {
            percentage: 100,
            region_ids: vec![3],
            remaining_keys: vec![]
        }
    );
}
#[test]
fn task_064_cancel_retries_and_capped_final_sleep() {
    let conf = config();
    let mut manager = rules();
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut cluster = BasicCluster::new();
    cluster.regions.set_region(region(1, "a", "z"));
    let ctl = ControllerAdmission::new(1);
    let mut retry = SplitRetry::new(keys(&["m"]), 11);
    let mut now = 0;
    for i in 0..=11 {
        let submissions = retry
            .start_attempt(now, &cluster, &ctl, &context, |_, _| {})
            .unwrap();
        if i == 0 {
            assert_eq!(submissions.admitted.len(), 1);
        } else {
            assert!(matches!(
                submissions.rejected[&1],
                SplitExecutionError::Admission(_)
            ));
        }
        // Already-cancelled parent still allows admission, but never a scan.
        observe(&mut cluster, &["m"]);
        retry.advance(now, true, &cluster.regions);
        assert_eq!(retry.result().percentage, 0);
        let delay = (100_u64 << i).min(60_000);
        assert_eq!(
            retry.phase(),
            SplitRetryPhase::Backoff {
                until_ms: now + delay
            }
        );
        now += delay;
        retry.advance(now, true, &cluster.regions);
        if i < 11 {
            assert_eq!(retry.phase(), SplitRetryPhase::Ready);
            cluster.regions = astersql_pd_core::RegionsInfo::new();
            cluster.regions.set_region(region(1, "a", "z"));
        }
    }
    assert_eq!(retry.phase(), SplitRetryPhase::Done);
    assert!(retry.result().region_ids.is_empty());
}
#[test]
fn task_064_empty_and_negative_limit() {
    for (input, limit) in [(vec![], 5), (keys(&["m"]), -1)] {
        let retry = SplitRetry::new(input.clone(), limit);
        assert_eq!(retry.phase(), SplitRetryPhase::Done);
        assert_eq!(
            retry.result(),
            SplitRetryResult {
                percentage: 0,
                region_ids: vec![],
                remaining_keys: input
            }
        );
    }
}
