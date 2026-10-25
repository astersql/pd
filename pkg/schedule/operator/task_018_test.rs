// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_storelimit::Type;
use std::sync::Arc;

fn peer(id: u64) -> Peer {
    Peer {
        id: id + 10,
        store_id: id,
        ..Default::default()
    }
}
fn region(leader: u64) -> RegionInfo {
    let mut r = RegionInfo::new(
        RegionMeta {
            peers: vec![peer(1), peer(2), peer(3)],
            ..Default::default()
        },
        (leader != 0).then(|| peer(leader)),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = 50;
    r
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_018.json"
    ))
    .unwrap()
}
#[test]
fn task_018_behavior_cases() {
    for c in fixture()["completion"].as_array().unwrap() {
        let step = TransferLeader {
            from_store: 1,
            to_store: c["to"].as_u64().unwrap(),
            to_stores: c["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect(),
        };
        let r = region(c["leader"].as_u64().unwrap());
        assert_eq!(step.is_finish(&r), c["finished"].as_bool().unwrap(), "{c}");
        assert_eq!(step.conf_ver_changed(&r), 0);
        assert_eq!(
            step.to_string(),
            format!("transfer leader from store 1 to store {}", step.to_store)
        );
    }
    let step = TransferLeader {
        from_store: 1,
        to_store: 2,
        to_stores: vec![3, 9, 3],
    };
    let r = region(1);
    for v2 in [false, true] {
        let cmd = step.get_cmd(&r, v2);
        assert_eq!(
            cmd,
            astersql_pd_schedule_hbstream::Operation {
                transfer_leader: Some(astersql_pd_schedule_hbstream::TransferLeader {
                    peer: Some(peer(2)),
                    peers: vec![Some(peer(3)), None, Some(peer(3))]
                }),
                ..Default::default()
            }
        );
    }
    let missing = TransferLeader {
        to_store: 99,
        ..Default::default()
    }
    .get_cmd(&r, false);
    assert_eq!(
        missing.transfer_leader.unwrap(),
        astersql_pd_schedule_hbstream::TransferLeader::default()
    );
    // A new heartbeat, rather than sending a command, completes the step.
    assert!(!step.is_finish(&r));
    assert!(step.is_finish(&region(3)));
}

#[test]
fn task_018_candidate_health_and_errors() {
    for c in fixture()["validation"].as_array().unwrap() {
        let cluster = BasicCluster::new();
        for s in c["stores"].as_array().unwrap() {
            cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id: s[0].as_u64().unwrap(),
                last_heartbeat: 2_000_000_000_000 - s[1].as_i64().unwrap(),
                ..Default::default()
            })));
        }
        let mut peers: Vec<_> = c["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let mut p = peer(v[0].as_u64().unwrap());
                if v[1].as_bool().unwrap() {
                    p.role = PeerRole::Learner;
                }
                p
            })
            .collect();
        let r = RegionInfo::new(
            RegionMeta {
                peers: std::mem::take(&mut peers),
                ..Default::default()
            },
            Some(peer(1)),
            vec![],
            vec![],
        );
        let step = TransferLeader {
            from_store: 1,
            to_store: c["to"].as_u64().unwrap(),
            to_stores: c["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect(),
        };
        let before = step.clone();
        let mut cfg = ProviderSnapshot::<String>::default();
        cfg.schedule.max_store_down_time = "30m".parse().unwrap();
        for _ in 0..2 {
            let result = step.check_in_progress(
                &cluster,
                &cfg,
                &r,
                c["health"].as_bool().unwrap(),
                2_000_000_000_000,
            );
            let actual = result.err().map(|e| e.to_string());
            assert_eq!(actual.as_deref(), c["error"].as_str(), "{c}");
        }
        assert_eq!(step, before);
    }
}

#[test]
fn task_018_influence_and_timeout() {
    for c in fixture()["influence"].as_array().unwrap() {
        let step = TransferLeader {
            from_store: 1,
            to_store: c["to"].as_u64().unwrap(),
            to_stores: c["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect(),
        };
        let influence = OpInfluence::new();
        step.influence(&influence, &region(1));
        let snapshot = influence.snapshot();
        assert_eq!(snapshot.len(), c["expected"].as_array().unwrap().len());
        for row in c["expected"].as_array().unwrap() {
            let s = &snapshot[&row[0].as_u64().unwrap()];
            assert_eq!(
                (
                    s.leader_size,
                    s.leader_count,
                    s.get_step_cost(Type::TRANSFER_LEADER_IN)
                ),
                (
                    row[1].as_i64().unwrap(),
                    row[2].as_i64().unwrap(),
                    row[3].as_i64().unwrap()
                )
            );
            assert_eq!((s.region_size, s.region_count, s.witness_count), (0, 0, 0));
            assert_eq!(
                s.step_cost.len(),
                usize::from(row[3].as_i64().unwrap() != 0)
            );
        }
    }
    let step = TransferLeader::default();
    for row in fixture()["timeouts"].as_array().unwrap() {
        assert_eq!(
            step.timeout(row[0].as_i64().unwrap()).as_nanos(),
            row[1].as_u64().unwrap() as u128
        );
    }
}

#[test]
fn task_018_role_and_signed_boundaries() {
    use astersql_pd_core::DownPeer;
    let cluster = BasicCluster::new();
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        ..Default::default()
    })));
    let mut cfg = ProviderSnapshot::<String>::default();
    cfg.schedule.max_store_down_time = "30m".parse().unwrap();
    let step = TransferLeader {
        from_store: 1,
        to_store: 2,
        to_stores: vec![99],
    };
    for role in [
        PeerRole::Voter,
        PeerRole::IncomingVoter,
        PeerRole::DemotingVoter,
        PeerRole::Unknown(9),
        PeerRole::Learner,
    ] {
        let mut p = peer(2);
        p.role = role;
        p.is_witness = true;
        let r = RegionInfo::new(
            RegionMeta {
                peers: vec![p.clone()],
                ..Default::default()
            },
            Some(peer(1)),
            vec![DownPeer {
                peer: p.clone(),
                down_seconds: 1000,
            }],
            vec![p],
        );
        let result = step.check_in_progress(&cluster, &cfg, &r, true, 0);
        if role == PeerRole::Learner {
            assert_eq!(
                result,
                Err(LeaderTransferError(vec![
                    LeaderCandidateError {
                        store_id: 99,
                        reason: LeaderTargetError::MissingPeer
                    },
                    LeaderCandidateError {
                        store_id: 2,
                        reason: LeaderTargetError::Learner
                    }
                ]))
            );
        } else {
            assert_eq!(result, Ok(()));
        }
    }
    // Configuration durations and elapsed time are signed, including future heartbeats.
    cfg.schedule.max_store_down_time = "-1ns".parse().unwrap();
    assert_eq!(
        step.check_in_progress(&cluster, &cfg, &region(1), false, 0)
            .unwrap_err()
            .0[1]
            .reason,
        LeaderTargetError::Down
    );
    assert!(
        step.check_in_progress(&cluster, &cfg, &region(1), false, -2)
            .is_ok()
    );
    for (size, from, to) in [(0, 0, 0), (-5, 5, -5), (i64::MIN, i64::MIN, i64::MIN)] {
        let mut r = region(1);
        r.stats_mut().approximate_size = size;
        let influence = OpInfluence::new();
        step.influence(&influence, &r);
        assert_eq!(influence.get_store_influence(1).leader_size, from);
        assert_eq!(influence.get_store_influence(2).leader_size, to);
    }
}
