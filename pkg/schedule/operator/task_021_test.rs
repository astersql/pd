// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{BatchSwitchWitness as BatchCommand, Operation, SwitchWitness};
use astersql_pd_storelimit::Type;
use std::sync::Arc;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_021.json"
    ))
    .unwrap()
}
fn peer(id: u64, store_id: u64, is_witness: bool, role: PeerRole) -> Peer {
    Peer {
        id,
        store_id,
        is_witness,
        role,
    }
}
fn region(peers: Vec<Peer>, pending: Vec<Peer>) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            peers,
            ..Default::default()
        },
        None,
        vec![],
        pending,
    )
}
fn steps() -> (BecomeWitness, BecomeNonWitness) {
    (
        BecomeWitness {
            peer_id: 12,
            store_id: 2,
        },
        BecomeNonWitness {
            peer_id: 12,
            store_id: 2,
            send_store: 3,
        },
    )
}
#[test]
fn task_021_behavior_cases() {
    let (w, n) = steps();
    for role in [
        PeerRole::Voter,
        PeerRole::Learner,
        PeerRole::IncomingVoter,
        PeerRole::DemotingVoter,
        PeerRole::Unknown(9),
    ] {
        for c in fixture()["completion"].as_array().unwrap() {
            let peers = c[1]
                .as_u64()
                .map(|id| vec![peer(id, 2, c[2].as_bool().unwrap(), role)])
                .unwrap_or_default();
            let pending = if c[3].as_bool().unwrap() {
                peers.clone()
            } else {
                vec![]
            };
            let r = region(peers, pending);
            assert_eq!(
                (w.is_finish(&r), w.conf_ver_changed(&r)),
                (c[4].as_bool().unwrap(), c[5].as_u64().unwrap()),
                "{c} {role:?}"
            );
            assert_eq!(
                (n.is_finish(&r), n.conf_ver_changed(&r)),
                (c[6].as_bool().unwrap(), c[7].as_u64().unwrap()),
                "{c} {role:?}"
            );
        }
    }
    // Go's nil-safe getters acknowledge an absent ID-zero nonwitness, but never finish it.
    let empty = region(vec![], vec![]);
    let zero = BecomeNonWitness::default();
    assert_eq!(
        (zero.conf_ver_changed(&empty), zero.is_finish(&empty)),
        (1, false)
    );
    let batch = BatchSwitchWitness {
        to_witnesses: vec![w.clone()],
        to_non_witnesses: vec![BecomeNonWitness {
            store_id: 3,
            peer_id: 13,
            send_store: 0,
        }],
    };
    for (witness, data, pending, conf, finished) in [
        (false, true, false, 0, false),
        (true, false, false, 0, false),
        (true, true, true, 2, false),
        (true, true, false, 2, true),
    ] {
        let a = peer(12, 2, witness, PeerRole::Learner);
        let b = peer(13, 3, !data, PeerRole::Voter);
        let r = region(vec![a, b.clone()], if pending { vec![b] } else { vec![] });
        assert_eq!(
            (batch.conf_ver_changed(&r), batch.is_finish(&r)),
            (conf, finished)
        );
    }
    assert_eq!(
        (
            BatchSwitchWitness::default().conf_ver_changed(&empty),
            BatchSwitchWitness::default().is_finish(&empty)
        ),
        (0, true)
    );
    assert_eq!(w.to_string(), "switch peer 12 on store 2 to witness");
    assert_eq!(n.to_string(), "switch peer 12 on store 2 to non-witness");
    assert_eq!(
        batch.to_string(),
        "batch switch witness, switch peer 12 on store 2 to witness, switch peer 13 on store 3 to non-witness"
    );
}
#[test]
fn task_021_commands_costs_timeouts() {
    let (w, n) = steps();
    let empty = region(vec![], vec![]);
    let batch = BatchSwitchWitness {
        to_witnesses: vec![w.clone(), w.clone()],
        to_non_witnesses: vec![n.clone()],
    };
    for v2 in [false, true] {
        let command = |switches| Operation {
            switch_witnesses: Some(BatchCommand {
                switch_witnesses: switches,
            }),
            ..Default::default()
        };
        let a = Some(SwitchWitness {
            peer_id: 12,
            is_witness: true,
        });
        let b = Some(SwitchWitness {
            peer_id: 12,
            is_witness: false,
        });
        assert_eq!(w.get_cmd(&empty, v2), command(vec![a.clone()]));
        assert_eq!(n.get_cmd(&empty, v2), command(vec![b.clone()]));
        assert_eq!(batch.get_cmd(&empty, v2), command(vec![a.clone(), a, b]));
        assert_eq!(
            BatchSwitchWitness::default().get_cmd(&empty, v2),
            command(vec![])
        );
    }
    for c in fixture()["costs"].as_array().unwrap() {
        let mut r = region(vec![peer(12, 2, true, PeerRole::Voter)], vec![]);
        let size = c[0].as_i64().unwrap();
        let cost = c[2].as_i64().unwrap();
        r.stats_mut().approximate_size = size;
        r.stats_mut().approximate_keys = c[1].as_i64().unwrap();
        let inf = OpInfluence::new();
        batch.influence(&inf, &r);
        let mut expected = StoreInfluence {
            region_size: -size,
            witness_count: 1,
            ..Default::default()
        };
        if cost != 0 {
            expected.add_step_cost(Type::REMOVE_PEER, 2 * cost);
            expected.add_step_cost(Type::ADD_PEER, cost);
        }
        assert_eq!(inf.get_store_influence(2), expected, "{c}");
        let mut sender = StoreInfluence::default();
        sender.add_step_cost(Type::SEND_SNAPSHOT, size);
        assert_eq!(inf.get_store_influence(3), sender);
        // Snapshot source can coincide with target; updates must accumulate without reentrant locking.
        let inf = OpInfluence::new();
        BecomeNonWitness {
            send_store: 2,
            ..n.clone()
        }
        .influence(&inf, &r);
        assert_eq!(inf.snapshot().len(), 1);
        assert_eq!(
            inf.get_store_influence(2)
                .get_step_cost(Type::SEND_SNAPSHOT),
            size
        );
        let inf = OpInfluence::new();
        BecomeNonWitness {
            send_store: 0,
            ..n.clone()
        }
        .influence(&inf, &r);
        assert_eq!(inf.snapshot().len(), 1);
    }
    for c in fixture()["timeouts"].as_array().unwrap() {
        let size = c[0].as_i64().unwrap();
        assert_eq!(w.timeout(size).as_secs(), c[1].as_u64().unwrap());
        assert_eq!(n.timeout(size).as_secs(), c[2].as_u64().unwrap());
        assert_eq!(
            batch.timeout(size).as_nanos(),
            n.timeout(size).as_nanos() * 2
        );
        assert_eq!(BatchSwitchWitness::default().timeout(size), n.timeout(size));
    }
    assert_eq!(
        batch.timeout_nanos(1_000_000_000),
        -6_446_744_073_709_551_616
    );
    let mut r = empty;
    r.stats_mut().approximate_size = 1;
    let inf = OpInfluence::new();
    inf.update_store(2, |s| {
        s.region_size = i64::MIN;
        s.witness_count = i64::MAX;
    });
    w.influence(&inf, &r);
    assert_eq!(inf.get_store_influence(2).region_size, i64::MAX);
    assert_eq!(inf.get_store_influence(2).witness_count, i64::MIN);
    n.influence(&inf, &r);
    assert_eq!(inf.get_store_influence(2).region_size, i64::MIN);
    assert_eq!(inf.get_store_influence(2).witness_count, i64::MAX);
}
#[test]
fn task_021_store_faults() {
    let (w, n) = steps();
    let mut cfg = ProviderSnapshot::<String>::default();
    cfg.schedule.max_store_down_time = "30m".parse().unwrap();
    let now = 2_000_000_000_000i128;
    for (elapsed, health, w_error, n_error) in [
        (0, false, None, None),
        (
            601_000_000_000,
            true,
            None,
            Some(LeaderTargetError::Unhealthy),
        ),
        (601_000_000_000, false, None, None),
        (
            1_900_000_000_000,
            false,
            Some(LeaderTargetError::Down),
            Some(LeaderTargetError::Down),
        ),
        (
            1_900_000_000_000,
            true,
            Some(LeaderTargetError::Down),
            Some(LeaderTargetError::Unhealthy),
        ),
    ] {
        let cluster = BasicCluster::new();
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 2,
            last_heartbeat: now as i64 - elapsed,
            ..Default::default()
        })));
        let r = region(vec![peer(12, 2, false, PeerRole::Learner)], vec![]);
        assert_eq!(
            w.check_in_progress(&cluster, &cfg, &r, health, now),
            w_error.map_or(Ok(()), |e| Err(PeerStepError::Store(e)))
        );
        assert_eq!(
            n.check_in_progress(&cluster, &cfg, &r, health, now),
            n_error.map_or(Ok(()), |e| Err(PeerStepError::Store(e)))
        );
    }
    let cluster = BasicCluster::new();
    let empty = region(vec![], vec![]);
    assert_eq!(
        w.check_in_progress(&cluster, &cfg, &empty, false, now),
        Err(PeerStepError::Store(LeaderTargetError::MissingStore))
    );
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        last_heartbeat: now as i64,
        ..Default::default()
    })));
    for peers in [vec![], vec![peer(99, 2, true, PeerRole::Voter)]] {
        let r = region(peers, vec![]);
        assert_eq!(
            w.check_in_progress(&cluster, &cfg, &r, true, now),
            Err(PeerStepError::MissingPeer)
        );
        assert_eq!(
            n.check_in_progress(&cluster, &cfg, &r, true, now),
            Err(PeerStepError::MissingPeer)
        );
    }
    let batch = BatchSwitchWitness {
        to_witnesses: vec![w],
        to_non_witnesses: vec![BecomeNonWitness { store_id: 3, ..n }],
    };
    assert_eq!(
        batch.check_in_progress(&cluster, &cfg, &empty, false, now),
        Err(PeerStepError::MissingPeer)
    );
    let r = region(vec![peer(12, 2, true, PeerRole::Voter)], vec![]);
    assert_eq!(
        batch.check_in_progress(&cluster, &cfg, &r, false, now),
        Err(PeerStepError::Store(LeaderTargetError::MissingStore))
    );
    assert!(
        BatchSwitchWitness::default()
            .check_in_progress(&cluster, &cfg, &empty, true, now)
            .is_ok()
    );
}
