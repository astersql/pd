// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoresInfo,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderContext, BuilderError, OpKind, OpStep};
use std::sync::Arc;

#[tokio::test]
async fn task_060_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_060.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let stores = StoresInfo::new();
        for id in 1..=4 {
            let mut store = StoreInfo::new(StoreMeta {
                id,
                last_heartbeat: 1_000_000_000,
                ..Default::default()
            });
            if id == 3 {
                store.set_labels(vec![StoreLabel {
                    key: "engine".into(),
                    value: "tiflash".into(),
                }]);
            }
            stores.put_store(Arc::new(store));
        }
        let peers: Vec<_> = (1..=3)
            .map(|id| Peer {
                id: 100 + id,
                store_id: id,
                ..Default::default()
            })
            .collect();
        let region = RegionInfo::new(
            RegionMeta {
                id: 60,
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            vec![],
        );
        let mut target = ScatterTarget {
            peers: peers.iter().map(|p| (p.store_id, p.clone())).collect(),
            leader: 1,
            leader_picked_count: 7,
        };
        if case["move"] == true {
            target.peers.remove(&1);
            target.peers.insert(
                4,
                Peer {
                    id: 0,
                    store_id: 4,
                    ..Default::default()
                },
            );
            target.leader = 4;
        }
        if case["invalid"] == true {
            target.peers.remove(&2);
        }
        if case["no_leader"] == true {
            target.leader = 0;
        }
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = false;
        let context = BuilderContext {
            stores: &stores,
            config: &conf,
            rules: &[],
            now_unix_nanos: 1_000_000_000,
        };
        let mut counts = ScatterCounts::default();
        counts.set("other", "tikv", 4, false, 23);
        counts.set("g", "tikv", 4, true, 7);
        let mut allocations = 0;
        let result = create_scatter_operator(
            context,
            &region,
            &target,
            "g",
            case["skip"] == true,
            false,
            &mut counts,
            || {
                allocations += 1;
                if case["allocation_fail"] == true {
                    Err(BuilderError::Allocation("disk unavailable".into()))
                } else {
                    Ok(900)
                }
            },
        )
        .await;
        if case["invalid"] == true || case["no_leader"] == true {
            assert!(
                matches!(result, Err(ScatterOperatorError::InvalidTarget)),
                "{name}"
            );
        } else if case["allocation_fail"] == true {
            assert!(
                matches!(
                    result,
                    Err(ScatterOperatorError::Build(BuilderError::Allocation(_)))
                ),
                "{name}"
            );
        } else if case["move"] == true {
            let op = result.unwrap().unwrap();
            assert_eq!(op.priority_level(), PriorityLevel::HIGH, "{name}");
            assert_ne!(op.kind().0 & OpKind::ADMIN.0, 0);
            assert_eq!(op.additional_info().get("group").as_deref(), Some("g"));
            assert_eq!(
                op.additional_info().get("leader-picked-count").as_deref(),
                Some("7")
            );
            let steps: Vec<_> = (0..op.len()).map(|i| op.step(i).unwrap()).collect();
            assert!(
                matches!(steps[0], OpStep::AddLearner(p) if p.to_store == 4 && p.peer_id == 900 && p.is_light_weight)
            );
            assert!(
                steps
                    .iter()
                    .any(|s| matches!(s, OpStep::TransferLeader(p) if p.to_store == 4))
            );
            assert!(
                steps.iter().any(|s| matches!(s, OpStep::RemovePeer(p) if p.from_store == 1 && p.is_light_weight == (case["skip"] == true)))
            );
        } else {
            assert!(result.unwrap().is_none(), "{name}");
            assert_eq!(allocations, 0);
        }
        let expected = case["accounted_store"].as_u64().unwrap();
        assert_eq!(
            counts.get("g", "tikv", 1, false),
            u64::from(expected == 1),
            "{name}: original count"
        );
        assert_eq!(
            counts.get("g", "tikv", 4, false),
            u64::from(expected == 4),
            "{name}: target count"
        );
        assert_eq!(counts.get("g", "tikv", 1, true), u64::from(expected == 1));
        assert_eq!(
            counts.get("g", "tikv", 4, true),
            7 + u64::from(expected == 4)
        );
        assert_eq!(counts.get("g", "tikv", 2, false), 1);
        assert_eq!(counts.get("g", "tiflash", 3, false), 1);
        assert_eq!(counts.get("g", "tikv", 3, false), 0);
        assert_eq!(counts.get("other", "tikv", 4, false), 23);
        assert_eq!(region.meta().peers, peers);
        println!("case {name}: passed");
    }
}

#[tokio::test]
async fn task_060_accounting_boundaries() {
    let stores = StoresInfo::new();
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    let peers = [
        (
            1,
            Peer {
                id: 101,
                store_id: 1,
                ..Default::default()
            },
        ),
        (
            9,
            Peer {
                id: 109,
                store_id: 9,
                ..Default::default()
            },
        ),
    ]
    .into();
    let mut counts = ScatterCounts::default();
    counts.record_distribution(&stores, &peers, 0, "g");
    assert_eq!(counts, ScatterCounts::default());
    for _ in 0..2 {
        counts.record_distribution(&stores, &peers, 1, "g");
    }
    assert_eq!(counts.get("g", "tikv", 1, false), 2);
    assert_eq!(counts.get("g", "tikv", 1, true), 2);
    assert_eq!(counts.get("g", "tikv", 9, false), 0);
    let saved = counts.clone();
    let region = RegionInfo::new(
        RegionMeta {
            id: 60,
            peers: peers.values().cloned().collect(),
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    let target = ScatterTarget {
        peers,
        leader: 1,
        leader_picked_count: 0,
    };
    let conf = ProviderSnapshot::<()>::default();
    let result = create_scatter_operator(
        BuilderContext {
            stores: &stores,
            config: &conf,
            rules: &[],
            now_unix_nanos: 0,
        },
        &region,
        &target,
        "g",
        false,
        false,
        &mut counts,
        || panic!("must not allocate"),
    )
    .await;
    assert!(matches!(result, Err(ScatterOperatorError::NoLeader)));
    assert_eq!(counts, saved);
}

#[tokio::test]
async fn task_060_invalid_roles_and_identity() {
    let stores = StoresInfo::new();
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    let peer = Peer {
        id: 101,
        store_id: 1,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            id: 60,
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer.clone()),
        vec![],
        vec![],
    );
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    for variant in 0..4 {
        let mut target = ScatterTarget {
            peers: [(1, peer.clone())].into(),
            leader: 1,
            leader_picked_count: 0,
        };
        match variant {
            0 => target.peers.get_mut(&1).unwrap().is_witness = true,
            1 => target.peers.get_mut(&1).unwrap().role = astersql_pd_core::PeerRole::Learner,
            2 => target.peers.get_mut(&1).unwrap().store_id = 2,
            _ => {
                target.peers = [(
                    2,
                    Peer {
                        id: 0,
                        store_id: 2,
                        ..peer.clone()
                    },
                )]
                .into();
                target.leader = 2;
            }
        }
        let mut counts = ScatterCounts::default();
        let result = create_scatter_operator(
            BuilderContext {
                stores: &stores,
                config: &conf,
                rules: &[],
                now_unix_nanos: 0,
            },
            &region,
            &target,
            "g",
            false,
            false,
            &mut counts,
            || Ok(900),
        )
        .await;
        if variant < 3 {
            assert!(matches!(result, Err(ScatterOperatorError::InvalidTarget)));
        } else {
            assert!(matches!(result, Err(ScatterOperatorError::Build(_))));
        }
        assert_eq!(counts.get("g", "tikv", 1, false), 1);
        assert_eq!(counts.get("g", "tikv", 1, true), 1);
        assert_eq!(counts.get("g", "tikv", 2, false), 0);
    }
}
