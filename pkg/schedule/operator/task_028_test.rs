// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoresInfo,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::PeerRoleType;
use std::{collections::BTreeMap, sync::Arc};
fn peers(v: &serde_json::Value) -> Vec<Peer> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| Peer {
            store_id: p[0].as_u64().unwrap(),
            id: p[2].as_u64().unwrap(),
            role: if p[1] == "l" {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        })
        .collect()
}
#[tokio::test]
async fn task_028_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_028.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 17);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let stores = StoresInfo::new();
        let now = 1_000_000_000_000i128;
        for s in 1..=5 {
            if c["missing"]
                .as_array()
                .is_some_and(|v| v.contains(&s.into()))
            {
                continue;
            }
            let mut st = StoreInfo::new(StoreMeta {
                id: s,
                labels: vec![StoreLabel {
                    key: "zone".into(),
                    value: c["zones"][s.to_string()].as_str().unwrap_or("").into(),
                }],
                ..Default::default()
            });
            st.set_last_heartbeat_unix_nanos(
                if c["down"].as_array().is_some_and(|v| v.contains(&s.into())) {
                    1
                } else {
                    now as i64
                },
            );
            st.set_leader_count(c["counts"][s.to_string()].as_i64().unwrap_or(0));
            st.set_leader_size(c["sizes"][s.to_string()].as_i64().unwrap_or(0));
            stores.put_store(Arc::new(st));
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.location_labels = vec!["zone".into()].into();
        config.schedule.max_store_down_time = "100s".parse().unwrap();
        let mut origin = peers(&c["origin"]);
        if c["origin_witness"] == true {
            origin[0].is_witness = true;
        }
        let r = RegionInfo::new(
            RegionMeta {
                id: 27,
                peers: origin.clone(),
                ..Default::default()
            },
            Some(origin[0].clone()),
            vec![],
            vec![],
        );
        let mut b = Builder::new(
            "simple",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: now,
            },
            &r,
            BuilderOptions::default(),
        );
        b.set_peers(
            peers(&c["target"])
                .into_iter()
                .map(|mut p| {
                    if p.store_id == 1 && c["origin_witness"] == true {
                        p.is_witness = true;
                    }
                    (p.store_id, p)
                })
                .collect::<BTreeMap<_, _>>(),
        );
        if let Some(s) = c["leader"].as_u64() {
            b.set_leader(s);
        }
        if let Some(roles) = c["roles"].as_object() {
            b.set_expected_roles(
                roles
                    .iter()
                    .map(|(s, r)| {
                        (
                            s.parse().unwrap(),
                            match r.as_str().unwrap() {
                                "leader" => PeerRoleType::Leader,
                                "follower" => PeerRoleType::Follower,
                                _ => PeerRoleType::Voter,
                            },
                        )
                    })
                    .collect(),
            );
        }
        if let Some(ids) = c["leaders"].as_array() {
            b.set_leaders(ids.iter().map(|v| v.as_u64().unwrap()).collect());
        }
        if c["force"] == true {
            b.enable_force_target_leader();
        }
        let mut allocations = 0;
        let result = b
            .build_simple(
                OpKind::ADMIN,
                SimpleBuildOptions {
                    add_light_peer: c["light"] == true,
                    remove_light_peer: c["light"] == true,
                },
                || {
                    allocations += 1;
                    if c["allocation_fault"] == true {
                        Err(BuilderError::Allocation("injected".into()))
                    } else {
                        Ok(99 + allocations)
                    }
                },
            )
            .await;
        assert_eq!(allocations, c["allocated"].as_u64().unwrap_or(0), "{name}");
        assert_eq!(
            result.as_ref().err().map(|e| format!("{e:?}")),
            c["error"].as_str().map(str::to_owned),
            "{name}"
        );
        if let Err(error) = &result {
            assert_eq!(b.error(), Some(error), "sticky build error: {name}");
        }
        if let Ok(op) = result {
            let mut actual = vec![];
            let mut region_change = false;
            let mut leader_change = false;
            for i in 0..op.len() {
                actual.push(match op.step(i).unwrap() {
                    OpStep::AddLearner(s) => {
                        region_change = true;
                        assert_eq!(s.send_store, 1, "{name}");
                        assert_eq!(s.is_light_weight, c["light"] == true);
                        serde_json::json!(["add", s.to_store, s.peer_id])
                    }
                    OpStep::PromoteLearner(s) => {
                        serde_json::json!(["promote", s.to_store, s.peer_id])
                    }
                    OpStep::RemovePeer(s) => {
                        region_change = true;
                        assert_eq!(s.is_light_weight, c["light"] == true);
                        assert_eq!(
                            s.is_down_store,
                            c["down"]
                                .as_array()
                                .is_some_and(|v| v.contains(&s.from_store.into()))
                        );
                        serde_json::json!(["remove", s.from_store, s.peer_id])
                    }
                    OpStep::TransferLeader(s) => {
                        leader_change = true;
                        assert_eq!(
                            serde_json::json!(s.to_stores),
                            c.get("to_stores").cloned().unwrap_or(serde_json::json!([])),
                            "{name}"
                        );
                        serde_json::json!(["transfer", s.from_store, s.to_store])
                    }
                    s => panic!("unexpected {s:?}"),
                });
            }
            assert_eq!(serde_json::Value::Array(actual), c["steps"], "{name}");
            assert_eq!(
                op.kind().0,
                OpKind::ADMIN.0
                    | if region_change { OpKind::REGION.0 } else { 0 }
                    | if leader_change { OpKind::LEADER.0 } else { 0 },
                "{name}"
            );
            assert_eq!(op.region_id(), 27);
            assert_eq!(op.desc(), "simple");
        }
        assert_eq!(r.meta().peers, origin, "input ownership {name}");
    }
    println!("task028: {} fixed source-derived cases passed", cases.len());
}

#[tokio::test]
async fn task_028_current_leader_admission() {
    use astersql_pd_core::NodeState;
    let stores = StoresInfo::new();
    let mut st = StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Removing,
        ..Default::default()
    });
    st.set_last_heartbeat_unix_nanos(1_000_000_000_000);
    stores.put_store(Arc::new(st));
    let config = ProviderSnapshot::<()>::default();
    let peer = |s| Peer {
        id: s + 10,
        store_id: s,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            peers: vec![peer(1), peer(2)],
            ..Default::default()
        },
        Some(peer(1)),
        vec![],
        vec![],
    );
    let b = Builder::new(
        "leader",
        BuilderContext {
            stores: &stores,
            config: &config,
            rules: &[],
            now_unix_nanos: 1_000_000_000_000,
        },
        &region,
        BuilderOptions::default(),
    );
    assert!(
        !b.allow_leader_at(&peer(1), false, 2).await,
        "former leader must pass store filter after transfer"
    );
    assert!(
        b.allow_leader_at(&peer(2), false, 2).await,
        "current leader bypasses missing store"
    );
    let planner = super::super::Planner {
        builder: &b,
        current: BTreeMap::from([(2, peer(2))]),
        add: BTreeMap::from([(3, peer(3))]),
        remove: BTreeMap::new(),
        promote: BTreeMap::new(),
        added: BTreeMap::new(),
        leader: 2,
        steps: vec![],
        kind: OpKind(0),
        options: SimpleBuildOptions::default(),
    };
    assert_eq!(planner.select_target_leader().await, 2);
    assert_eq!(planner.next().await.unwrap().before_add, 2);
    let learner = Peer {
        role: PeerRole::Learner,
        ..peer(2)
    };
    assert!(!b.allow_leader_at(&learner, true, 2).await);
    let witness = Peer {
        is_witness: true,
        ..peer(2)
    };
    assert!(!b.allow_leader_at(&witness, true, 2).await);
}

#[test]
fn task_028_preference_vectors() {
    use super::super::{Plan, Planner};
    use astersql_pd_core::NodeState;
    let stores = StoresInfo::new();
    for s in 1..=4 {
        let mut st = StoreInfo::new(StoreMeta {
            id: s,
            node_state: if s == 1 {
                NodeState::Removing
            } else {
                NodeState::Serving
            },
            labels: vec![StoreLabel {
                key: "zone".into(),
                value: if s == 2 || s == 4 { "a" } else { "b" }.into(),
            }],
            ..Default::default()
        });
        st.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        stores.put_store(Arc::new(st));
    }
    let mut config = ProviderSnapshot::<()>::default();
    config.replication.location_labels = vec!["zone".into()].into();
    let peer = |s| Peer {
        id: s + 10,
        store_id: s,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            peers: vec![peer(1), peer(2), peer(3)],
            ..Default::default()
        },
        Some(peer(1)),
        vec![],
        vec![],
    );
    let mut b = Builder::new(
        "rank",
        BuilderContext {
            stores: &stores,
            config: &config,
            rules: &[],
            now_unix_nanos: 1_000_000_000_000,
        },
        &region,
        BuilderOptions::default(),
    );
    b.expected_roles.insert(3, PeerRoleType::Leader);
    b.target_leader = 3;
    let p = Planner {
        builder: &b,
        current: b.origin_peers.clone(),
        add: BTreeMap::new(),
        remove: BTreeMap::new(),
        promote: BTreeMap::from([(2, peer(2))]),
        added: BTreeMap::from([(3, 5)]),
        leader: 1,
        steps: vec![],
        kind: OpKind(0),
        options: SimpleBuildOptions::default(),
    };
    assert_eq!(p.leader_rank(1), [0, 0, 1, 1, 0]);
    assert_eq!(p.leader_rank(2), [0, 1, 0, 0, 0]);
    assert_eq!(p.leader_rank(3), [1, 1, 0, 1, -5]);
    let base = Plan {
        before_add: 2,
        before_remove: 3,
        add: Some(peer(4)),
        remove: Some(peer(2)),
        ..Default::default()
    };
    assert_eq!(p.rank(&base), [1, 1, -5, 0, 1, 0]);
    let far = Plan {
        remove: Some(peer(3)),
        ..base.clone()
    };
    assert_eq!(p.rank(&far)[0], 0);
    let mut best = Some(base.clone());
    p.best(&mut best, far);
    assert_eq!(best.as_ref().unwrap().remove.as_ref().unwrap().store_id, 2);
    let same_rank = Plan {
        promote: Some(peer(2)),
        ..base.clone()
    };
    p.best(&mut best, same_rank);
    assert!(
        best.as_ref().unwrap().promote.is_none(),
        "ties keep the first plan"
    );
    let fresh = Plan {
        before_add: 1,
        before_remove: 4,
        ..base.clone()
    };
    assert_eq!(p.rank(&fresh), [1, 0, -1, 0, 0, 2]);
    let target_add = Plan {
        before_add: 1,
        before_remove: 3,
        add: Some(peer(3)),
        remove: None,
        ..Default::default()
    };
    assert_eq!(p.rank(&target_add), [0, 0, -1, 1, 1, 2]);
    println!("task028: 3 leader vectors, 5 plan vectors and 2 comparisons passed");
}
