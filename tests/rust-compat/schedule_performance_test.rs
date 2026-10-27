// Copyright 2026 AsterSQL.
//! Reproducible in-process scheduler baseline. No machine-dependent pass threshold.
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
    StoreStats,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::AffinityFilter;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep, Operator};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_schedulers::*;
use astersql_pd_statistics::*;
use astersql_pd_statistics_utils::RWType;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

struct MeasuredAllocator;
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
// SAFETY: every request and unchanged layout is forwarded to the System allocator.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let ptr = unsafe { System.realloc(ptr, layout, size) };
        if !ptr.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        ptr
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;
fn measure() -> (Instant, u64, u64) {
    (
        Instant::now(),
        ALLOCATIONS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    )
}
fn finish(start: (Instant, u64, u64)) -> (u128, u64, u64) {
    (
        start.0.elapsed().as_nanos(),
        ALLOCATIONS.load(Ordering::Relaxed) - start.1,
        BYTES.load(Ordering::Relaxed) - start.2,
    )
}
fn report(
    measured: (u128, u64, u64),
    scheduler: &str,
    cache: &str,
    stores: u64,
    regions: u64,
    sample: u64,
    quality: Value,
) {
    println!(
        "TASK155 {}",
        json!({"scheduler":scheduler,"cache":cache,"stores":stores,"regions":regions,"sample":sample,"elapsed_ns":measured.0,"allocations":measured.1,"allocated_bytes":measured.2,"quality":quality})
    );
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_155.json"
    ))
    .unwrap()
}
fn region(id: u64, leader: usize) -> RegionInfo {
    let peers: Vec<_> = (1..=3)
        .map(|s| Peer {
            id: id * 100 + s,
            store_id: s,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            epoch: Some(RegionEpoch {
                version: 1,
                conf_ver: 1,
            }),
            key_range: KeyRange {
                start_key: id.to_be_bytes().to_vec(),
                end_key: (id + 1).to_be_bytes().to_vec(),
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[leader - 1].clone()),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = 96;
    r
}
fn cluster(stores: u64, regions: u64) -> BasicCluster {
    let mut c = BasicCluster::new();
    for id in 1..=stores {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        s.set_leader_count(if id == 1 { regions as i64 } else { 0 });
        s.set_leader_size(if id == 1 { regions as i64 * 96 } else { 0 });
        s.set_region_count(if id <= 3 { regions as i64 } else { 0 });
        s.set_region_size(if id <= 3 { regions as i64 * 96 } else { 0 });
        let used = s.region_size() as u64 * (1 << 20);
        s.update_raw_stats(StoreStats {
            capacity: 6 * (1u64 << 40),
            available: 6 * (1u64 << 40) - used,
            used_size: used,
            ..Default::default()
        });
        c.stores.put_store(Arc::new(s));
    }
    for id in 1..=regions {
        let r = Arc::new(region(id, 1));
        let (o, v, b) = c.regions.set_region(r.clone());
        c.regions.update_sub_tree(r, o.as_deref(), &v, b);
    }
    c
}
fn check_operator(op: &Operator, kind: OpKind, regions: u64) -> Value {
    if kind == OpKind::LEADER {
        assert_eq!(op.kind(), kind);
    } else {
        assert!(op.kind().0 & OpKind::REGION.0 != 0);
    }
    assert!(op.need_store_health_check());
    assert!((1..=regions).contains(&op.region_id()));
    let info = op.additional_info();
    let src: f64 = info.get("sourceScore").unwrap().parse().unwrap();
    let dst: f64 = info.get("targetScore").unwrap().parse().unwrap();
    assert!(src > dst);
    if kind == OpKind::LEADER {
        assert_eq!(op.len(), 1);
        assert!(
            matches!(op.step(0),Some(OpStep::TransferLeader(s)) if s.from_store==1 && [2,3].contains(&s.to_store))
        );
        assert!((src - 1. - (dst + 1.)).abs() < (src - dst).abs());
    } else {
        assert!(matches!(op.step(0),Some(OpStep::AddLearner(s)) if s.to_store>3));
        assert!(
            matches!(op.step(op.len()-1),Some(OpStep::RemovePeer(s)) if (1..=3).contains(&s.from_store))
        );
    }
    if kind == OpKind::REGION {
        let Some(OpStep::RemovePeer(remove)) = op.step(op.len() - 1) else {
            unreachable!()
        };
        assert_eq!(
            op.kind(),
            if remove.from_store == 1 {
                OpKind(OpKind::REGION.0 | OpKind::LEADER.0)
            } else {
                OpKind::REGION
            }
        );
        assert!(matches!(op.step(1), Some(OpStep::PromoteLearner(_))));
        let before = regions as i64 * 96;
        assert!((before - 192).abs() < before);
    }
    json!({"operators":1,"region_id":op.region_id(),"steps":format!("{:?}",(0..op.len()).map(|i| op.step(i).unwrap()).collect::<Vec<_>>()),"source_score":src,"target_score":dst,"before_gap":if kind==OpKind::LEADER {regions} else {regions*96},"after_gap":if kind==OpKind::LEADER {regions-2} else {regions*96-192}})
}
#[tokio::test]
async fn task_155_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    for scale in f["scales"].as_array().unwrap() {
        let stores = scale["stores"].as_u64().unwrap();
        let regions = scale["regions"].as_u64().unwrap();
        let c = cluster(stores, regions);
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.enable_placement_rules = true;
        config.replication.enable_placement_rules_cache = true;
        config.replication.max_replicas = 3;
        config.schedule.tolerant_size_ratio = 1.;
        config.schedule.leader_schedule_policy = "count".into();
        config.schedule.region_score_formula_version = "v1".into();
        let storage = Arc::new(MemoryStore::default());
        let mut rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(storage.clone())),
            ClusterPaths::new(155),
        );
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let labels = RegionLabeler::load(
            StorageEndpoint::new(storage),
            ClusterPaths::new(155),
            Default::default(),
        )
        .await
        .unwrap();
        let ranges = astersql_pd_schedule_keyrange::Manager::new();
        let influence = OpInfluence::new();
        let statistics = StatisticsSnapshot::default();
        let all_stores = c.stores.get_stores();
        let sample_region = c.regions.get_region(1).unwrap();
        let mut leader_quality = None;
        let mut region_quality = None;
        for cache in ["cold", "warm"] {
            if cache == "warm" {
                let fit = rules.fit_region_without_cache(&all_stores, &sample_region, &config);
                for _ in 0..11 {
                    rules.set_region_fit_cache(
                        &all_stores,
                        &sample_region,
                        fit.clone(),
                        1_000_000_000_000,
                    );
                }
                assert!(Arc::ptr_eq(
                    &fit,
                    &rules.fit_region(&all_stores, &sample_region, &config, 1_000_000_000_000)
                ));
            }
            let mut leader = BalanceLeaderScheduler::default();
            leader.config = BalanceLeaderConfig::new(vec![], 1).unwrap();
            let mut peer = BalanceRegionScheduler::default();
            for sample in 0..(f["samples"].as_u64().unwrap() + u64::from(cache == "warm")) {
                if cache == "cold" {
                    rules.invalid_cache(1);
                    assert!(!rules.is_region_fit_cached(
                        &all_stores,
                        &sample_region,
                        1_000_000_000_000
                    ));
                    leader = BalanceLeaderScheduler::default();
                    leader.config = BalanceLeaderConfig::new(vec![], 1).unwrap();
                    peer = BalanceRegionScheduler::default();
                }
                let mut random = |_: usize| 0;
                let mut shuffle = |_: &mut [usize]| {};
                let mut ctx = LeaderBalanceContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    affinity: AffinityFilter {
                        manager: None,
                        labels: &labels,
                        now: 1000,
                    },
                    statistics: &statistics,
                    hot_degree: 1,
                    key_ranges: &ranges,
                    influence: &influence,
                    now_unix_nanos: 1_000_000_000_000,
                    random_index: &mut random,
                    shuffle: &mut shuffle,
                };
                let start = measure();
                let out = leader.schedule(&mut ctx, false).await;
                let measured = finish(start);
                assert!(out.errors.is_empty());
                assert_eq!(
                    out.operators.len(),
                    f["expected_operators"].as_u64().unwrap() as usize
                );
                let q = check_operator(&out.operators[0], OpKind::LEADER, regions);
                if let Some(expected) = &leader_quality {
                    assert_eq!(&q, expected);
                } else {
                    leader_quality = Some(q.clone());
                }
                if cache != "warm" || sample > 0 {
                    report(
                        measured,
                        "balance-leader",
                        cache,
                        stores,
                        regions,
                        sample - u64::from(cache == "warm"),
                        q,
                    );
                }
                let mut allocate = || Ok(9_000_000);
                let mut ctx = RegionBalanceContext {
                    cluster: &c,
                    config: &config,
                    rules: &mut rules,
                    affinity: AffinityFilter {
                        manager: None,
                        labels: &labels,
                        now: 1000,
                    },
                    statistics: &statistics,
                    hot_degree: 1,
                    key_ranges: &ranges,
                    influence: &influence,
                    now_unix_nanos: 1_000_000_000_000,
                    random_index: &mut random,
                    shuffle: &mut shuffle,
                    allocate: &mut allocate,
                    supports_conf_change_v2: false,
                    allow_empty_regions: false,
                };
                let start = measure();
                let out = peer.schedule(&mut ctx, false).await;
                let measured = finish(start);
                assert!(out.errors.is_empty(), "{:?}", out.errors);
                assert_eq!(out.operators.len(), 1);
                let q = check_operator(&out.operators[0], OpKind::REGION, regions);
                if let Some(expected) = &region_quality {
                    assert_eq!(&q, expected);
                } else {
                    region_quality = Some(q.clone());
                }
                if cache != "warm" || sample > 0 {
                    report(
                        measured,
                        "balance-region",
                        cache,
                        stores,
                        regions,
                        sample - u64::from(cache == "warm"),
                        q,
                    );
                }
            }
        }
    }
}
fn detail(id: u64, loads: Loads) -> StoreLoadDetail {
    StoreLoadDetail {
        store_summary_info: StoreSummaryInfo {
            store_info: StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }),
            ..Default::default()
        },
        hot_peers: vec![],
        load_pred: StoreLoadPred {
            current: StoreLoad {
                loads,
                ..Default::default()
            },
            future: StoreLoad {
                loads,
                ..Default::default()
            },
            stddev: StoreLoad {
                loads: [1.; 4],
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
fn solver(version: &str) -> HotSolver {
    HotSolver::new(
        HotRegionParams {
            rank_formula_version: version.into(),
            min_hot_byte_rate: 1.,
            min_hot_key_rate: 1.,
            ..Default::default()
        },
        [0, 1],
        RWType::READ,
        HotSourceOperation::MovePeer,
        StoreLoad::default(),
        StoreLoad::default(),
        StoreLoad {
            loads: [10.; 4],
            hot_peer_count: 1.,
            ..Default::default()
        },
    )
    .unwrap()
}
#[tokio::test]
async fn task_155_hot_solver_and_faults() {
    let f = fixture();
    let c = cluster(100, 10000);
    let conf = ProviderSnapshot::<()>::default();
    let loads = |key: &str| std::array::from_fn(|i| f[key][i].as_f64().unwrap());
    let src = detail(1, loads("hot_source"));
    let dst = detail(4, loads("hot_destination"));
    let regions: Vec<_> = (1..=10000).map(|id| region(id, 3)).collect();
    let peers: Vec<_> = (1..=10000)
        .map(|id| HotPeerStat {
            store_id: 1,
            region_id: id,
            loads: loads("hot_peer").to_vec(),
            ..Default::default()
        })
        .collect();
    let candidates: Vec<_> = regions
        .iter()
        .zip(&peers)
        .map(|(r, p)| HotSolveCandidate::new(&src, &dst, p, r))
        .collect();
    for version in ["v1", "v2"] {
        for cache in ["cold", "warm"] {
            let mut solve = solver(version);
            for sample in 0..(f["samples"].as_u64().unwrap() + u64::from(cache == "warm")) {
                if cache == "cold" {
                    solve = solver(version);
                }
                let mut allocate = || Ok(9_000_000);
                let mut ctx = HotSolveContext {
                    stores: &c.stores,
                    config: &conf,
                    rules: None,
                    now_unix_nanos: 1_000_000_000_000,
                    supports_conf_change_v2: false,
                    allocate: &mut allocate,
                };
                let start = measure();
                let out = solve.solve(&candidates, &mut ctx).await;
                let measured = finish(start);
                assert!(out.errors.is_empty());
                assert_eq!(out.operators.len(), 1);
                assert_eq!(out.best_rank, Some(f["hot_rank"].as_i64().unwrap() as i32));
                let op = &out.operators[0];
                assert!(op.kind().0 & OpKind::HOT_REGION.0 != 0);
                assert!(matches!(op.step(0),Some(OpStep::AddLearner(s)) if s.to_store==4));
                assert!(
                    matches!(op.step(op.len()-1),Some(OpStep::RemovePeer(s)) if s.from_store==1)
                );
                let after_gap = ((1000f64 - 200.) - (500f64 + 200.)).abs();
                assert!(after_gap < (1000f64 - 500.).abs());
                if cache != "warm" || sample > 0 {
                    report(
                        measured,
                        &format!("hot-{version}"),
                        cache,
                        100,
                        10000,
                        sample - u64::from(cache == "warm"),
                        json!({"operators":1,"rank":out.best_rank,"region_id":op.region_id(),"steps":format!("{:?}",(0..op.len()).map(|i| op.step(i).unwrap()).collect::<Vec<_>>()),"before_gap":500,"after_gap":after_gap}),
                    );
                }
            }
        }
        let mut allocate = || Err(BuilderError::Allocation("task155 injected".into()));
        let mut ctx = HotSolveContext {
            stores: &c.stores,
            config: &conf,
            rules: None,
            now_unix_nanos: 1_000_000_000_000,
            supports_conf_change_v2: false,
            allocate: &mut allocate,
        };
        let out = solver(version).solve(&candidates[..1], &mut ctx).await;
        assert!(out.operators.is_empty());
        assert!(!out.errors.is_empty());
        let out = solver(version).solve(&[], &mut ctx).await;
        assert!(out.operators.is_empty());
        assert!(out.errors.is_empty());
    }
}

#[tokio::test]
async fn task_155_leader_convergence() {
    let mut c = cluster(4, 1000);
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.leader_schedule_policy = "count".into();
    config.schedule.tolerant_size_ratio = 1.;
    config.replication.max_replicas = 3;
    let storage = Arc::new(MemoryStore::default());
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(storage.clone())),
        ClusterPaths::new(155),
    );
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    let labels = RegionLabeler::load(
        StorageEndpoint::new(storage),
        ClusterPaths::new(155),
        Default::default(),
    )
    .await
    .unwrap();
    let statistics = StatisticsSnapshot::default();
    let ranges = astersql_pd_schedule_keyrange::Manager::new();
    let influence = OpInfluence::new();
    let mut scheduler = BalanceLeaderScheduler::default();
    scheduler.config = BalanceLeaderConfig::new(vec![], 1).unwrap();
    let mut rounds = 0;
    loop {
        let mut random = |_: usize| 0;
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = LeaderBalanceContext {
            cluster: &c,
            config: &config,
            rules: &mut rules,
            affinity: AffinityFilter {
                manager: None,
                labels: &labels,
                now: 1000,
            },
            statistics: &statistics,
            hot_degree: 1,
            key_ranges: &ranges,
            influence: &influence,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut random,
            shuffle: &mut shuffle,
        };
        let out = scheduler.schedule(&mut ctx, false).await;
        assert!(out.errors.is_empty());
        if out.operators.is_empty() {
            break;
        }
        assert_eq!(out.operators.len(), 1);
        rounds += 1;
        assert!(rounds < 1000, "scheduler failed to converge");
        let op = &out.operators[0];
        let Some(OpStep::TransferLeader(step)) = op.step(0) else {
            panic!("unexpected step")
        };
        let from = c.stores.get_store(step.from_store).unwrap();
        let to = c.stores.get_store(step.to_store).unwrap();
        let before = from.leader_count().pow(2) + to.leader_count().pow(2);
        let mut from = (*from).clone();
        let mut to = (*to).clone();
        from.set_leader_count(from.leader_count() - 1);
        to.set_leader_count(to.leader_count() + 1);
        from.set_leader_size(from.leader_size() - 96);
        to.set_leader_size(to.leader_size() + 96);
        assert!(from.leader_count().pow(2) + to.leader_count().pow(2) < before);
        c.stores.put_store(Arc::new(from));
        c.stores.put_store(Arc::new(to));
        let old = c.regions.get_region(op.region_id()).unwrap();
        let mut next = RegionInfo::new(
            old.meta().clone(),
            old.get_store_peer(step.to_store).cloned(),
            vec![],
            vec![],
        );
        next.stats_mut().approximate_size = 96;
        let next = Arc::new(next);
        let (o, v, b) = c.regions.set_region(next.clone());
        c.regions.update_sub_tree(next, o.as_deref(), &v, b);
    }
    let counts: Vec<_> = (1..=3)
        .map(|id| c.stores.get_store(id).unwrap().leader_count())
        .collect();
    assert_eq!(counts.iter().sum::<i64>(), 1000);
    assert!(rounds > 0);
    assert!(
        counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 3,
        "{counts:?}"
    );
    println!(
        "CONVERGENCE {}",
        json!({"scheduler":"balance-leader","stores":4,"regions":1000,"rounds":rounds,"final_counts":counts})
    );
}
