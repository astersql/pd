// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_schedule_placement::{LabelConstraint, LabelConstraintOp, Rule};
use astersql_pd_statistics::*;
use serde_json::Value;
fn rule(pool: &str) -> Rule {
    Rule {
        label_constraints: vec![
            LabelConstraint {
                key: "pool".into(),
                op: LabelConstraintOp::In,
                values: vec![pool.into()],
            },
            LabelConstraint {
                key: "engine".into(),
                op: LabelConstraintOp::NotIn,
                values: vec!["other".into()],
            },
        ],
        ..Default::default()
    }
}
fn details() -> Vec<StoreLoadDetail> {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_091.json"
    ))
    .unwrap();
    f["stores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut labels = vec![StoreLabel {
                key: "pool".into(),
                value: r[2].as_str().unwrap().into(),
            }];
            if r[3] != "" {
                labels.push(StoreLabel {
                    key: "engine".into(),
                    value: r[3].as_str().unwrap().into(),
                });
            }
            let load = StoreLoad {
                loads: [r[1].as_f64().unwrap(), 10., 0., 0.],
                hot_peer_count: 1.,
                history_loads: [vec![10.], vec![10.], vec![], vec![]],
            };
            StoreLoadDetail {
                store_summary_info: StoreSummaryInfo {
                    store_info: StoreInfo::new(StoreMeta {
                        id: r[0].as_u64().unwrap(),
                        labels,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                load_pred: StoreLoadPred {
                    current: load.clone(),
                    future: load,
                    ..Default::default()
                },
                hot_peers: vec![],
            }
        })
        .collect()
}
#[test]
fn task_091_behavior_cases() {
    let mut round = HotPlacementLoads::new(details()).unwrap();
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_091.json"
    ))
    .unwrap();
    for row in f["cases"].as_array().unwrap() {
        let scope = round.scope(
            &[&rule(row[0].as_str().unwrap())],
            row[1].as_bool().unwrap(),
        );
        if row[3].is_null() {
            assert!(scope.is_none());
            continue;
        }
        let s = scope.unwrap();
        assert_eq!(
            s.store_ids(),
            row[2]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(s.expect.loads[0], row[3].as_f64().unwrap());
        assert_eq!(s.expect.history_loads[0], vec![10.]);
        assert_eq!(s.expect.hot_peer_count, 1.);
    }
    let s = round.scope(&[&rule("target")], true).unwrap();
    assert!((s.stddev.loads[0] - (0.125_f64).sqrt() / 10.).abs() < 1e-9);
    assert!(s.contains(1));
    assert!(!s.contains(5));
    assert!(!s.contains(999));
    println!("5 frozen source-derived population/load cases");
}

#[test]
fn task_091_cache_and_snapshot_faults() {
    use std::sync::Arc;
    let mut r = HotPlacementLoads::new(details()).unwrap();
    let a = r.scope(&[&rule("target")], true).unwrap();
    let mut equivalent = rule("target");
    equivalent.id = "reallocated".into();
    equivalent.start_key = b"a".to_vec();
    assert!(Arc::ptr_eq(&a, &r.scope(&[&equivalent], true).unwrap()));
    equivalent.label_constraints[0].values.push("unused".into());
    let b = r.scope(&[&equivalent], true).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert!(same_hot_placement_population(
        Some(&a),
        true,
        Some(&b),
        true
    ));
    let other = r.scope(&[&rule("other")], true).unwrap();
    assert!(!same_hot_placement_population(
        Some(&a),
        true,
        Some(&other),
        true
    ));
    assert!(!same_hot_placement_population(
        Some(&a),
        true,
        Some(&a),
        false
    ));
    assert!(same_hot_placement_population(None, true, None, true));
    assert!(!same_hot_placement_population(None, true, None, false));
    assert!(!same_hot_placement_population(Some(&a), true, None, true));
    for _ in 0..2 {
        assert!(r.scope(&[&Rule::default()], true).is_none());
    }
    let n = r.cached_scopes();
    assert!(r.scope(&[&Rule::default()], true).is_none());
    assert_eq!(n, r.cached_scopes());
    assert!(r.scope(&[], true).is_none());
    assert!(r.scope(&[&rule("target"), &rule("other")], true).is_none());
    let mut changed = details();
    changed[0].load_pred.current.loads[0] = 50.5;
    let mut new = HotPlacementLoads::new(changed).unwrap();
    assert_eq!(
        new.scope(&[&rule("target")], true).unwrap().expect.loads[0],
        20.
    );
    assert_eq!(a.expect.loads[0], 10.);
    let mut duplicate = details();
    duplicate.push(duplicate[0].clone());
    assert!(matches!(
        HotPlacementLoads::new(duplicate),
        Err(HotPlacementError::DuplicateStore(1))
    ));
    assert!(
        HotPlacementLoads::new(vec![])
            .unwrap()
            .scope(&[&rule("target")], true)
            .is_none()
    );
    // More than one word; omitted old store and newly added IDs cannot alias.
    let mut many = vec![];
    for id in 1..=130 {
        let mut d = details()[0].clone();
        d.store_summary_info.store_info = StoreInfo::new(StoreMeta {
            id,
            labels: vec![StoreLabel {
                key: "pool".into(),
                value: if id == 130 { "other" } else { "target" }.into(),
            }],
            ..Default::default()
        });
        many.push(d);
    }
    let mut many = HotPlacementLoads::new(many).unwrap();
    let s = many.scope(&[&rule("target")], true).unwrap();
    assert_eq!(s.population_words().len(), 3);
    assert!(s.contains(129));
    assert!(!s.contains(130));
    assert_eq!(s.population_words()[2], 1);
    // Exact byte lengths avoid delimiter and Unicode key collisions.
    let mut unicode = rule("值:é");
    unicode.label_constraints[0].key = "池:".into();
    assert_eq!(
        PlacementScopeKey::new(&[&unicode], true).rule_scope,
        "1:2:4:池:2:in1:6:值:é6:engine5:notIn1:5:other"
    );
}

#[test]
fn task_091_scoped_destination_diagnostic() {
    let mut ds = details();
    // Deliberately malformed foreign history must never be accessed.
    ds[4].load_pred.current.history_loads[0] = vec![1., 2.];
    let mut round = HotPlacementLoads::new(ds.clone()).unwrap();
    let scope = round.scope(&[&rule("target")], true).unwrap();
    let selector = HotDestinationSelector::new(
        HotRegionParams::default(),
        [0, 1],
        astersql_pd_statistics_utils::RWType::READ,
        HotSourceOperation::MovePeer,
    )
    .unwrap();
    assert_eq!(
        selector
            .destination_store_decision(&ds[4], Some(&scope.destination_scope()))
            .label(),
        "dst-store-out-of-scope"
    );
}

#[tokio::test]
async fn task_091_real_range_fit_and_stale_labels() {
    use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
    use astersql_pd_schedule_placement::{PeerRoleType, RuleManager};
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::sync::Arc;
    let ds = details();
    let owned: Vec<_> = ds
        .iter()
        .map(|d| d.store_summary_info.store_info.clone())
        .collect();
    let conf = astersql_pd_schedule_config::ProviderSnapshot::<()>::default();
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let mut manager = RuleManager::new(storage, astersql_pd_keypath::ClusterPaths::new(91));
    manager
        .initialize(1, &[], "", false, &conf, &owned)
        .await
        .unwrap();
    let mut a = rule("target");
    a.group_id = "pd".into();
    a.id = "default".into();
    a.count = 1;
    a.role = PeerRoleType::Voter;
    manager.set_rule(a, 0, &conf, &owned).await.unwrap();
    let mut b = rule("other");
    b.group_id = "pd".into();
    b.id = "range".into();
    b.index = 10;
    b.override_ = true;
    b.start_key = b"m".to_vec();
    b.start_key_hex = "6d".into();
    b.count = 1;
    b.role = PeerRoleType::Voter;
    manager.set_rule(b, 0, &conf, &owned).await.unwrap();
    let ds = details();
    let stores: Vec<_> = ds
        .iter()
        .map(|d| Arc::new(d.store_summary_info.store_info.clone()))
        .collect();
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: astersql_pd_core::KeyRange {
                start_key: vec![],
                end_key: b"m".to_vec(),
            },
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    let fit = manager.fit_region(&stores, &region, &conf, 0);
    let mut loads = HotPlacementLoads::new(ds.clone()).unwrap();
    assert_eq!(
        loads
            .prepare_for_region(&region, Some(&fit), &ds[0], HotSourceOperation::MovePeer)
            .unwrap()
            .expect
            .loads[0],
        10.
    );
    assert!(
        loads
            .prepare_for_region(&region, None, &ds[0], HotSourceOperation::MovePeer)
            .is_none()
    );
    assert!(
        loads
            .prepare_for_region(&region, Some(&fit), &ds[4], HotSourceOperation::MovePeer)
            .is_none()
    );
    let mut stale = ds[0].clone();
    stale.store_summary_info.store_info = ds[4].store_summary_info.store_info.clone();
    // Same source ID, labels from a newer snapshot disagree with the fit.
    stale.store_summary_info.store_info = StoreInfo::new(StoreMeta {
        id: 1,
        labels: vec![StoreLabel {
            key: "pool".into(),
            value: "other".into(),
        }],
        ..Default::default()
    });
    assert!(
        loads
            .prepare_for_region(
                &region,
                Some(&fit),
                &stale,
                HotSourceOperation::TransferLeader
            )
            .is_none()
    );
    let peer = Peer {
        id: 5,
        store_id: 5,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            id: 2,
            key_range: astersql_pd_core::KeyRange {
                start_key: b"m".to_vec(),
                end_key: vec![],
            },
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    let fit = manager.fit_region(&stores, &region, &conf, 0);
    assert_eq!(
        loads
            .prepare_for_region(
                &region,
                Some(&fit),
                &ds[4],
                HotSourceOperation::TransferLeader
            )
            .unwrap()
            .expect
            .loads[0],
        100.
    );
}

#[test]
fn task_091_leader_union_excludes_witness() {
    use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
    use astersql_pd_schedule_placement::{PeerRoleType, fit_region};
    use std::sync::Arc;
    let ds = details();
    let stores: Vec<_> = ds
        .iter()
        .map(|d| Arc::new(d.store_summary_info.store_info.clone()))
        .collect();
    let peers: Vec<_> = [1, 5, 6]
        .iter()
        .map(|&id| Peer {
            id,
            store_id: id,
            is_witness: id == 6,
            ..Default::default()
        })
        .collect();
    let region = RegionInfo::new(
        RegionMeta {
            id: 91,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    let rules: Vec<_> = [("target", false), ("other", false), ("other", true)]
        .into_iter()
        .map(|(pool, witness)| {
            let mut r = rule(pool);
            r.role = PeerRoleType::Voter;
            r.count = 1;
            r.is_witness = witness;
            Arc::new(r)
        })
        .collect();
    let fit = fit_region(&stores, &region, &rules, true);
    let mut round = HotPlacementLoads::new(ds.clone()).unwrap();
    assert_eq!(
        round
            .prepare_for_region(&region, Some(&fit), &ds[0], HotSourceOperation::MovePeer)
            .unwrap()
            .expect
            .loads[0],
        10.
    );
    // Leader's union includes both voter pools, thus global engine population.
    assert!(
        round
            .prepare_for_region(
                &region,
                Some(&fit),
                &ds[0],
                HotSourceOperation::TransferLeader
            )
            .is_none()
    );
    assert!(
        round
            .prepare_for_region(
                &region,
                Some(&fit),
                &ds[5],
                HotSourceOperation::TransferLeader
            )
            .is_none()
    );
    // With only the other pool assigned to witness rules the leader scope remains target.
    let fit = fit_region(
        &stores,
        &region,
        &[rules[0].clone(), rules[2].clone()],
        true,
    );
    assert_eq!(
        round
            .prepare_for_region(
                &region,
                Some(&fit),
                &ds[0],
                HotSourceOperation::TransferLeader
            )
            .unwrap()
            .store_ids(),
        &[1, 2, 3, 4]
    );
}
