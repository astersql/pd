// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster as CoreCluster, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, RegionRole,
    StoreInfo, StoreMeta, StoreStats,
};
use astersql_pd_schedule_core::BasicCluster;
use serde_json::Value;
use std::sync::Arc;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_068.json"
    ))
    .unwrap()
}
fn store(id: u64, used: u64) -> Arc<StoreInfo> {
    let mut s = StoreInfo::new(StoreMeta {
        id,
        ..Default::default()
    });
    s.set_region_size(1000);
    s.set_region_count(99);
    s.set_learner_count(77);
    s.set_witness_count(88);
    s.set_new_store_stats(StoreStats {
        capacity: 200 << 20,
        available: 100 << 20,
        used_size: used,
        ..Default::default()
    });
    Arc::new(s)
}
fn region(id: u64, start: &[u8], end: &[u8], size: i64, leader: u64) -> Arc<RegionInfo> {
    let peers = vec![
        Peer {
            id: id * 10 + 1,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: id * 10 + 2,
            store_id: 2,
            ..Default::default()
        },
        Peer {
            id: id * 10 + 3,
            store_id: 3,
            role: if id == 1 {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            is_witness: id == 2,
        },
    ];
    let lead = peers.iter().find(|p| p.store_id == leader).cloned();
    let pending = if id == 2 {
        vec![peers[0].clone()]
    } else {
        vec![]
    };
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.to_vec(),
                end_key: end.to_vec(),
            },
            peers,
            ..Default::default()
        },
        lead,
        vec![],
        pending,
    );
    r.stats_mut().approximate_size = size;
    Arc::new(r)
}
fn cluster() -> CoreCluster {
    let mut c = CoreCluster::new();
    for id in 1..=3 {
        c.stores.put_store(store(id, 100 << 20));
    }
    for r in [
        region(1, b"a", b"c", 10, 1),
        region(2, b"c", b"e", 20, 2),
        region(3, b"e", b"", 70, 1),
    ] {
        let (o, v, b) = c.regions.set_region(r.clone());
        c.regions.update_sub_tree(r, o.as_deref(), &v, b);
    }
    c
}
#[test]
fn task_068_behavior_cases() {
    let f = fixture();
    let mut q = RetryQuota::default();
    for e in f["quota"].as_array().unwrap() {
        assert_eq!(q.get_limit(1), e.as_u64().unwrap() as usize);
        q.attenuate(1);
    }
    q.attenuate(2);
    assert_eq!(q.get_limit(2), 5);
    q.gc(&[1, 1]);
    assert_eq!(q.get_limit(1), 1);
    assert_eq!(q.get_limit(2), 10);
    q.reset_limit(1);
    assert_eq!(q.get_limit(1), 10);
    q.gc(&[]);
    assert_eq!(q.tracked_stores(), 0);
    for c in f["ranges"].as_array().unwrap() {
        let args: Vec<_> = c["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let actual = get_key_ranges(&args);
        if c["error"] == true {
            assert!(actual.is_err(), "{c}");
            continue;
        }
        let actual = actual.unwrap();
        let expected = c["expected"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected) {
            for (k, b) in [&a.start_key, &a.end_key].iter().zip(e.as_array().unwrap()) {
                assert_eq!(
                    **k,
                    b.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u8)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    let error = get_key_ranges(&["a", "x%q0"]).unwrap_err();
    assert_eq!((error.argument, error.offset), (1, 1));
    println!("6 quota progression cases + 11 parser cases passed");
}
#[test]
fn task_068_range_projection() {
    let c = cluster();
    let mut view = RangeCluster::new(&c, b"b", b"d");
    let s = view.get_store(1).unwrap();
    assert_eq!(
        (
            s.leader_count(),
            s.region_count(),
            s.pending_peer_count(),
            s.leader_size(),
            s.region_size()
        ),
        (1, 2, 1, 10, 30)
    );
    assert_eq!((s.used_size(), s.available()), (3 << 20, 197 << 20));
    assert_eq!((s.learner_count(), s.witness_count()), (77, 88));
    assert_eq!(view.average_region_size(), 15);
    assert_eq!(s.region_score("v1", 0.7, 0.9, 0), 30.0);
    assert_eq!(
        c.get_store(1).unwrap().region_score("v1", 0.7, 0.9, 0),
        1000.0
    );
    let config = astersql_pd_schedule_config::ScheduleConfig::default();
    let kind = astersql_pd_core_constant::ScheduleKind {
        resource: astersql_pd_core_constant::ResourceKind::LEADER,
        policy: astersql_pd_core_constant::SchedulePolicy::BY_COUNT,
    };
    assert_eq!(
        BalanceSolver::from_range(kind, &config, &view).tolerant_ratio(),
        5.0
    );
    view.set_tolerant_size_ratio(7.0);
    assert_eq!(
        BalanceSolver::from_range(kind, &config, &view).tolerant_ratio(),
        7.0
    );
    view.set_tolerant_size_ratio(0.0);
    assert_eq!(
        view.local_regions()
            .store_role_count(3, RegionRole::Learner),
        1
    );
    assert_eq!(
        view.local_regions()
            .store_role_count(3, RegionRole::Witness),
        1
    );
    assert_eq!(c.stores.get_store(1).unwrap().used_size(), 100 << 20);
    assert!(view.get_store(99).is_none());
    assert_eq!(view.get_stores().len(), 3);
    let r = c.regions.get_region(2).unwrap();
    assert!(
        view.get_region_stores(&r)
            .iter()
            .all(|s| s.region_size() == 30)
    );
    assert!(
        view.get_follower_stores(&r)
            .iter()
            .all(|s| s.region_size() == 30)
    );
    assert_eq!(view.get_leader_store(&r).unwrap().leader_size(), 20);
    assert_eq!(view.avg_network_slow_score(1), 0);
    assert_eq!(view.get_tolerant_size_ratio(2.0), 2.0);
    view.set_tolerant_size_ratio(7.0);
    assert_eq!(view.get_tolerant_size_ratio(2.0), 7.0);
    view.set_tolerant_size_ratio(0.0);
    assert_eq!(view.get_tolerant_size_ratio(2.0), 2.0);
    let leaders = view.rand_leader_regions(1, &[], &mut |_| 0, &mut |_| {});
    assert!(!leaders.is_empty());
    assert!(leaders.iter().all(|r| r.meta().id == 1));
    let followers = view.rand_follower_regions(1, &[], &mut |_| 0, &mut |_| {});
    assert!(!followers.is_empty());
    assert!(followers.iter().all(|r| r.meta().id == 2));
    assert_eq!(view.total_region_count(), 3); // Go embedded root reads remain global
    let empty = RangeCluster::new(&c, b"e", b"e");
    assert_eq!(empty.get_store(1).unwrap().region_count(), 0);
    assert_eq!(empty.average_region_size(), 0);
    let zero = store(9, 0);
    assert!(Arc::ptr_eq(&zero, &view.update_store_info(zero.clone())));
    assert_eq!(view.update_store_info(store(1, 1 << 20)).used_size(), 0);
    let mut s = (*store(1, 100 << 20)).clone();
    s.set_new_store_stats(StoreStats {
        capacity: 1,
        used_size: 100 << 20,
        ..Default::default()
    });
    assert_eq!(
        view.update_store_info(Arc::new(s)).available(),
        1u64.wrapping_sub(3 << 20)
    );
}

#[tokio::test]
async fn task_068_count_threshold_roles_engines_and_splits() {
    use astersql_pd_core::{NodeState, Rule as ScatterRule, StoreLabel};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_placement::{
        LabelConstraint, LabelConstraintOp, PeerRoleType, Rule as PlacementRule, RuleManager,
    };
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    let c = cluster();
    let stores: Vec<_> = (1..=5)
        .map(|id| {
            let mut s = (*store(id, 100 << 20)).clone();
            let mut labels = vec![StoreLabel {
                key: "zone".into(),
                value: if id <= 3 { "z1" } else { "z2" }.into(),
            }];
            if id == 5 {
                labels.push(StoreLabel {
                    key: "engine".into(),
                    value: "tiflash".into(),
                });
            }
            if id == 1 {
                labels.push(StoreLabel {
                    key: "rack".into(),
                    value: "r1".into(),
                });
            }
            s.set_labels(labels);
            Arc::new(s)
        })
        .collect();
    let mut config = ProviderSnapshot::<String>::default();
    config.replication.max_replicas = 3;
    config.replication.enable_placement_rules = true;
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(68),
    );
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    let constraint = |key: &str, value: &str| LabelConstraint {
        key: key.into(),
        op: LabelConstraintOp::In,
        values: vec![value.into()],
    };
    for (id, role, start, end, constraints) in [
        (
            "leader",
            PeerRoleType::Leader,
            b"a".to_vec(),
            b"c".to_vec(),
            vec![constraint("zone", "z1")],
        ),
        (
            "follower",
            PeerRoleType::Follower,
            b"a".to_vec(),
            b"c".to_vec(),
            vec![constraint("zone", "z2")],
        ),
        (
            "learner",
            PeerRoleType::Learner,
            b"a".to_vec(),
            b"c".to_vec(),
            vec![constraint("engine", "tiflash")],
        ),
        (
            "voter",
            PeerRoleType::Voter,
            b"c".to_vec(),
            b"e".to_vec(),
            vec![constraint("rack", "r1")],
        ),
    ] {
        let rule = PlacementRule {
            group_id: "task68".into(),
            id: id.into(),
            role,
            count: 1,
            start_key_hex: start.iter().map(|v| format!("{v:02x}")).collect(),
            end_key_hex: end.iter().map(|v| format!("{v:02x}")).collect(),
            label_constraints: constraints,
            ..Default::default()
        };
        rules.set_rule(rule, 0, &config, &[]).await.unwrap();
    }
    rules
        .set_rule_group(
            astersql_pd_schedule_placement::RuleGroup {
                id: "task68".into(),
                index: 1,
                override_: true,
            },
            0,
        )
        .await
        .unwrap();
    let range = KeyRange {
        start_key: b"a".to_vec(),
        end_key: b"e".to_vec(),
    };
    let f = fixture();
    for case in f["thresholds"].as_array().unwrap() {
        let kind = match case["role"].as_str().unwrap() {
            "leader" => ScatterRule::LeaderScatter,
            "learner" => ScatterRule::LearnerScatter,
            _ => ScatterRule::PeerScatter,
        };
        let id = case["store"].as_u64().unwrap();
        let got = get_count_threshold(
            &c,
            &stores,
            &stores[(id - 1) as usize],
            &range,
            kind,
            &config.replication,
            &rules,
        );
        assert!(
            (got - case["expected"].as_f64().unwrap()).abs() < 1e-10,
            "{case}: {got}"
        );
    }
    // An empty matching set zeros only its own segment, preserving earlier ranges.
    let only_second = &stores[1..2];
    assert!(
        (get_count_threshold(
            &c,
            only_second,
            &stores[0],
            &range,
            ScatterRule::LeaderScatter,
            &config.replication,
            &rules
        ) - 1.0)
            .abs()
            < 1e-10
    );
    let mut removing = stores[1].as_ref().clone();
    removing.edit_meta(|m| m.node_state = NodeState::Removing);
    let mut removed = stores[2].as_ref().clone();
    removed.edit_meta(|m| m.node_state = NodeState::Removed);
    let eligible = vec![stores[0].clone(), Arc::new(removing), Arc::new(removed)];
    assert_eq!(
        get_count_threshold(
            &c,
            &eligible,
            &stores[0],
            &range,
            ScatterRule::LeaderScatter,
            &config.replication,
            &rules
        ),
        2.0
    );
    assert_eq!(
        get_count_threshold(
            &c,
            &[],
            &stores[0],
            &range,
            ScatterRule::LeaderScatter,
            &config.replication,
            &rules
        ),
        0.0
    );
    config.replication.enable_placement_rules = false;
    assert_eq!(
        get_count_threshold(
            &c,
            &stores,
            &stores[0],
            &range,
            ScatterRule::LeaderScatter,
            &config.replication,
            &rules
        ),
        2.0
    );
    assert!(
        (get_count_threshold(
            &c,
            &stores,
            &stores[0],
            &range,
            ScatterRule::PeerScatter,
            &config.replication,
            &rules
        ) - 1.2)
            .abs()
            < 1e-10
    );
    println!("15 frozen role/engine thresholds + 5 boundary cases passed");
}
