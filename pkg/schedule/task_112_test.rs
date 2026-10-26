// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

fn put(cluster: &mut BasicCluster, id: u64, source: RegionSource) {
    let peer = Peer {
        id,
        store_id: 1,
        role: PeerRole::Voter,
        is_witness: false,
    };
    let mut region = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: id.to_be_bytes().to_vec(),
                end_key: (id + 1).to_be_bytes().to_vec(),
            },
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    region.set_source(source);
    let region = Arc::new(region);
    let (old, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, old.as_deref(), &overlaps, changed);
}

fn store(cluster: &BasicCluster, state: NodeState, reported: u32) {
    let mut store = StoreInfo::new(StoreMeta {
        id: 1,
        node_state: state,
        ..Default::default()
    });
    store.set_new_store_stats(StoreStats {
        region_count: reported,
        ..Default::default()
    });
    cluster.stores.put_store(Arc::new(store));
}

#[test]
fn task_112_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_112.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let mut cluster = BasicCluster::new();
        let source = match case["source"].as_str().unwrap() {
            "heartbeat" => RegionSource::Heartbeat,
            "sync" => RegionSource::Sync,
            _ => RegionSource::Storage,
        };
        for id in 1..=case["indexed"].as_u64().unwrap() {
            put(
                &mut cluster,
                id,
                if id <= case["active"].as_u64().unwrap() {
                    source
                } else {
                    RegionSource::Storage
                },
            );
        }
        let state = match case["state"].as_str().unwrap() {
            "serving" => NodeState::Serving,
            "preparing" => NodeState::Preparing,
            "removing" => NodeState::Removing,
            "removed" => NodeState::Removed,
            _ => NodeState::Unknown(99),
        };
        store(&cluster, state, case["reported"].as_u64().unwrap() as u32);
        let gate = PrepareChecker::new(Duration::from_secs(17), || {
            Ok::<_, ()>(case["total"].as_i64().unwrap())
        });
        assert!(!gate.is_prepared());
        let now =
            Duration::from_secs(17) + Duration::from_nanos(case["elapsed_ns"].as_u64().unwrap());
        let result = gate.should_run(&cluster, now);
        assert_eq!(
            result,
            case["expected"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(gate.is_prepared(), result);
    }
    println!("task112 frozen cases: 16 passed, 0 failed");
}

#[test]
fn task_112_live_collection_duplicates_and_sticky_admission() {
    let mut cluster = BasicCluster::new();
    store(&cluster, NodeState::Serving, 10);
    for id in 1..=10 {
        put(&mut cluster, id, RegionSource::Storage);
    }
    let gate = PrepareChecker::new(Duration::ZERO, || Ok::<_, ()>(10));
    assert!(!gate.should_run(&cluster, Duration::ZERO));
    for _ in 0..20 {
        put(&mut cluster, 1, RegionSource::Heartbeat);
    }
    assert_eq!(cluster.regions.not_from_storage_regions_count(), 1);
    assert_eq!(
        cluster.regions.not_from_storage_regions_count_by_store(1),
        1
    );
    assert_eq!(cluster.regions.total_region_count(), 10);
    assert!(!gate.should_run(&cluster, Duration::ZERO));
    for id in 2..=8 {
        put(&mut cluster, id, RegionSource::Heartbeat);
    }
    assert!(!gate.check(&cluster, Duration::ZERO));
    put(&mut cluster, 9, RegionSource::Heartbeat);
    assert!(gate.should_run(&cluster, Duration::ZERO));
    for id in 1..=10 {
        put(&mut cluster, id, RegionSource::Storage);
    }
    assert_eq!(cluster.regions.not_from_storage_regions_count(), 0);
    assert!(gate.should_run(&cluster, Duration::ZERO));
    let restarted = PrepareChecker::new(Duration::ZERO, || Ok::<_, ()>(10));
    assert!(!restarted.should_run(&cluster, Duration::ZERO));
}

#[test]
fn task_112_provider_failure_recovery_timeout_and_override() {
    #[derive(Debug)]
    enum ReadError {
        Unavailable,
    }
    let mut cluster = BasicCluster::new();
    store(&cluster, NodeState::Serving, 1);
    put(&mut cluster, 1, RegionSource::Heartbeat);
    let fail = AtomicBool::new(true);
    let calls = AtomicUsize::new(0);
    let gate = PrepareChecker::new(Duration::ZERO, || {
        calls.fetch_add(1, Ordering::SeqCst);
        if fail.load(Ordering::SeqCst) {
            Err(ReadError::Unavailable)
        } else {
            Ok(1)
        }
    });
    assert!(!gate.check(&cluster, Duration::ZERO));
    assert!(!gate.is_prepared());
    fail.store(false, Ordering::SeqCst);
    assert!(gate.check(&cluster, Duration::ZERO));
    fail.store(true, Ordering::SeqCst);
    assert!(gate.check(&cluster, Duration::ZERO));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let timed = PrepareChecker::new(Duration::ZERO, || Err::<i64, _>(ReadError::Unavailable));
    assert!(!timed.check(&cluster, COLLECT_TIMEOUT));
    assert!(timed.check(&cluster, COLLECT_TIMEOUT + Duration::from_nanos(1)));
    assert!(timed.is_prepared());
    let forced = PrepareChecker::new(Duration::ZERO, || -> Result<i64, ()> {
        panic!("override must bypass provider")
    });
    forced.set_prepared();
    assert!(forced.should_run(&BasicCluster::new(), Duration::ZERO));
}

#[test]
fn task_112_concurrent_sticky_transition() {
    let cluster = BasicCluster::new();
    let calls = AtomicUsize::new(0);
    let gate = PrepareChecker::new(Duration::ZERO, || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(0)
    });
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..16)
            .map(|_| scope.spawn(|| gate.should_run(&cluster, Duration::ZERO)))
            .collect();
        for handle in handles {
            assert!(handle.join().unwrap());
        }
    });
    assert!(gate.is_prepared());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
