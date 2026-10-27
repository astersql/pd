// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_statistics::*;
use astersql_pd_statistics_utils::RWType;
use serde_json::Value;
use std::collections::HashSet;
fn config() -> HotRegionParams {
    HotRegionParams {
        src_tolerance_ratio: 1.1,
        enable_for_tiflash: true,
        ..Default::default()
    }
}
fn selector(c: HotRegionParams, write: bool, moving: bool) -> HotSourceSelector {
    HotSourceSelector::new(
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
fn detail(current: Loads, future: Loads, engine: &str) -> StoreLoadDetail {
    StoreLoadDetail {
        store_summary_info: StoreSummaryInfo {
            store_info: StoreInfo::new(StoreMeta {
                id: 1,
                labels: vec![StoreLabel {
                    key: "engine".into(),
                    value: engine.into(),
                }],
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
        hot_peers: vec![HotPeerStat::default()],
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_089.json"
    ))
    .unwrap()
}
#[test]
fn task_089_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    for row in f["source_cases"].as_array().unwrap() {
        let b = |i: usize| row[i].as_bool().unwrap();
        let loads = |i: usize| std::array::from_fn(|d| row[i][d].as_f64().unwrap());
        let mut c = config();
        c.rank_formula_version = if b(6) { "v1" } else { "v2" }.into();
        c.strict_picking_store = b(7);
        let s = selector(c, b(4), b(5));
        let d = detail(loads(1), loads(2), row[3].as_str().unwrap());
        let stores = [d];
        let result = s.filter_src_stores(&stores, b(8), [b(9), b(9)]);
        assert_eq!(result[0].1.label(), row[10].as_str().unwrap(), "{}", row[0]);
    }
    println!("13 frozen source cases");
}
#[test]
fn task_089_peer_sort_and_pending() {
    let f = fixture();
    let peers: Vec<_> = f["peers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| HotPeerStat {
            region_id: r[0].as_u64().unwrap(),
            loads: (1..5).map(|i| r[i].as_f64().unwrap()).collect(),
            ..Default::default()
        })
        .collect();
    for row in f["peer_cases"].as_array().unwrap() {
        let mut c = config();
        c.max_peer_number = row[0].as_i64().unwrap();
        let s = HotSourceSelector::new(c, [2, 0], RWType::READ, HotSourceOperation::TransferLeader)
            .unwrap();
        let r = s.filter_hot_peers(&peers, &HashSet::new(), 0, 3, 2);
        let mut ids: Vec<_> = r.peers.iter().map(|p| p.region_id).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            row[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(r.nth.map(|p| p.unwrap().region_id), [3, 3]);
    }
    let mut c = config();
    c.max_peer_number = 1;
    let s = HotSourceSelector::new(c, [2, 0], RWType::READ, HotSourceOperation::TransferLeader)
        .unwrap();
    let r = s.filter_hot_peers(&peers, &HashSet::from([1]), 0, 3, 2);
    assert!(r.peers.is_empty()); // cut off before rejecting pending; no refill
    assert_eq!(r.nth[0].unwrap().region_id, 3);
    println!("4 frozen peer caps and cutoff-before-pending case");
}
#[test]
fn task_089_history_scope_and_errors() {
    let s = selector(config(), false, true);
    let mut d = detail([130.; 4], [130.; 4], "");
    d.load_pred.current.history_loads = [vec![130., 110.], vec![130., 110.], vec![], vec![]];
    d.load_pred.expect.history_loads = [vec![100.; 2], vec![100.; 2], vec![], vec![]];
    assert_eq!(
        s.source_store_decision(&d, None),
        HotSourceDecision::HistoryFailed
    );
    d.load_pred.current.history_loads[1] = vec![130.; 2];
    assert_eq!(
        s.source_store_decision(&d, None),
        HotSourceDecision::Eligible
    );
    let mut scoped = StoreLoad {
        loads: [200.; 4],
        ..Default::default()
    };
    assert!(std::ptr::eq(s.expect_load(&d, Some(&scoped)), &scoped));
    assert_eq!(
        s.source_store_decision(&d, Some(&scoped)),
        HotSourceDecision::LoadFailed
    );
    scoped.loads = [50.; 4];
    scoped.history_loads = [vec![50.; 2], vec![50.; 2], vec![], vec![]];
    assert_eq!(
        s.source_store_decision(&d, Some(&scoped)),
        HotSourceDecision::Eligible
    );
    d.load_pred.expect.history_loads[0].clear();
    assert!(std::panic::catch_unwind(|| s.source_store_decision(&d, None)).is_err());
    for dims in [[0, 0], [0, 4], [5, 2]] {
        assert!(matches!(
            HotSourceSelector::new(config(), dims, RWType::READ, HotSourceOperation::MovePeer),
            Err(HotSourceError::InvalidPriorities)
        ));
    }
    let mut c = config();
    c.max_peer_number = -1;
    assert!(matches!(
        HotSourceSelector::new(c, [0, 1], RWType::READ, HotSourceOperation::MovePeer),
        Err(HotSourceError::InvalidPeerLimit)
    ));
    let mut d = detail([130.; 4], [130.; 4], "");
    d.hot_peers.clear();
    assert_eq!(
        s.filter_src_stores(&[d], true, [true; 2])[0].1,
        HotSourceDecision::Ineligible
    );
    let mut c = config();
    c.enable_for_tiflash = false;
    assert_eq!(
        selector(c, true, true).filter_src_stores(
            &[detail([130.; 4], [130.; 4], "tiflash")],
            false,
            [false; 2]
        )[0]
        .1,
        HotSourceDecision::Ineligible
    );
    println!(
        "history strict threshold, any priority, scoped override, malformed history and invalid inputs"
    );
}
#[test]
fn task_089_cooldown_topn_and_pointer_union() {
    let mut peers = vec![
        HotPeerStat {
            region_id: 1,
            loads: vec![100.; 4],
            last_transfer_leader_time: Some(0),
            ..Default::default()
        },
        HotPeerStat {
            region_id: 2,
            loads: vec![1.; 4],
            ..Default::default()
        },
    ];
    let mut c = config();
    c.max_peer_number = 1;
    let s = selector(c, false, true);
    let r = s.filter_hot_peers(&peers, &HashSet::new(), 29_999_999_999, 3, 1);
    assert!(r.peers.is_empty());
    assert_eq!(r.nth[0].unwrap().region_id, 1);
    assert_eq!(
        s.filter_hot_peers(&peers, &HashSet::new(), 30_000_000_000, 3, 1)
            .peers[0]
            .region_id,
        1
    );
    peers[0].last_transfer_leader_time = Some(40_000_000_000);
    assert!(
        s.filter_hot_peers(&peers, &HashSet::new(), 30_000_000_000, 3, 10)
            .peers
            .is_empty()
    );
    assert!(
        s.filter_hot_peers(&peers, &HashSet::new(), 0, 3, 10)
            .nth
            .iter()
            .all(Option::is_none)
    );
    // Distinct pointer identities with equal IDs must both survive the union.
    peers[1].region_id = 1;
    let a = &peers[0];
    let b = &peers[1];
    let r = HotSourceSelector::sort_hot_peers(&[a, a, b], &[a, b], 8);
    assert_eq!(r.len(), 2);
    assert!(std::ptr::eq(r[0], a));
    assert!(std::ptr::eq(r[1], b));
    assert_eq!(HotSourceSelector::sort_hot_peers(&[], &[a, b], 1).len(), 1);
    assert!(HotSourceSelector::sort_hot_peers(&[a], &[b], 0).is_empty());
    // CPU priority from existing Go TestHotCacheSortHotPeer.
    let p = vec![
        HotPeerStat {
            region_id: 1,
            loads: vec![1., 0., 1., 30.],
            ..Default::default()
        },
        HotPeerStat {
            region_id: 2,
            loads: vec![1., 0., 100., 5.],
            ..Default::default()
        },
    ];
    let mut c = config();
    c.max_peer_number = 1;
    let s = HotSourceSelector::new(c, [3, 0], RWType::READ, HotSourceOperation::TransferLeader)
        .unwrap();
    assert_eq!(
        s.filter_hot_peers(&p, &HashSet::new(), 0, 3, 10).peers[0].region_id,
        1
    );
}
use astersql_pd_core::{DownPeer, Peer, PeerRole, RegionInfo, RegionMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{cell::RefCell, sync::Arc};
fn region(n: u64, learner: bool, pending: bool, down: bool) -> RegionInfo {
    let peers: Vec<_> = (1..=n)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if learner && id == 1 {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        })
        .collect();
    RegionInfo::new(
        RegionMeta {
            id: 10,
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        if down {
            vec![DownPeer {
                peer: peers[0].clone(),
                down_seconds: 0,
            }]
        } else {
            vec![]
        },
        if pending {
            vec![peers[0].clone()]
        } else {
            vec![]
        },
    )
}
#[tokio::test]
async fn task_089_real_region_placement_and_stale() {
    let stores: Vec<_> = (1..=4)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }))
        })
        .collect();
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(10),
    );
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
    for rules in [false, true] {
        conf.replication.enable_placement_rules = rules;
        let context = RegionReplicationContext {
            conf: &conf,
            stores: &stores,
            manager: RefCell::new(&mut manager),
            now: 0,
        };
        let s = selector(config(), false, true);
        assert!(matches!(
            s.get_region(None, 1, &context),
            Err(HotSourceError::MissingRegion)
        ));
        for (n, learner, pending, down, want) in [
            (3, false, false, false, None),
            (3, false, true, false, None),
            (3, false, false, true, Some(HotSourceError::UnhealthyRegion)),
            (
                2,
                false,
                false,
                false,
                Some(HotSourceError::AbnormalReplicas),
            ),
            (
                3,
                true,
                false,
                false,
                Some(HotSourceError::AbnormalReplicas),
            ),
            (
                4,
                false,
                false,
                false,
                Some(HotSourceError::AbnormalReplicas),
            ),
        ] {
            let r = region(n, learner, pending, down);
            let result = s.get_region(Some(&r), 1, &context);
            assert_eq!(
                result.as_ref().err().copied(),
                want,
                "rules={rules} n={n} learner={learner} pending={pending} down={down}"
            );
            if let Ok((out, fit)) = result {
                assert!(std::ptr::eq(out, &r));
                assert_eq!(fit.is_some(), rules);
                if let Some(fit) = fit {
                    assert!(fit.is_satisfied());
                }
            }
        }
        let r = region(3, false, false, false);
        assert!(matches!(
            s.get_region(Some(&r), 4, &context),
            Err(HotSourceError::StalePeer)
        ));
        let s = selector(config(), false, false);
        assert!(matches!(
            s.get_region(Some(&r), 2, &context),
            Err(HotSourceError::StaleLeader)
        ));
        assert!(s.get_region(Some(&r), 1, &context).is_ok());
    }
    println!("2 placement modes x (missing + 6 health/replica + 3 stale/leader cases) = 20 cases");
}
#[test]
fn task_089_zombie_provenance() {
    for (write, moving, engine, first, region_stats) in [
        (false, true, "", 0, false),
        (false, false, "", 0, false),
        (true, true, "", 0, false),
        (true, true, "tiflash", 0, true),
        (true, false, "", 1, true),
        (true, false, "", 2, false),
    ] {
        let c = config();
        let expect = if region_stats {
            c.region_stat_zombie_duration()
        } else {
            c.store_stat_zombie_duration()
        };
        let s = HotSourceSelector::new(
            c,
            [first, 3],
            if write { RWType::WRITE } else { RWType::READ },
            if moving {
                HotSourceOperation::MovePeer
            } else {
                HotSourceOperation::TransferLeader
            },
        )
        .unwrap();
        assert_eq!(
            s.max_zombie_duration(&detail([0.; 4], [0.; 4], engine)),
            expect
        );
    }
    println!("6 Go TestMaxZombieDuration source cases");
}

#[test]
fn task_089_priority_history_modes_and_engine_scopes() {
    let mut d = detail([130.; 4], [130.; 4], "");
    d.load_pred.current.history_loads = [vec![130.], vec![100.], vec![], vec![]];
    d.load_pred.expect.history_loads = [vec![100.], vec![100.], vec![], vec![]];
    for (version, strict, write, moving, want) in [
        ("v1", true, false, true, HotSourceDecision::HistoryFailed),
        ("v1", false, false, true, HotSourceDecision::Eligible),
        ("v2", true, false, true, HotSourceDecision::Eligible),
        ("v1", true, true, false, HotSourceDecision::Eligible),
    ] {
        let mut c = config();
        c.rank_formula_version = version.into();
        c.strict_picking_store = strict;
        assert_eq!(
            selector(c, write, moving).source_store_decision(&d, None),
            want
        );
    }
    let stores = [
        detail([50.; 4], [50.; 4], ""),
        detail([50.; 4], [50.; 4], "tiflash"),
    ];
    let s = selector(config(), true, true);
    let r = s.filter_src_stores(&stores, true, [false, true]);
    assert_eq!(r[0].1, HotSourceDecision::LoadFailed);
    assert_eq!(r[1].1, HotSourceDecision::Deferred);
    assert!(!r[0].1.admitted());
    assert!(r[1].1.admitted());
    assert!(HotSourceDecision::Eligible.admitted());
    assert!(!HotSourceDecision::HistoryFailed.admitted());
    let mut d = detail([0., 0., 0., 130.], [0., 0., 0., 130.], "");
    let s = HotSourceSelector::new(config(), [3, 2], RWType::READ, HotSourceOperation::MovePeer)
        .unwrap();
    assert_eq!(
        s.source_store_decision(&d, None),
        HotSourceDecision::Eligible
    );
    d.load_pred.current.loads[3] = f64::NAN;
    assert_eq!(
        s.source_store_decision(&d, None),
        HotSourceDecision::LoadFailed
    );
    println!("4 priority history modes, engine-specific scope admission, CPU and NaN safeguards");
}
