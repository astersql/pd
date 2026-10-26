// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::PlacementGuardContext;
use astersql_pd_schedule_placement::{
    LabelConstraint, LabelConstraintOp, PeerRoleType, Rule, RuleManager,
};
use astersql_pd_statistics::*;
use astersql_pd_statistics_utils::RWType;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::Value;
use std::sync::Arc;
fn config() -> HotRegionParams {
    HotRegionParams {
        dst_tolerance_ratio: 1.,
        enable_for_tiflash: true,
        ..Default::default()
    }
}
fn selector(c: HotRegionParams, write: bool, moving: bool) -> HotDestinationSelector {
    HotDestinationSelector::new(
        c,
        [0, 1],
        if write { RWType::WRITE } else { RWType::READ },
        if moving {
            HotSourceOperation::MovePeer
        } else {
            HotSourceOperation::TransferLeader
        },
    )
    .unwrap()
}
fn detail(id: u64, current: Loads, future: Loads, engine: &str) -> StoreLoadDetail {
    StoreLoadDetail {
        store_summary_info: StoreSummaryInfo {
            store_info: StoreInfo::new(StoreMeta {
                id,
                labels: if engine.is_empty() {
                    vec![]
                } else {
                    vec![StoreLabel {
                        key: "engine".into(),
                        value: engine.into(),
                    }]
                },
                ..Default::default()
            }),
            ..Default::default()
        },
        load_pred: StoreLoadPred {
            current: StoreLoad {
                loads: current,
                ..Default::default()
            },
            future: StoreLoad {
                loads: future,
                ..Default::default()
            },
            expect: StoreLoad {
                loads: [100.; 4],
                ..Default::default()
            },
            ..Default::default()
        },
        hot_peers: vec![],
    }
}
#[test]
fn task_090_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_090.json"
    ))
    .unwrap();
    assert_eq!(f["evidence_kind"], "source_assertion");
    for row in f["destination_cases"].as_array().unwrap() {
        let loads = |i: usize| std::array::from_fn(|d| row[i][d].as_f64().unwrap());
        let mut c = config();
        c.rank_formula_version = row[3].as_str().unwrap().into();
        c.strict_picking_store = row[4].as_bool().unwrap();
        let s = selector(c, row[5].as_bool().unwrap(), row[6].as_bool().unwrap());
        let d = detail(1, loads(1), loads(2), row[7].as_str().unwrap());
        assert_eq!(
            s.destination_store_decision(&d, None).label(),
            row[8].as_str().unwrap(),
            "{}",
            row[0]
        );
    }
    println!("10 frozen source-derived destination threshold cases");
}
#[test]
fn task_090_history_scope_and_faults() {
    let mut d = detail(1, [80.; 4], [80.; 4], "");
    d.load_pred.current.history_loads = [vec![80., 100.], vec![80., 100.], vec![], vec![]];
    d.load_pred.expect.history_loads = [vec![100.; 2], vec![100.; 2], vec![], vec![]];
    let s = selector(config(), false, true);
    assert_eq!(
        s.destination_store_decision(&d, None),
        HotDestinationDecision::HistoryFailed
    );
    d.load_pred.current.history_loads[1] = vec![80.; 2];
    for (version, strict, write, moving, want) in [
        (
            "v1",
            false,
            false,
            true,
            HotDestinationDecision::HistoryFailed,
        ),
        (
            "v1",
            true,
            false,
            true,
            HotDestinationDecision::HistoryFailed,
        ),
        ("v2", true, false, true, HotDestinationDecision::Eligible),
        (
            "v2",
            true,
            true,
            false,
            HotDestinationDecision::HistoryFailed,
        ),
    ] {
        let mut c = config();
        c.rank_formula_version = version.into();
        c.strict_picking_store = strict;
        assert_eq!(
            selector(c, write, moving).destination_store_decision(&d, None),
            want
        );
    }
    let expect = StoreLoad {
        loads: [200.; 4],
        history_loads: [vec![200.; 2], vec![200.; 2], vec![], vec![]],
        ..Default::default()
    };
    let scope = HotDestinationScope {
        store_ids: &[2],
        expect: &expect,
    };
    assert_eq!(
        s.destination_store_decision(&d, Some(&scope)),
        HotDestinationDecision::OutOfScope
    );
    let scope = HotDestinationScope {
        store_ids: &[1],
        expect: &expect,
    };
    assert_eq!(
        s.destination_store_decision(&d, Some(&scope)),
        HotDestinationDecision::Eligible
    );
    d.load_pred.expect.history_loads[0].clear();
    assert!(std::panic::catch_unwind(|| s.destination_store_decision(&d, None)).is_err());
    for dims in [[0, 0], [0, 4], [5, 2]] {
        assert!(matches!(
            HotDestinationSelector::new(config(), dims, RWType::READ, HotSourceOperation::MovePeer),
            Err(HotDestinationError::InvalidPriorities)
        ));
    }
    let s =
        HotDestinationSelector::new(config(), [3, 2], RWType::READ, HotSourceOperation::MovePeer)
            .unwrap();
    let d = detail(1, [100., 100., 100., 80.], [100., 100., 100., 80.], "");
    assert_eq!(
        s.destination_store_decision(&d, None),
        HotDestinationDecision::Eligible
    );
    println!(
        "4 history modes, scope membership/override, malformed history panic, invalid dimensions, CPU priority"
    );
}
fn region() -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if id == 3 {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        })
        .collect();
    Arc::new(RegionInfo::new(
        RegionMeta {
            id: 10,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ))
}
fn manager() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(10),
    )
}
fn ids(v: Vec<&StoreLoadDetail>) -> Vec<u64> {
    let mut ids: Vec<_> = v
        .iter()
        .map(|d| d.store_summary_info.store_info.id())
        .collect();
    ids.sort_unstable();
    ids
}
#[tokio::test]
async fn task_090_candidate_actions_and_empty() {
    let conf = ProviderSnapshot::<()>::default();
    let details: Vec<_> = (1..=4)
        .map(|id| detail(id, [80.; 4], [80.; 4], ""))
        .collect();
    let stores: Vec<_> = details
        .iter()
        .map(|d| Arc::new(d.store_summary_info.store_info.clone()))
        .collect();
    let mut manager = manager();
    let mut ctx = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: region(),
        now: 0,
    };
    for (write, moving, leader, want) in [
        (false, true, true, vec![]),
        (false, true, false, vec![4]),
        (true, true, true, vec![4]),
        (false, false, false, vec![]),
        (false, false, true, vec![2, 3, 4]),
        (true, false, true, vec![2]),
        (true, false, false, vec![]),
    ] {
        let s = selector(config(), write, moving);
        let peer = HotPeerStat {
            store_id: 1,
            is_leader: leader,
            ..Default::default()
        };
        assert_eq!(
            ids(s
                .filter_dst_stores(&details, &stores[0], &peer, &mut ctx, None, None)
                .await),
            want,
            "write={write} move={moving} leader={leader}"
        );
    }
    let s = selector(config(), false, true);
    let peer = HotPeerStat::default();
    assert!(
        s.filter_dst_stores(&[], &stores[0], &peer, &mut ctx, None, None)
            .await
            .is_empty()
    );
    let hot: Vec<_> = (1..=4)
        .map(|id| detail(id, [120.; 4], [120.; 4], ""))
        .collect();
    assert!(
        s.filter_dst_stores(&hot, &stores[0], &peer, &mut ctx, None, None)
            .await
            .is_empty()
    );
    println!("7 action/leader cases, empty statistics and no cool destination");
}
#[tokio::test]
async fn task_090_engines_and_real_store_filters() {
    let conf = ProviderSnapshot::<()>::default();
    let mut details = vec![
        detail(4, [80.; 4], [80.; 4], ""),
        detail(5, [80.; 4], [80.; 4], "tiflash"),
        detail(6, [80.; 4], [80.; 4], "tiflash_compute"),
    ];
    for (write, moving, enabled, want) in [
        (true, true, true, vec![4, 5]),
        (true, true, false, vec![4]),
        (false, true, true, vec![4]),
        (false, false, true, vec![4]),
    ] {
        let mut c = config();
        c.enable_for_tiflash = enabled;
        let s = selector(c, write, moving);
        assert_eq!(
            ids(s
                .pick_dst_stores(&details.iter().collect::<Vec<_>>(), &[], &conf, None)
                .await),
            want
        );
    }
    let mut store = details[0].store_summary_info.store_info.clone();
    store.set_labels(vec![StoreLabel {
        key: "specialUse".into(),
        value: "reserved".into(),
    }]);
    details[0].store_summary_info.store_info = store;
    let stores: Vec<_> = (1..=3)
        .map(|id| {
            Arc::new(
                detail(id, [0.; 4], [0.; 4], "")
                    .store_summary_info
                    .store_info,
            )
        })
        .chain(
            details
                .iter()
                .map(|d| Arc::new(d.store_summary_info.store_info.clone())),
        )
        .collect();
    let mut manager = manager();
    let mut ctx = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: region(),
        now: 0,
    };
    let s = selector(config(), true, true);
    assert_eq!(
        ids(s
            .filter_dst_stores(
                &details,
                &stores[0],
                &HotPeerStat::default(),
                &mut ctx,
                None,
                None
            )
            .await),
        vec![5]
    );
    // A real store heartbeat timeout is rejected before the low load can admit it.
    ctx.now = 100_000_000_000_000;
    assert!(
        s.filter_dst_stores(
            &details,
            &stores[0],
            &HotPeerStat::default(),
            &mut ctx,
            None,
            None
        )
        .await
        .is_empty()
    );
    println!("4 engine/flag combinations, reserved special-use rejection, real store timeout");
}
#[tokio::test]
async fn task_090_revert_real_rules() {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = true;
    let stores: Vec<_> = (1..=4)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                labels: if id == 2 {
                    vec![StoreLabel {
                        key: "b".into(),
                        value: "yes".into(),
                    }]
                } else {
                    vec![]
                },
                ..Default::default()
            }))
        })
        .collect();
    let owned: Vec<_> = stores.iter().map(|s| (**s).clone()).collect();
    let mut manager = manager();
    manager
        .initialize(3, &[], "", false, &conf, &owned)
        .await
        .unwrap();
    manager
        .set_rule(
            Rule {
                group_id: "pd".into(),
                id: "default".into(),
                role: PeerRoleType::Voter,
                count: 2,
                ..Default::default()
            },
            0,
            &conf,
            &owned,
        )
        .await
        .unwrap();
    manager
        .set_rule(
            Rule {
                group_id: "pd".into(),
                id: "b".into(),
                role: PeerRoleType::Voter,
                count: 1,
                label_constraints: vec![LabelConstraint {
                    key: "b".into(),
                    op: LabelConstraintOp::In,
                    values: vec!["yes".into()],
                }],
                ..Default::default()
            },
            0,
            &conf,
            &owned,
        )
        .await
        .unwrap();
    let peers: Vec<_> = (2..=4)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let r = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 20,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[1].clone()),
        vec![],
        vec![],
    ));
    let fit = manager.fit_region(&stores, &r, &conf, 0);
    assert!(fit.is_satisfied());
    let mut ctx = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: r.clone(),
        now: 0,
    };
    let s = selector(config(), true, true);
    assert!(!s.is_revert_region_valid(&mut ctx, &stores[1], &stores[0], Some(fit.clone())));
    assert!(s.is_revert_region_valid(&mut ctx, &stores[2], &stores[0], Some(fit.clone())));
    // Forward replacement also reuses the real fit and rejects removing the sole b replica.
    let d = [detail(1, [80.; 4], [80.; 4], "")];
    assert!(
        s.filter_dst_stores(
            &d,
            &stores[1],
            &HotPeerStat::default(),
            &mut ctx,
            Some(fit.clone()),
            None
        )
        .await
        .is_empty()
    );
    assert_eq!(
        ids(s
            .filter_dst_stores(
                &d,
                &stores[2],
                &HotPeerStat::default(),
                &mut ctx,
                Some(fit.clone()),
                None
            )
            .await),
        vec![1]
    );
    // Leader reverse transfer requires an existing destination peer for write,
    // whereas read may move the leader to a new store.
    for (write, want) in [(true, false), (false, true)] {
        let s = selector(config(), write, false);
        assert_eq!(
            s.is_revert_region_valid(&mut ctx, &stores[2], &stores[0], Some(fit.clone())),
            want
        );
    }
    for moving in [true, false] {
        let s = selector(config(), true, moving);
        assert_eq!(s.allow_revert_region(&r, 1), moving);
        assert_eq!(s.allow_revert_region(&r, 2), !moving);
    }
    println!(
        "2 reverse peer rules, 2 forward rules, 2 leader safeguard modes, 4 reverse membership cases"
    );
}

#[tokio::test]
async fn task_090_eviction_scope_and_recovery() {
    use astersql_pd_core_constant::Direction;
    let mut conf = ProviderSnapshot::<()>::default();
    conf.schedule.max_pending_peer_count = 64;
    let mut details: Vec<_> = (1..=4)
        .map(|id| detail(id, [80.; 4], [80.; 4], ""))
        .collect();
    let stores: Vec<_> = details
        .iter()
        .map(|d| Arc::new(d.store_summary_info.store_info.clone()))
        .collect();
    let mut manager = manager();
    let mut ctx = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: region(),
        now: 0,
    };
    let moving = selector(config(), true, true);
    for mode in 0..3 {
        let store = &mut details[3].store_summary_info.store_info;
        match mode {
            0 => store.slow_store_evicted(),
            1 => store.stopping_store_evicted(),
            _ => store.slow_trend_evicted(),
        }
        assert!(
            moving
                .filter_dst_stores(
                    &details,
                    &stores[0],
                    &HotPeerStat::default(),
                    &mut ctx,
                    None,
                    None
                )
                .await
                .is_empty()
        );
        let store = &mut details[3].store_summary_info.store_info;
        match mode {
            0 => store.slow_store_recovered(),
            1 => store.stopping_store_recovered(),
            _ => store.slow_trend_recovered(),
        }
        assert_eq!(
            ids(moving
                .filter_dst_stores(
                    &details,
                    &stores[0],
                    &HotPeerStat::default(),
                    &mut ctx,
                    None,
                    None
                )
                .await),
            vec![4]
        );
    }
    let expect = StoreLoad {
        loads: [50.; 4],
        ..Default::default()
    };
    let scope = HotDestinationScope {
        store_ids: &[4],
        expect: &expect,
    };
    assert!(
        moving
            .filter_dst_stores(
                &details,
                &stores[0],
                &HotPeerStat::default(),
                &mut ctx,
                None,
                Some(&scope)
            )
            .await
            .is_empty()
    );
    let expect = StoreLoad {
        loads: [200.; 4],
        ..Default::default()
    };
    let scope = HotDestinationScope {
        store_ids: &[4],
        expect: &expect,
    };
    assert_eq!(
        ids(moving
            .filter_dst_stores(
                &details,
                &stores[0],
                &HotPeerStat::default(),
                &mut ctx,
                None,
                Some(&scope)
            )
            .await),
        vec![4]
    );
    let scope = HotDestinationScope {
        store_ids: &[1, 2, 3],
        expect: &expect,
    };
    assert!(
        moving
            .filter_dst_stores(
                &details,
                &stores[0],
                &HotPeerStat::default(),
                &mut ctx,
                None,
                Some(&scope)
            )
            .await
            .is_empty()
    );
    // Read leader transfer only adds move-region checks for stores without a peer.
    details[1]
        .store_summary_info
        .store_info
        .set_pending_peer_count(10000);
    details[3]
        .store_summary_info
        .store_info
        .set_pending_peer_count(10000);
    let leader = selector(config(), false, false);
    let peer = HotPeerStat {
        store_id: 1,
        is_leader: true,
        ..Default::default()
    };
    assert_eq!(
        ids(leader
            .filter_dst_stores(&details, &stores[0], &peer, &mut ctx, None, None)
            .await),
        vec![2, 3]
    );
    details[1]
        .store_summary_info
        .store_info
        .pause_leader_transfer(Direction::IN);
    assert_eq!(
        ids(leader
            .filter_dst_stores(&details, &stores[0], &peer, &mut ctx, None, None)
            .await),
        vec![3]
    );
    details[1]
        .store_summary_info
        .store_info
        .resume_leader_transfer(Direction::IN);
    details[3]
        .store_summary_info
        .store_info
        .set_pending_peer_count(0);
    details[3]
        .store_summary_info
        .store_info
        .set_labels(vec![StoreLabel {
            key: "specialUse".into(),
            value: "hotRegion".into(),
        }]);
    assert_eq!(
        ids(leader
            .filter_dst_stores(&details, &stores[0], &peer, &mut ctx, None, None)
            .await),
        vec![2, 3, 4]
    );
    println!(
        "3 eviction/recovery pairs, 3 scope outcomes, follower versus move-leader state, pause/recovery and allowed hotRegion use"
    );
}

#[tokio::test]
async fn task_090_location_and_witness_safeguards() {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.location_labels = vec!["zone".into()].into();
    let details: Vec<_> = (1..=5)
        .map(|id| {
            let mut d = detail(id, [80.; 4], [80.; 4], "");
            d.store_summary_info.store_info.set_labels(vec![StoreLabel {
                key: "zone".into(),
                value: match id {
                    1 | 5 => "a",
                    2 | 4 => "b",
                    _ => "c",
                }
                .into(),
            }]);
            d
        })
        .collect();
    let stores: Vec<_> = details
        .iter()
        .map(|d| Arc::new(d.store_summary_info.store_info.clone()))
        .collect();
    let mut manager = manager();
    let mut ctx = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: region(),
        now: 0,
    };
    let s = selector(config(), true, true);
    assert_eq!(
        ids(s
            .filter_dst_stores(
                &details,
                &stores[0],
                &HotPeerStat::default(),
                &mut ctx,
                None,
                None
            )
            .await),
        vec![5]
    );
    // With rules enabled a witness voter cannot become a leader even when it
    // belongs to the same fit. Use real fitting and guard construction.
    drop(ctx);
    conf.replication.enable_placement_rules = true;
    manager
        .initialize(
            3,
            &[],
            "",
            false,
            &conf,
            &stores.iter().map(|s| (**s).clone()).collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            is_witness: id == 2,
            ..Default::default()
        })
        .collect();
    let r = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 50,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let fit = manager.fit_region(&stores, &r, &conf, 0);
    let mut ctx = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: r,
        now: 0,
    };
    let s = selector(config(), true, false);
    assert!(!s.is_revert_region_valid(&mut ctx, &stores[0], &stores[1], Some(fit.clone())));
    assert!(s.is_revert_region_valid(&mut ctx, &stores[0], &stores[2], Some(fit.clone())));
    let peer = HotPeerStat {
        store_id: 1,
        is_leader: true,
        ..Default::default()
    };
    assert_eq!(
        ids(s
            .filter_dst_stores(&details, &stores[0], &peer, &mut ctx, Some(fit), None)
            .await),
        vec![3]
    );
    println!(
        "legacy diversity rejection/preservation and real rule witness/normal leader safeguards"
    );
}
