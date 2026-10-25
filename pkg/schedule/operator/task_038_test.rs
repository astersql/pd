// Copyright 2026 AsterSQL.
use super::{apply_steps::*, *};
use astersql_pd_core::{
    BasicCluster, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, RegionOption, StoreInfo,
    StoreMeta,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};
fn region() -> RegionInfo {
    let peers = vec![
        Peer {
            id: 11,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: 22,
            store_id: 2,
            ..Default::default()
        },
    ];
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 38,
            peers: peers.clone(),
            epoch: Some(RegionEpoch {
                conf_ver: 7,
                version: 9,
            }),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = 50;
    r
}
fn op(steps: Vec<OpStep>) -> Operator {
    Operator::new(
        "apply",
        "",
        38,
        region().meta().epoch.clone(),
        OpKind::REGION,
        50,
        steps,
    )
}
fn basic(name: &str) -> OpStep {
    match name {
        "transfer" => TransferLeader {
            from_store: 1,
            to_store: 2,
            ..Default::default()
        }
        .into(),
        "add" => AddPeer {
            to_store: 3,
            peer_id: 33,
            ..Default::default()
        }
        .into(),
        "learner" => AddLearner {
            to_store: 3,
            peer_id: 33,
            ..Default::default()
        }
        .into(),
        "promote" => PromoteLearner {
            to_store: 3,
            peer_id: 33,
            ..Default::default()
        }
        .into(),
        "remove" => RemovePeer {
            from_store: 2,
            peer_id: 22,
            ..Default::default()
        }
        .into(),
        _ => unreachable!(),
    }
}
fn put(c: &mut BasicCluster, r: RegionInfo) {
    let r = Arc::new(r);
    let (old, overlaps, changed) = c.regions.set_region(r.clone());
    c.regions
        .update_sub_tree(r, old.as_deref(), &overlaps, changed);
}
#[test]
fn task_038_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_038.json"
    ))
    .unwrap();
    assert_eq!(fixture["evidence_kind"], "source_assertion");
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut r = region();
        if name == "promote" {
            r = r.clone_with([RegionOption::AddPeer(Peer {
                store_id: 3,
                id: 33,
                role: PeerRole::Learner,
                is_witness: true,
            })]);
        }
        let original = r.clone();
        let o = op(vec![basic(name)]);
        let before = OpInfluence::new();
        o.unfinished_influence(&before, &r);
        let next = apply_operator_step(&r, &o);
        let peers: Vec<_> = next
            .meta()
            .peers
            .iter()
            .map(|p| {
                serde_json::json!([
                    p.store_id,
                    p.id,
                    if p.role == PeerRole::Learner {
                        "l"
                    } else {
                        "v"
                    }
                ])
            })
            .collect();
        assert_eq!(serde_json::json!(peers), case["peers"], "{name}");
        assert_eq!(
            next.leader().unwrap().store_id,
            case["leader"].as_u64().unwrap()
        );
        assert_eq!(next.meta().epoch, original.meta().epoch);
        assert_eq!(next.stats(), original.stats());
        assert_eq!(r.meta().peers, original.meta().peers);
        assert!(next.meta().peers.iter().all(|p| !p.is_witness));
        assert_eq!(o.status(), STARTED); // Check acknowledges on the following call.
        let after = OpInfluence::new();
        o.unfinished_influence(&after, &next);
        assert!(after.snapshot().is_empty());
        if name != "promote" {
            assert!(!before.snapshot().is_empty());
        }
        assert_eq!(apply_operator_step(&next, &o).meta(), next.meta());
        assert_eq!(o.status(), SUCCESS);
    }
}
#[test]
fn task_038_apply_full_and_store_status() {
    let mut c = BasicCluster::new();
    for id in 1..=4 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    put(&mut c, region());
    c.update_all_store_status();
    let o = op(vec![
        basic("learner"),
        basic("promote"),
        basic("transfer"),
        RemovePeer {
            from_store: 1,
            peer_id: 11,
            ..Default::default()
        }
        .into(),
    ]);
    apply_operator(&mut c, &o, 123456);
    let r = c.regions.get_region(38).unwrap();
    assert_eq!(o.status(), SUCCESS);
    assert_eq!(o.current_step(), 4);
    assert_eq!(r.store_ids().into_iter().collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(r.leader().unwrap().store_id, 2);
    assert_eq!(c.stores.get_store(1).unwrap().region_count(), 0);
    assert_eq!(c.stores.get_store(1).unwrap().leader_count(), 0);
    assert_eq!(c.stores.get_store(2).unwrap().leader_count(), 1);
    assert_eq!(c.stores.get_store(3).unwrap().region_count(), 1);
    assert_eq!(c.stores.get_store(3).unwrap().region_size(), 50);
    assert_eq!(
        c.stores.get_store(3).unwrap().last_heartbeat_unix_nanos(),
        123456
    );
    assert_eq!(
        c.stores.get_store(4).unwrap().last_heartbeat_unix_nanos(),
        0
    );
    // Go samples the previous StoreInfo size before updating indexed counts.
    assert_eq!(c.stores.get_store(3).unwrap().used_size(), 0);
    assert_eq!(c.stores.get_store(2).unwrap().used_size(), 50 * 1024 * 1024);
    let done = op(vec![]);
    apply_operator(&mut c, &done, 200000);
    assert_eq!(done.status(), SUCCESS);
    let canceled = op(vec![basic("add")]);
    canceled.cancel();
    apply_operator(&mut c, &canceled, 300000);
    assert_eq!(canceled.status(), CANCELED);
}
#[test]
fn task_038_invalid_membership_panics() {
    let r = region();
    let cases = vec![
        (
            r.clone(),
            AddPeer {
                to_store: 2,
                peer_id: 99,
                ..Default::default()
            }
            .into(),
            "Add peer that exists",
        ),
        (
            r.clone(),
            AddLearner {
                to_store: 2,
                peer_id: 99,
                ..Default::default()
            }
            .into(),
            "Add learner that exists",
        ),
        (
            r.clone(),
            PromoteLearner {
                to_store: 3,
                peer_id: 33,
                ..Default::default()
            }
            .into(),
            "Promote peer that doesn't exist",
        ),
        (
            r.clone(),
            RemovePeer {
                from_store: 1,
                peer_id: 11,
                ..Default::default()
            }
            .into(),
            "Cannot remove the leader peer",
        ),
        (
            r.clone_with([RegionOption::PendingPeers(vec![r.meta().peers[1].clone()])]),
            AddPeer {
                to_store: 2,
                peer_id: 22,
                ..Default::default()
            }
            .into(),
            "Add peer that exists",
        ),
    ];
    for (r, s, msg) in cases {
        let o = op(vec![s]);
        let e = catch_unwind(AssertUnwindSafe(|| apply_operator_step(&r, &o))).unwrap_err();
        assert_eq!(e.downcast_ref::<&str>(), Some(&msg));
    }
    // Missing removals and already satisfied additions are skipped by Operator::check.
    let o = op(vec![
        RemovePeer {
            from_store: 9,
            peer_id: 99,
            ..Default::default()
        }
        .into(),
        AddPeer {
            to_store: 2,
            peer_id: 22,
            ..Default::default()
        }
        .into(),
    ]);
    assert_eq!(apply_operator_step(&r, &o).meta(), r.meta());
    assert_eq!(o.status(), SUCCESS);
}
#[test]
fn task_038_unsupported_steps_and_failed_publication() {
    let r = region().clone_with([RegionOption::AddPeer(Peer {
        store_id: 3,
        id: 33,
        role: PeerRole::Learner,
        is_witness: true,
    })]);
    let steps = vec![
        BecomeWitness {
            peer_id: 11,
            store_id: 1,
        }
        .into(),
        BecomeNonWitness {
            peer_id: 33,
            store_id: 3,
            send_store: 1,
        }
        .into(),
        BatchSwitchWitness {
            to_witnesses: vec![BecomeWitness {
                peer_id: 11,
                store_id: 1,
            }],
            ..Default::default()
        }
        .into(),
        ChangePeerV2Enter {
            promote_learners: vec![PromoteLearner {
                to_store: 3,
                peer_id: 33,
                ..Default::default()
            }],
            ..Default::default()
        }
        .into(),
        ChangePeerV2Leave {
            promote_learners: vec![PromoteLearner {
                to_store: 3,
                peer_id: 33,
                ..Default::default()
            }],
            ..Default::default()
        }
        .into(),
        SplitRegion::default().into(),
        MergeRegion::default().into(),
    ];
    assert_eq!(steps.len(), 7);
    for step in steps {
        let o = op(vec![step]);
        let e = catch_unwind(AssertUnwindSafe(|| apply_operator_step(&r, &o))).unwrap_err();
        assert_eq!(e.downcast_ref::<&str>(), Some(&"Unknown operator step"));
    }
    let mut c = BasicCluster::new();
    put(&mut c, r.clone());
    let o = op(vec![
        AddPeer {
            to_store: 4,
            peer_id: 44,
            ..Default::default()
        }
        .into(),
        MergeRegion::default().into(),
    ]);
    assert!(catch_unwind(AssertUnwindSafe(|| apply_operator(&mut c, &o, 0))).is_err());
    assert_eq!(c.regions.get_region(38).unwrap().meta(), r.meta());
}

#[test]
fn task_038_source_quirks_and_terminal_noop() {
    let r = region();
    // The source helper builds ordinary peers even when the step asks for witnesses.
    for step in [
        AddPeer {
            to_store: 3,
            peer_id: 33,
            is_witness: true,
            ..Default::default()
        }
        .into(),
        AddLearner {
            to_store: 3,
            peer_id: 33,
            is_witness: true,
            ..Default::default()
        }
        .into(),
    ] {
        let o = op(vec![step]);
        let next = apply_operator_step(&r, &o);
        assert!(!next.get_store_peer(3).unwrap().is_witness);
        // A second call cannot silently acknowledge the unsatisfied witness request.
        assert!(catch_unwind(AssertUnwindSafe(|| apply_operator_step(&next, &o))).is_err());
    }
    let o = op(vec![
        TransferLeader {
            from_store: 1,
            to_store: 9,
            ..Default::default()
        }
        .into(),
    ]);
    assert!(apply_operator_step(&r, &o).leader().is_none());
    assert_eq!(r.leader().unwrap().store_id, 1);
    let done = op(vec![MergeRegion::default().into()]);
    done.cancel();
    assert_eq!(apply_operator_step(&r, &done).meta(), r.meta());
    assert_eq!(done.status(), CANCELED);
    // Promotion preserves unrelated pending/down reports and drops witness status.
    let learner = Peer {
        id: 33,
        store_id: 3,
        role: PeerRole::Learner,
        is_witness: true,
    };
    let r = r.clone_with([
        RegionOption::AddPeer(learner.clone()),
        RegionOption::PendingPeers(vec![learner]),
    ]);
    let o = op(vec![basic("promote")]);
    let next = apply_operator_step(&r, &o);
    assert_eq!(next.pending_peers(), r.pending_peers());
    assert_eq!(next.get_store_peer(3).unwrap().role, PeerRole::Voter);
    assert!(!next.get_store_peer(3).unwrap().is_witness);
}
