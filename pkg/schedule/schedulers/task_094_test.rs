// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpStep};
use astersql_pd_statistics::*;
use std::sync::Arc;
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
async fn task_094_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_094.json"
    ))
    .unwrap();
    assert_eq!(fixture["evidence_kind"], "source_assertion");
    let stores = stores();
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
    let src = detail(1, [1000., 1000., 0., 0.]);
    let dst = detail(2, [500., 500., 0., 0.]);
    let p = peer(1, 10, [200., 200., 0., 0.]);
    let leader = region(10, &[1, 2, 3], 1);
    let member = region(10, &[1, 3, 4], 3);
    let mut count = 0;
    for version in ["v1", "v2"] {
        for c in fixture["cases"].as_array().unwrap() {
            let r = HotResource::ALL[c["resource"].as_u64().unwrap() as usize];
            let target = HotResource::ALL[c["winner"].as_u64().unwrap() as usize];
            let mut candidates: [Vec<HotSolveCandidate<'_>>; 4] = std::array::from_fn(|_| vec![]);
            candidates[target as usize].push(HotSolveCandidate::new(
                &src,
                &dst,
                &p,
                if target.operation() == HotSourceOperation::TransferLeader {
                    &leader
                } else {
                    &member
                },
            ));
            let out = scheduler("none", version)
                .dispatch(r, &candidates, &mut ctx)
                .await;
            assert_eq!(out.diagnostic, None);
            assert!(out.errors.is_empty());
            assert_eq!(out.offers.len(), 1);
            let (actual, result) = &out.offers[0];
            assert_eq!(*actual, target);
            assert_eq!(
                result.operators[0].desc(),
                c["description"].as_str().unwrap()
            );
            assert_eq!(result.operators[0].region_id(), 10);
            if target.operation() == HotSourceOperation::TransferLeader {
                assert!(
                    matches!(result.operators[0].step(0),Some(OpStep::TransferLeader(x)) if x.from_store==1 && x.to_store==2)
                );
            } else {
                assert!(
                    matches!(result.operators[0].step(0),Some(OpStep::AddLearner(x)) if x.to_store==2)
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 12);
    println!("12 frozen v1/v2 read/write route cases, real operator steps and targets");
}
#[test]
fn task_094_intervals_limits_and_config() {
    for previous in [i64::MIN, 0, 1, 20_000_000_000, i64::MAX] {
        assert_eq!(HotScheduler::next_interval(previous), 1_300_000_000);
    }
    for (count, limit, expected) in [
        (0, 0, false),
        (0, 1, true),
        (1, 1, false),
        (2, 1, false),
        (u64::MAX, u64::MAX, false),
    ] {
        assert_eq!(HotScheduler::is_schedule_allowed(count, limit), expected);
    }
    assert_eq!(HotScheduler::min_interval(), 1_000_000_000);
    let mut cfg = HotRegionParams::default();
    cfg.forbid_rw_type = "invalid".into();
    assert!(matches!(
        HotScheduler::new(cfg, false, false, Default::default()),
        Err(HotScheduleError::Config(_))
    ));
}
#[tokio::test]
async fn task_094_forbid_no_solution_and_faults() {
    let stores = stores();
    let config = ProviderSnapshot::<()>::default();
    let mut allocate = || Err(BuilderError::Allocation("injected".into()));
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &config,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000., 1000., 0., 0.]);
    let dst = detail(2, [500., 500., 0., 0.]);
    let p = peer(1, 10, [200., 200., 0., 0.]);
    let r = region(10, &[1, 3, 4], 3);
    let mut candidates: [Vec<HotSolveCandidate<'_>>; 4] = std::array::from_fn(|_| vec![]);
    candidates[0].push(HotSolveCandidate::new(&src, &dst, &p, &r));
    for version in ["v1", "v2"] {
        let out = scheduler("none", version)
            .dispatch(HotResource::WritePeer, &candidates, &mut ctx)
            .await;
        assert_eq!(out.diagnostic, Some(HotScheduleDiagnostic::NoSolution));
        assert!(out.offers.is_empty());
        assert_eq!(
            out.errors,
            vec![BuilderError::Allocation("injected".into())]
        );
        for resource in HotResource::ALL {
            let direction = if resource.rw() == astersql_pd_statistics_utils::RWType::READ {
                "read"
            } else {
                "write"
            };
            let out = scheduler(direction, version)
                .dispatch(resource, &candidates, &mut ctx)
                .await;
            assert_eq!(out.diagnostic, Some(HotScheduleDiagnostic::Forbidden));
            assert!(out.errors.is_empty());
        }
    }
    let mut s = scheduler("none", "v2");
    let empty = std::array::from_fn(|_| vec![]);
    let out = s
        .schedule(&HotResource::ALL, usize::MAX, &empty, &mut ctx)
        .await
        .unwrap();
    assert_eq!(out.diagnostic, Some(HotScheduleDiagnostic::NoSolution));
    assert!(matches!(
        s.schedule(&[], 0, &empty, &mut ctx).await,
        Err(HotScheduleError::EmptyResources)
    ));
    // Opposite direction remains enabled despite forbid-read.
    assert_eq!(
        scheduler("read", "v2")
            .dispatch(HotResource::WritePeer, &candidates, &mut ctx)
            .await
            .errors
            .len(),
        1
    );
}
#[tokio::test]
async fn task_094_read_competition_and_fallback() {
    let stores = stores();
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
    let src = detail(1, [1000., 1000., 0., 0.]);
    let dst = detail(2, [500., 500., 0., 0.]);
    let leader = region(10, &[1, 2, 3], 1);
    let member = region(11, &[1, 3, 4], 3);
    for version in ["v1", "v2"] {
        for (lead_load, peer_load, expected) in [
            (200., 100., HotResource::ReadLeader),
            (100., 200., HotResource::ReadPeer),
            (200., 200., HotResource::ReadPeer),
        ] {
            let l = peer(1, 10, [lead_load, lead_load, 0., 0.]);
            let p = peer(1, 11, [peer_load, peer_load, 0., 0.]);
            let candidates = [
                vec![],
                vec![],
                vec![HotSolveCandidate::new(&src, &dst, &p, &member)],
                vec![HotSolveCandidate::new(&src, &dst, &l, &leader)],
            ];
            let out = scheduler("none", version)
                .dispatch(HotResource::ReadPeer, &candidates, &mut ctx)
                .await;
            assert_eq!(out.offers.len(), 2);
            assert_eq!(
                out.offers[0].0, expected,
                "{version}, {lead_load}, {peer_load}"
            );
            assert_ne!(out.offers[0].0, out.offers[1].0);
            assert_eq!(out.offers[1].1.operators.len(), 1);
        }
    }
    println!(
        "6 real read winner comparisons; fallback retains executable operator for pending owner"
    );
}

#[tokio::test]
async fn task_094_sampling_and_partial_read_failure() {
    let stores = stores();
    let config = ProviderSnapshot::<()>::default();
    let mut allocate = || Err(BuilderError::Allocation("peer unavailable".into()));
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &config,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000., 1000., 0., 0.]);
    let dst = detail(2, [500., 500., 0., 0.]);
    let p = peer(1, 10, [200., 200., 0., 0.]);
    let l = peer(1, 11, [100., 100., 0., 0.]);
    let member = region(10, &[1, 3, 4], 3);
    let leader = region(11, &[1, 2, 3], 1);
    let candidates = [
        vec![],
        vec![HotSolveCandidate::new(&src, &dst, &l, &leader)],
        vec![HotSolveCandidate::new(&src, &dst, &p, &member)],
        vec![HotSolveCandidate::new(&src, &dst, &l, &leader)],
    ];
    for version in ["v1", "v2"] {
        let mut scheduler = scheduler("none", version);
        // Leader transfer allocates no peer ID. A peer solve fault must not erase it.
        let out = scheduler
            .schedule(&HotResource::ALL, 6, &candidates, &mut ctx)
            .await
            .unwrap();
        assert_eq!(out.diagnostic, None);
        assert_eq!(
            out.errors,
            vec![BuilderError::Allocation("peer unavailable".into())]
        );
        assert_eq!(out.offers.len(), 1);
        assert_eq!(out.offers[0].0, HotResource::ReadLeader);
        assert_eq!(out.offers[0].1.operators[0].region_id(), 11);
        // Write leader selection must never enter the failing read-peer solver.
        let out = scheduler
            .schedule(&HotResource::ALL, 5, &candidates, &mut ctx)
            .await
            .unwrap();
        assert!(out.errors.is_empty());
        assert_eq!(out.offers[0].0, HotResource::WriteLeader);
        assert_eq!(
            out.offers[0].1.operators[0].desc(),
            "transfer-hot-write-leader"
        );
    }
}
