// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::*;
use astersql_pd_statistics::*;
use astersql_pd_statistics_utils::RWType;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
impl ManualClock {
    fn advance(&self, n: u64) {
        *self.0.lock().unwrap() += Duration::from_secs(n);
    }
}
fn op(id: u64, clock: Arc<ManualClock>) -> Arc<Operator> {
    Arc::new(Operator::with_clock(
        "hot",
        "",
        id,
        None,
        OpKind::HOT_REGION | OpKind::LEADER,
        1,
        vec![OpStep::TransferLeader(TransferLeader {
            from_store: 1,
            to_store: 2,
            to_stores: vec![],
        })],
        clock,
    ))
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_095.json"
    ))
    .unwrap()
}
fn peer(store: u64, id: u64, loads: Vec<f64>) -> HotPeerStat {
    HotPeerStat {
        store_id: store,
        region_id: id,
        loads,
        ..Default::default()
    }
}
fn stores() -> std::collections::HashMap<u64, StoreSummaryInfo> {
    summary_store_infos(
        &(1..=3)
            .map(|id| {
                StoreInfo::new(StoreMeta {
                    id,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>(),
    )
}
#[test]
fn task_095_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    for c in f["lifecycle"].as_array().unwrap() {
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        let op = op(10, clock.clone());
        let mut pending = HotLoadProjection::default();
        pending
            .try_add_pending_influence(
                op.clone(),
                vec![1],
                2,
                Influence {
                    loads: vec![10.; 8],
                    hot_peer_count: 1.,
                },
                Duration::from_secs(30),
            )
            .unwrap();
        let status = c[0].as_str().unwrap();
        if status != "expired" {
            assert!(op.start());
        }
        match status {
            "success" => {
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
            "canceled" => {
                assert!(op.cancel());
            }
            "replaced" => {
                assert!(op.replace());
            }
            "timeout" => {
                clock.advance(100);
            }
            "expired" => {
                clock.advance(4);
            }
            _ => {}
        }
        // First summary observes automatic timeout/expiry at the injected host time.
        pending.summary_pending_influence(&mut stores(), None, clock.as_ref());
        clock.advance(c[1].as_u64().unwrap());
        for _ in 0..2 {
            let mut s = stores();
            pending.summary_pending_influence(
                &mut s,
                Some(&HotRegionParams {
                    pending_weight: 2.5,
                    ..Default::default()
                }),
                clock.as_ref(),
            );
            assert_eq!(
                !pending.region_pendings.contains_key(&10),
                c[3].as_bool().unwrap(),
                "{c}"
            );
            let expected = c[2].as_f64().unwrap() * 2.5;
            assert_eq!(
                s[&1].pending_sum.as_ref().map_or(0., |i| i.loads[0]),
                -10. * expected,
                "{c}"
            );
            assert_eq!(
                s[&2].pending_sum.as_ref().map_or(0., |i| i.hot_peer_count),
                expected,
                "{c}"
            );
        }
        if status == "timeout" {
            assert_eq!(op.status(), TIMEOUT);
        }
        if status == "expired" {
            assert_eq!(op.status(), EXPIRED);
        }
    }
    println!(
        "11 lifecycle fixtures x 2 heartbeat rounds; real success/cancel/timeout/expiry/replace, pending weight 2.5"
    );
}
#[test]
fn task_095_collect_and_duplicate_faults() {
    let f = fixture();
    for c in f["collect"].as_array().unwrap() {
        let values = |v: &serde_json::Value| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let rw = if c["rw"] == 0 {
            RWType::READ
        } else {
            RWType::WRITE
        };
        let p = peer(1, 10, values(&c["main"]));
        let inverse = (!c["inverse"].is_null()).then(|| peer(1, 10, values(&c["inverse"])));
        let infl = collect_hot_pending_influence(rw, &p, inverse.as_ref()).unwrap();
        assert_eq!(infl.loads, values(&c["expected"]));
        assert_eq!(infl.hot_peer_count, 1.);
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        let first = op(10, clock.clone());
        let mut pending = HotLoadProjection::default();
        pending
            .try_add_pending_influence(first.clone(), vec![1], 2, infl, Duration::from_secs(30))
            .unwrap();
        assert_eq!(
            pending.try_add_pending_influence(
                op(10, clock),
                vec![3],
                1,
                Influence::default(),
                Duration::ZERO
            ),
            Err(HotPendingError::AlreadyPending(10))
        );
        assert!(Arc::ptr_eq(&pending.region_pendings[&10].op, &first));
        assert_eq!(pending.region_pendings[&10].froms, vec![1]);
        let mut wrong = peer(2, 10, vec![0.; 4]);
        assert_eq!(
            collect_hot_pending_influence(rw, &p, Some(&wrong)),
            Err(HotPendingError::StaleInverse)
        );
        wrong.store_id = 1;
        wrong.region_id = 11;
        assert_eq!(
            collect_hot_pending_influence(rw, &p, Some(&wrong)),
            Err(HotPendingError::StaleInverse)
        );
        assert_eq!(
            collect_hot_pending_influence(rw, &peer(1, 10, vec![]), None),
            Err(HotPendingError::ShortLoads)
        );
    }
    println!(
        "4 frozen collect cases; duplicate does not replace or account twice; stale/short input errors"
    );
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
fn solver_peer(store_id: u64, region_id: u64, loads: Loads) -> HotPeerStat {
    HotPeerStat {
        store_id,
        region_id,
        loads: loads.to_vec(),
        ..Default::default()
    }
}
fn region(id: u64, stores: &[u64], leader: u64) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&s| Peer {
            id: id * 10 + s,
            store_id: s,
            ..Default::default()
        })
        .collect();
    let lead = peers.iter().find(|p| p.store_id == leader).cloned();
    RegionInfo::new(
        RegionMeta {
            id,
            peers,
            ..Default::default()
        },
        lead,
        vec![],
        vec![],
    )
}
fn solver_stores() -> StoresInfo {
    let s = StoresInfo::new();
    for id in 1..=4 {
        s.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    s
}

fn scheduler(forbid: &str, version: &str) -> HotScheduler {
    HotScheduler::new(
        HotRegionParams {
            rank_formula_version: version.into(),
            read_priorities: vec!["byte".into(), "key".into()],
            write_leader_priorities: vec!["byte".into(), "key".into()],
            forbid_rw_type: forbid.into(),
            min_hot_byte_rate: 1.,
            min_hot_key_rate: 1.,
            ..Default::default()
        },
        true,
        true,
        std::array::from_fn(|_| HotRankInputs {
            step: StoreLoad {
                loads: [10.; 4],
                hot_peer_count: 1.,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap()
}

#[tokio::test]
async fn task_095_dispatch_admission_fallback_and_reverse_conflict() {
    let stores = solver_stores();
    let config = ProviderSnapshot::<()>::default();
    let mut allocate = || Ok(100);
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &config,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000.; 4]);
    let dst = detail(2, [500.; 4]);
    let l = solver_peer(1, 10, [100.; 4]);
    let p = solver_peer(1, 11, [200.; 4]);
    let leader = region(10, &[1, 2, 3], 1);
    let member = region(11, &[1, 3, 4], 3);
    let candidates = [
        vec![],
        vec![],
        vec![HotSolveCandidate::new(&src, &dst, &p, &member)],
        vec![HotSolveCandidate::new(&src, &dst, &l, &leader)],
    ];
    for version in ["v1", "v2"] {
        let mut scheduler = scheduler("none", version);
        let out = scheduler
            .dispatch(HotResource::ReadPeer, &candidates, &mut ctx)
            .await;
        assert_eq!(out.offers[0].0, HotResource::ReadPeer);
        let mut pending = HotLoadProjection::default();
        let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
        pending
            .try_add_pending_influence(
                op(11, clock),
                vec![1],
                2,
                Influence::default(),
                Duration::from_secs(30),
            )
            .unwrap();
        let (selected, errors) =
            out.admit_pending(&mut pending, &HotRegionParams::default(), |_, _, _| None);
        assert_eq!(errors, vec![HotPendingError::AlreadyPending(11)]);
        let (_, selected) = selected.unwrap();
        assert_eq!(selected.operators[0].region_id(), 10);
        assert_eq!(pending.region_pendings.len(), 2);
        assert_eq!(
            pending.region_pendings[&10].origin.loads,
            vec![100., 100., 100., 0., 0., 0., 100., 0.]
        );
        assert_eq!(
            pending.region_pendings[&10].max_zombie_duration,
            Duration::from_secs(30)
        );
    }
    // Construct actual main/reverse operators using the canonical operator builder.
    let back = solver_peer(2, 12, [50.; 4]);
    let back_region = region(12, &[2, 3, 4], 3);
    let mut c = HotSolveCandidate::new(&src, &dst, &p, &member);
    c.revert_peer = Some(&back);
    c.revert_region = Some(&back_region);
    let solver = HotSolver::new(
        HotRegionParams::default(),
        [0, 1],
        RWType::WRITE,
        HotSourceOperation::MovePeer,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let (ops, decorations) = solver.build_operators(&c, -4, &mut ctx).await.unwrap();
    assert_eq!(ops.len(), 2);
    let result = HotSolveResult {
        operators: ops,
        decorations,
        best: Some(c),
        ..Default::default()
    };
    let mut pending = HotLoadProjection::default();
    pending
        .admit_hot_solution(
            HotResource::WritePeer,
            &result,
            &HotRegionParams::default(),
            |_, _, _| None,
        )
        .unwrap();
    assert_eq!(pending.region_pendings[&12].froms, vec![2]);
    assert_eq!(pending.region_pendings[&12].to, 1);
    assert_eq!(
        pending.region_pendings[&12].origin.loads,
        vec![0., 0., 0., 50., 50., 50., 0., 50.]
    );
    // Go inserts main before a conflicting reverse; preserve the recorded main.
    pending.region_pendings.remove(&11);
    assert_eq!(
        pending.admit_hot_solution(
            HotResource::WritePeer,
            &result,
            &HotRegionParams::default(),
            |_, _, _| None
        ),
        Err(HotPendingError::AlreadyPending(12))
    );
    assert_eq!(pending.region_pendings.len(), 2);
    assert!(Arc::ptr_eq(
        &pending.region_pendings[&11].op,
        &result.operators[0]
    ));
    println!(
        "2 formulas: first read offer pending, actual leader fallback; real reverse pair and sequential conflict retention"
    );
}

#[test]
fn task_095_split_and_admission_errors() {
    use astersql_pd_core::StoreLabel;
    use astersql_pd_schedule_hbstream::SplitPolicy;
    let r = region(10, &[1, 2, 3], 1);
    let p = solver_peer(1, 10, [10.; 4]);
    let op = Arc::new(
        create_split_region_operator(
            "split",
            &r,
            OpKind::SPLIT,
            SplitPolicy::UseKey,
            vec![b"x".to_vec()],
        )
        .unwrap(),
    );
    let mut pending = HotLoadProjection::default();
    pending
        .admit_hot_split(
            op.clone(),
            &r,
            RWType::READ,
            &p,
            None,
            Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(pending.region_pendings[&10].froms, vec![1, 2, 3]);
    assert_eq!(pending.region_pendings[&10].to, 0);
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let mut s = stores();
    pending.summary_pending_influence(&mut s, None, clock.as_ref());
    for i in 1..=3 {
        assert_eq!(s[&i].pending_sum.as_ref().unwrap().loads[0], -10.);
    }
    assert_eq!(
        pending.admit_hot_split(op.clone(), &r, RWType::READ, &p, None, Duration::ZERO),
        Err(HotPendingError::AlreadyPending(10))
    );
    assert_eq!(
        pending.admit_hot_split(
            op,
            &region(99, &[1, 2, 3], 1),
            RWType::READ,
            &p,
            None,
            Duration::ZERO
        ),
        Err(HotPendingError::StaleRegion)
    );
    // Go GetStoreIDs is a set: even duplicate snapshot peers count once per store.
    let duplicate_region = region(20, &[1, 1, 2], 1);
    let duplicate_peer = solver_peer(1, 20, [10.; 4]);
    let split = Arc::new(
        create_split_region_operator(
            "split",
            &duplicate_region,
            OpKind::SPLIT,
            SplitPolicy::UseKey,
            vec![b"x".to_vec()],
        )
        .unwrap(),
    );
    let mut duplicate_pending = HotLoadProjection::default();
    duplicate_pending
        .admit_hot_split(
            split,
            &duplicate_region,
            RWType::READ,
            &duplicate_peer,
            None,
            Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(duplicate_pending.region_pendings[&20].froms, vec![1, 2]);
    assert_eq!(
        pending.admit_hot_solution(
            HotResource::ReadPeer,
            &HotSolveResult::default(),
            &HotRegionParams::default(),
            |_, _, _| None
        ),
        Err(HotPendingError::MissingSolution)
    );
    let src = detail(1, [0.; 4]);
    let mut dst = detail(2, [0.; 4]);
    dst.store_summary_info.store_info = StoreInfo::new(StoreMeta {
        id: 2,
        labels: vec![StoreLabel {
            key: "engine".into(),
            value: "tiflash".into(),
        }],
        ..Default::default()
    });
    let result = HotSolveResult {
        operators: vec![self::op(10, clock)],
        best: Some(HotSolveCandidate::new(&src, &dst, &p, &r)),
        ..Default::default()
    };
    pending.region_pendings.clear();
    assert_eq!(
        pending.admit_hot_solution(
            HotResource::ReadPeer,
            &result,
            &HotRegionParams::default(),
            |_, _, _| None
        ),
        Err(HotPendingError::EngineMismatch)
    );
    assert!(pending.region_pendings.is_empty());
}

#[test]
fn task_095_zombie_provenance_and_typed_errors() {
    use astersql_pd_core::StoreLabel;
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    for (resource, engine, first, seconds) in [
        (HotResource::ReadPeer, "", "byte", 30),
        (HotResource::ReadLeader, "", "byte", 30),
        (HotResource::WritePeer, "", "byte", 30),
        (HotResource::WritePeer, "tiflash", "byte", 180),
        (HotResource::WriteLeader, "", "key", 180),
        (HotResource::WriteLeader, "", "query", 30),
    ] {
        let mut src = detail(1, [0.; 4]);
        let mut dst = detail(2, [0.; 4]);
        for (d, id) in [(&mut src, 1), (&mut dst, 2)] {
            d.store_summary_info.store_info = StoreInfo::new(StoreMeta {
                id,
                labels: vec![StoreLabel {
                    key: "engine".into(),
                    value: engine.into(),
                }],
                ..Default::default()
            });
        }
        let r = region(10, &[1, 2, 3], 1);
        let p = solver_peer(1, 10, [10.; 4]);
        let result = HotSolveResult {
            operators: vec![op(10, clock.clone())],
            best: Some(HotSolveCandidate::new(&src, &dst, &p, &r)),
            ..Default::default()
        };
        let config = HotRegionParams {
            write_leader_priorities: vec![first.into(), "cpu".into()],
            ..Default::default()
        };
        let mut pending = HotLoadProjection::default();
        pending
            .admit_hot_solution(resource, &result, &config, |_, _, _| None)
            .unwrap();
        assert_eq!(
            pending.region_pendings[&10].max_zombie_duration,
            Duration::from_secs(seconds)
        );
        pending.region_pendings.clear();
        let mut invalid = config.clone();
        invalid.max_peer_number = -1;
        assert_eq!(
            pending.admit_hot_solution(resource, &result, &invalid, |_, _, _| None),
            Err(HotPendingError::Source(HotSourceError::InvalidPeerLimit))
        );
        assert!(pending.region_pendings.is_empty());
        let mut stale = result;
        stale.operators = vec![op(99, clock.clone())];
        assert_eq!(
            pending.admit_hot_solution(resource, &stale, &config, |_, _, _| None),
            Err(HotPendingError::StaleRegion)
        );
    }
    println!("6 source-derived zombie duration routes; typed invalid config and stale region");
}
