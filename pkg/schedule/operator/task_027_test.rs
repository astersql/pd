// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta, StoresInfo,
};
use astersql_pd_schedule_config::ProviderSnapshot;
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
async fn task_027_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_027.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);
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
            if c["execution_boundary"] == "existing-voter-rejects-add-learner" {
                // The source planner admits this input. Its real step contract
                // rejects execution; do not invent successful state transitions.
                let OpStep::AddLearner(step) = op.step(0).unwrap() else {
                    panic!("expected source boundary");
                };
                let cluster = astersql_pd_core::BasicCluster::new();
                for store in stores.get_stores() {
                    cluster.stores.put_store(store);
                }
                assert!(!step.is_finish(&r));
                assert_eq!(step.get_cmd(&r, false), None);
                assert_eq!(
                    step.check_in_progress(&cluster, &config, &r, false, now),
                    Err(PeerStepError::AlreadyVoter)
                );
                assert!(op.start());
                assert_eq!(op.check(&r), op.step(0));
                assert_eq!(op.current_step(), 0);
                assert_eq!(op.check(&r), op.step(0));
                assert_eq!(op.current_step(), 0);
            }
        }
        assert_eq!(r.meta().peers, origin, "input ownership {name}");
    }
    println!("task027: {} fixed source-derived cases passed", cases.len());
}

/// Exhaustive add/remove/promote targets over four real stores (no demotion).
/// Mixed demotion is covered by fixed source cases, including its execution
/// rejection boundary, rather than assuming a stronger universal Go guarantee.
#[tokio::test]
async fn task_027_safe_intermediate_configurations() {
    let stores = StoresInfo::new();
    for s in 1..=4 {
        let mut st = StoreInfo::new(StoreMeta {
            id: s,
            ..Default::default()
        });
        st.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        stores.put_store(Arc::new(st));
    }
    let config = ProviderSnapshot::<()>::default();
    let origin = vec![
        Peer {
            id: 11,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: 12,
            store_id: 2,
            ..Default::default()
        },
        Peer {
            id: 13,
            store_id: 3,
            role: PeerRole::Learner,
            ..Default::default()
        },
    ];
    let region = RegionInfo::new(
        RegionMeta {
            id: 27,
            peers: origin.clone(),
            ..Default::default()
        },
        Some(origin[0].clone()),
        vec![],
        vec![],
    );
    let mut checked = 0;
    for code in 0..81 {
        let mut digits = code;
        let mut target = BTreeMap::new();
        for s in 1..=4 {
            let role = digits % 3;
            digits /= 3;
            if role > 0 {
                target.insert(
                    s,
                    Peer {
                        id: s + 10,
                        store_id: s,
                        role: if role == 1 {
                            PeerRole::Voter
                        } else {
                            PeerRole::Learner
                        },
                        ..Default::default()
                    },
                );
            }
        }
        if target.iter().any(|(s, p)| *s <= 2 && p.is_learner()) {
            continue;
        }
        if !target.values().any(|p| !p.is_learner()) {
            continue;
        }
        let mut b = Builder::new(
            "safety",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 1_000_000_000_000,
            },
            &region,
            BuilderOptions::default(),
        );
        b.set_peers(target.clone());
        let result = b
            .build_simple(OpKind(0), SimpleBuildOptions::default(), || {
                panic!("all IDs assigned")
            })
            .await;
        let mut current: BTreeMap<_, _> = origin.iter().cloned().map(|p| (p.store_id, p)).collect();
        let mut leader = 1;
        match result {
            Err(BuilderError::NoSteps) => assert_eq!(current, target),
            Err(e) => panic!("target {code}: {e:?}"),
            Ok(op) => {
                for i in 0..op.len() {
                    let step = op.step(i).unwrap();
                    match step {
                        OpStep::AddLearner(s) => {
                            assert!(
                                !current.contains_key(&s.to_store),
                                "target {code}, step {i}: {step:?}, current {current:?}"
                            );
                            current.insert(
                                s.to_store,
                                Peer {
                                    id: s.peer_id,
                                    store_id: s.to_store,
                                    role: PeerRole::Learner,
                                    is_witness: s.is_witness,
                                },
                            );
                        }
                        OpStep::PromoteLearner(s) => {
                            let peer = current.get_mut(&s.to_store).unwrap();
                            assert!(peer.is_learner());
                            assert_eq!(peer.id, s.peer_id);
                            peer.role = PeerRole::Voter;
                        }
                        OpStep::RemovePeer(s) => {
                            assert_ne!(leader, s.from_store, "remove leader in target {code}");
                            assert_eq!(current.remove(&s.from_store).unwrap().id, s.peer_id);
                        }
                        OpStep::TransferLeader(s) => {
                            assert_eq!(leader, s.from_store);
                            let peer = &current[&s.to_store];
                            assert!(!peer.is_learner() && !peer.is_witness);
                            leader = s.to_store;
                        }
                        _ => panic!("unexpected step"),
                    }
                    assert!(!current[&leader].is_learner());
                    let heartbeat = RegionInfo::new(
                        RegionMeta {
                            id: 27,
                            peers: current.values().cloned().collect(),
                            ..Default::default()
                        },
                        Some(current[&leader].clone()),
                        vec![],
                        vec![],
                    );
                    assert!(
                        step.is_finish(&heartbeat),
                        "acknowledgement target {code} step {i}"
                    );
                }
                assert_eq!(current, target, "target {code}");
                assert_eq!(b.target_leader(), leader);
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 32);
    println!(
        "task027: {checked} target configurations preserve leader safety and acknowledge every step"
    );
}
