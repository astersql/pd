// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpKind, OpStep};
use astersql_pd_statistics::*;
use astersql_pd_statistics_utils::RWType;
use serde_json::Value;
use std::sync::Arc;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_092.json"
    ))
    .unwrap()
}
fn loads(v: &Value) -> Loads {
    std::array::from_fn(|i| v[i].as_f64().unwrap())
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
fn peer(store_id: u64, region_id: u64, loads: Loads) -> HotPeerStat {
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
fn stores() -> StoresInfo {
    let s = StoresInfo::new();
    for id in 1..=4 {
        s.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    s
}
#[tokio::test]
async fn task_092_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    let stores = stores();
    let conf = ProviderSnapshot::<()>::default();
    for c in f["solve_cases"].as_array().unwrap() {
        let src = detail(1, [1000., 1000., 0., 0.]);
        let dst = detail(2, [500., 500., 0., 0.]);
        let p = peer(1, 10, loads(&c["main"]));
        let r = region(10, &[1, 3, 4], 3);
        let candidate = HotSolveCandidate::new(&src, &dst, &p, &r);
        let mut allocate = || Ok(100);
        let mut ctx = HotSolveContext {
            stores: &stores,
            config: &conf,
            rules: None,
            now_unix_nanos: 0,
            supports_conf_change_v2: false,
            allocate: &mut allocate,
        };
        let mut solve = solver(c["formula"].as_str().unwrap());
        let result = solve.solve(&[candidate], &mut ctx).await;
        assert!(result.errors.is_empty());
        assert_eq!(
            result.operators.len(),
            c["operators"].as_u64().unwrap() as usize,
            "{c}"
        );
        assert_eq!(result.best_rank, c["rank"].as_i64().map(|r| r as i32));
        if let Some(op) = result.operators.first() {
            assert_eq!(op.region_id(), 10);
            assert_eq!(op.priority_level(), PriorityLevel::HIGH);
            assert!(op.kind().0 & OpKind::HOT_REGION.0 != 0);
            assert!(op.need_store_health_check());
            assert!(matches!(op.step(0),Some(OpStep::AddLearner(x)) if x.to_store==2));
            assert!(matches!(op.step(op.len()-1),Some(OpStep::RemovePeer(x)) if x.from_store==1));
            assert_eq!(result.decorations[0].operation, "move-peer");
            assert_eq!(result.decorations[0].dimension, "all");
            assert_eq!(
                result.decorations[0].directions,
                vec![(1, "out"), (2, "in")]
            );
        }
    }
    println!("4 frozen solve cases: v1/v2 improving and no improvement");
}
#[test]
fn task_092_step_compare_and_readiness() {
    let f = fixture();
    for c in f["step_cases"].as_array().unwrap() {
        assert_eq!(
            hot_step_rank(
                c[0].as_f64().unwrap(),
                c[1].as_f64().unwrap(),
                c[2].as_f64().unwrap()
            ),
            c[3].as_i64().unwrap()
        );
    }
    let src = detail(1, [1000.; 4]);
    let dst = detail(2, [500.; 4]);
    let p = peer(1, 10, [200.; 4]);
    let r = region(10, &[1, 3, 4], 3);
    let mut c = HotSolveCandidate::new(&src, &dst, &p, &r);
    assert!(c.is_ready_to_build());
    let bad = peer(2, 10, [200.; 4]);
    c.main_peer = &bad;
    assert!(!c.is_ready_to_build());
    c.main_peer = &p;
    let bad = peer(1, 11, [200.; 4]);
    c.main_peer = &bad;
    assert!(!c.is_ready_to_build());
    c.main_peer = &p;
    let back = peer(2, 11, [100.; 4]);
    let rr = region(11, &[2, 3, 4], 3);
    c.revert_peer = Some(&back);
    assert!(!c.is_ready_to_build());
    c.revert_region = Some(&rr);
    assert!(c.is_ready_to_build());
    c.revert_peer = None;
    assert!(!c.is_ready_to_build());
    let s = solver("v1");
    assert_eq!(s.compare_src(&src, &dst), -1);
    assert_eq!(s.compare_dst(&src, &dst), 1);
    assert_eq!(s.compare_src(&src, &src), 0);
    let mut pending = detail(3, [1000.; 4]);
    pending.load_pred.future.loads = [1020.; 4];
    assert_eq!(s.compare_src(&src, &pending), -1);
    println!(
        "5 frozen step cases; identity/region/revert readiness and source/destination/pending comparisons"
    );
}
#[tokio::test]
async fn task_092_reverse_and_atomic_failure() {
    let f = fixture();
    let c = &f["reverse"];
    let src = detail(1, loads(&c["source"]));
    let dst = detail(2, loads(&c["destination"]));
    let p = peer(1, 10, loads(&c["main"]));
    let back = peer(2, 11, loads(&c["revert"]));
    let r = region(10, &[1, 3, 4], 3);
    let rr = region(11, &[2, 3, 4], 3);
    let mut candidate = HotSolveCandidate::new(&src, &dst, &p, &r);
    candidate.reverts.push(HotRevertCandidate {
        peer: &back,
        region: &rr,
        scope: None,
    });
    let stores = stores();
    let conf = ProviderSnapshot::<()>::default();
    let mut n = 100;
    let mut allocate = || {
        n += 1;
        Ok(n)
    };
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &conf,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let mut s = solver("v2");
    s.set_search_revert_regions(None);
    let result = s.solve(&[candidate], &mut ctx).await;
    assert_eq!(result.best_rank, Some(4));
    assert_eq!(result.operators.len(), 2);
    assert!(result.best_revert);
    assert_eq!(
        result
            .operators
            .iter()
            .map(|o| o.region_id())
            .collect::<Vec<_>>(),
        vec![10, 11]
    );
    assert_eq!(
        result.decorations[1].directions,
        vec![
            (2, "out"),
            (1, "in"),
            (2, "out-for-revert"),
            (1, "in-for-revert")
        ]
    );
    let mut candidate = HotSolveCandidate::new(&src, &dst, &p, &r);
    candidate.reverts.push(HotRevertCandidate {
        peer: &back,
        region: &rr,
        scope: None,
    });
    let mut count = 0;
    let mut fail = || {
        count += 1;
        if count == 3 {
            Err(BuilderError::Allocation("injected".into()))
        } else {
            Ok(200)
        }
    };
    ctx.allocate = &mut fail;
    let mut s = solver("v2");
    s.set_search_revert_regions(None);
    let result = s.solve(&[candidate], &mut ctx).await;
    // The main rank 2 candidate was successfully built before attempting the pair.
    assert_eq!(result.best_rank, Some(2));
    assert_eq!(result.operators.len(), 1);
    assert!(!result.best_revert);
    assert_eq!(
        result.errors,
        vec![BuilderError::Allocation("injected".into())]
    );
    println!("real reverse pair and allocation failure preserve previous executable winner");
}

#[tokio::test]
async fn task_092_best_tie_uniform_and_stale() {
    let stores = stores();
    let conf = ProviderSnapshot::<()>::default();
    let mut allocate = || Ok(100);
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &conf,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000.; 4]);
    let dst = detail(2, [500.; 4]);
    let p = peer(1, 10, [100.; 4]);
    let better = peer(1, 11, [200.; 4]);
    let r = region(10, &[1, 3, 4], 3);
    let rr = region(11, &[1, 3, 4], 3);
    let c = HotSolveCandidate::new(&src, &dst, &p, &r);
    let b = HotSolveCandidate::new(&src, &dst, &better, &rr);
    for version in ["v1", "v2"] {
        let result = solver(version)
            .solve(&[c.clone(), b.clone(), b.clone()], &mut ctx)
            .await;
        assert_eq!(result.operators.len(), 1);
        assert_eq!(result.operators[0].region_id(), 11);
        assert_eq!(result.best_rank, Some(4));
        let mut uniform = src.clone();
        uniform.load_pred.stddev.loads = [0.; 4];
        let u = HotSolveCandidate::new(&uniform, &dst, &better, &rr);
        assert!(
            solver(version)
                .solve(&[u], &mut ctx)
                .await
                .operators
                .is_empty()
        );
        let stale = peer(2, 10, [200.; 4]);
        let bad = HotSolveCandidate::new(&src, &dst, &stale, &r);
        let result = solver(version).solve(&[bad], &mut ctx).await;
        assert!(result.operators.is_empty());
    }
    let empty = solver("v2").solve(&[], &mut ctx).await;
    assert!(empty.operators.is_empty());
    println!("v1/v2 best candidate, equal tie, uniform skip, stale source and empty round");
}
#[tokio::test]
async fn task_092_real_leader_and_role_builders() {
    let stores = stores();
    let conf = ProviderSnapshot::<()>::default();
    let mut allocate = || Ok(100);
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &conf,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000.; 4]);
    let dst = detail(2, [500.; 4]);
    let p = peer(1, 10, [200.; 4]);
    for (members, leader, kind, steps) in [
        (&[1, 2, 3][..], 1, "transfer-leader", 1),
        (&[1, 3, 4][..], 1, "move-leader", 5),
        (&[1, 3, 4][..], 3, "move-peer", 3),
    ] {
        let r = region(10, members, leader);
        let c = HotSolveCandidate::new(&src, &dst, &p, &r);
        let (ops, d) = solver("v1").build_operators(&c, 4, &mut ctx).await.unwrap();
        assert_eq!(ops[0].len(), steps);
        assert_eq!(d[0].operation, kind);
        assert_eq!(
            ops[0].desc(),
            format!(
                "{}-hot-read-{}",
                if kind == "transfer-leader" {
                    "transfer"
                } else {
                    "move"
                },
                if kind == "move-peer" {
                    "peer"
                } else {
                    "leader"
                }
            )
        );
        assert!(ops[0].need_store_health_check());
        assert_eq!(ops[0].priority_level(), PriorityLevel::HIGH);
        if kind == "transfer-leader" {
            assert!(
                matches!(ops[0].step(0),Some(OpStep::TransferLeader(x)) if x.from_store==1 && x.to_store==2)
            );
        }
    }
    let r = region(10, &[1, 3, 4], 3);
    let mut meta = r.meta().clone();
    meta.peers[0].role = astersql_pd_core::PeerRole::Learner;
    let learner = RegionInfo::new(meta, r.leader().cloned(), vec![], vec![]);
    let c = HotSolveCandidate::new(&src, &dst, &p, &learner);
    let (ops, _) = solver("v1").build_operators(&c, 4, &mut ctx).await.unwrap();
    assert_eq!(ops[0].len(), 2);
    assert!(matches!(ops[0].step(0),Some(OpStep::AddLearner(x)) if x.to_store==2));
    let mut bad = c.clone();
    let stale = peer(2, 10, [200.; 4]);
    bad.main_peer = &stale;
    assert!(matches!(
        solver("v1").build_operators(&bad, 4, &mut ctx).await,
        Err(BuilderError::InvalidOriginPeer)
    ));
    println!(
        "transfer/move leader/move peer with real steps, learner role preservation and typed readiness failure"
    );
}
#[tokio::test]
async fn task_092_reverse_gates_and_search_rounds() {
    let stores = stores();
    let conf = ProviderSnapshot::<()>::default();
    let mut allocate = || Ok(100);
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &conf,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000., 500., 0., 0.]);
    let dst = detail(2, [500., 1000., 0., 0.]);
    let p = peer(1, 10, [200., 400., 0., 0.]);
    let back = peer(2, 11, [0., 600., 0., 0.]);
    let r = region(10, &[1, 3, 4], 3);
    let rr = region(11, &[2, 3, 4], 3);
    let mut c = HotSolveCandidate::new(&src, &dst, &p, &r);
    c.reverts.push(HotRevertCandidate {
        peer: &back,
        region: &rr,
        scope: None,
    });
    let mut s = solver("v2");
    let first = s.solve(&[c.clone()], &mut ctx).await;
    assert_eq!(first.best_rank, Some(2));
    assert_eq!(first.operators.len(), 1);
    let next = s.solve(&[c.clone()], &mut ctx).await;
    assert_eq!(next.best_rank, Some(4));
    assert_eq!(next.operators.len(), 2);
    for bad_region in [region(10, &[2, 3, 4], 3), region(11, &[1, 2, 3], 3)] {
        let mut bad = c.clone();
        bad.reverts[0].region = &bad_region;
        let out = s.solve(&[bad], &mut ctx).await;
        assert_eq!(out.best_rank, Some(2));
        assert_eq!(out.operators.len(), 1);
    }
    // v1 never searches, even after an empty round.
    let mut v1 = solver("v1");
    v1.set_search_revert_regions(None);
    assert!(!v1.solve(&[c.clone()], &mut ctx).await.best_revert);
    // A stale reverse peer ID cannot build an atomic pair.
    let stale = peer(2, 12, [0., 600., 0., 0.]);
    c.reverts[0].peer = &stale;
    s.set_search_revert_regions(None);
    let out = s.solve(&[c], &mut ctx).await;
    assert_eq!(out.operators.len(), 1);
    assert!(!out.best_revert);
    for dims in [[0, 0], [4, 1]] {
        assert!(matches!(
            HotSolver::new(
                HotRegionParams::default(),
                dims,
                RWType::READ,
                HotSourceOperation::MovePeer,
                StoreLoad::default(),
                StoreLoad::default(),
                StoreLoad::default()
            ),
            Err(HotSolveError::InvalidPriorities)
        ));
    }
    println!(
        "initial disabled/retry enabled search, same-region and membership/stale reverse rejection, v1 no search, typed dimensions"
    );
}

#[tokio::test]
async fn task_092_placement_population_and_builder_fault() {
    use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp, Rule};
    let src = detail(1, [1000., 500., 0., 0.]);
    let mut dst = detail(2, [500., 1000., 0., 0.]);
    let mut fourth = detail(4, [0.; 4]);
    for d in [&mut dst, &mut fourth] {
        d.store_summary_info
            .store_info
            .set_labels(vec![astersql_pd_core::StoreLabel {
                key: "pool".into(),
                value: "b".into(),
            }]);
    }
    let mut populations =
        HotPlacementLoads::new(vec![src.clone(), dst.clone(), detail(3, [0.; 4]), fourth]).unwrap();
    let rule = Rule {
        label_constraints: vec![LabelConstraint {
            key: "pool".into(),
            op: LabelConstraintOp::In,
            values: vec!["b".into()],
        }],
        ..Default::default()
    };
    let scope = populations.scope(&[&rule], true).unwrap();
    assert_eq!(scope.store_ids(), &[2, 4]);
    let p = peer(1, 10, [200., 400., 0., 0.]);
    let back = peer(2, 11, [0., 600., 0., 0.]);
    let r = region(10, &[1, 3, 4], 3);
    let rr = region(11, &[2, 3, 4], 3);
    let mut c = HotSolveCandidate::new(&src, &dst, &p, &r);
    c.reverts.push(HotRevertCandidate {
        peer: &back,
        region: &rr,
        scope: Some(&scope),
    });
    let stores = stores();
    let conf = ProviderSnapshot::<()>::default();
    let mut allocate = || Ok(100);
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &conf,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let mut s = solver("v2");
    s.set_search_revert_regions(None);
    let out = s.solve(&[c.clone()], &mut ctx).await;
    assert_eq!(out.best_rank, Some(2));
    assert_eq!(out.operators.len(), 1);
    assert!(!out.best_revert);
    // A failure at the first allocation leaves no executable winner.
    c.reverts[0].scope = None;
    let mut allocate = || Err(BuilderError::Allocation("no IDs".into()));
    ctx.allocate = &mut allocate;
    let out = s.solve(&[c], &mut ctx).await;
    assert!(out.operators.is_empty());
    assert_eq!(out.best_rank, None);
    assert!(!out.errors.is_empty());
    assert!(
        out.errors
            .iter()
            .all(|e| matches!(e,BuilderError::Allocation(x) if x=="no IDs"))
    );
    let mut invalid = HotRegionParams::default();
    invalid.max_peer_number = -1;
    assert!(matches!(
        HotSolver::new(
            invalid,
            [0, 1],
            RWType::READ,
            HotSourceOperation::MovePeer,
            StoreLoad::default(),
            StoreLoad::default(),
            StoreLoad::default()
        ),
        Err(HotSolveError::InvalidPeerLimit)
    ));
    println!(
        "canonical placement population rejects reverse out-of-scope target; first allocation failure returns no operation and typed errors"
    );
}
