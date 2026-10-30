// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, PeerRole, RegionInfo, RegionMeta, StoresInfo};
use astersql_pd_schedule_config::ProviderSnapshot;
fn peer(store_id: u64, learner: bool) -> Peer {
    Peer {
        id: store_id,
        store_id,
        role: if learner {
            PeerRole::Learner
        } else {
            PeerRole::Voter
        },
        ..Default::default()
    }
}
#[test]
fn task_062_builder_brief_precedence() {
    let stores = StoresInfo::new();
    let config = ProviderSnapshot::<()>::default();
    let region = RegionInfo::new(
        RegionMeta {
            id: 62,
            peers: vec![peer(1, false), peer(2, true)],
            ..Default::default()
        },
        Some(peer(1, false)),
        vec![],
        vec![],
    );
    for (target, joint, light, leader, expected) in [
        (
            vec![peer(1, false), peer(2, true), peer(3, false)],
            false,
            false,
            1,
            "add peer: store [3]",
        ),
        (vec![peer(1, false)], false, false, 1, "rm peer: store [2]"),
        (
            vec![peer(1, false), peer(2, false)],
            false,
            false,
            1,
            "promote peer: store [2]",
        ),
        (
            vec![peer(1, true), peer(2, true)],
            true,
            false,
            1,
            "demote peer: store [1]",
        ),
        (
            vec![peer(1, true), peer(2, true)],
            false,
            false,
            1,
            "mv peer: store [1] to [1]",
        ),
        (
            vec![peer(1, false), peer(3, false)],
            false,
            true,
            1,
            "mv light peer: store [2] to [3]",
        ),
        (
            vec![peer(2, false)],
            true,
            false,
            2,
            "promote peer: store [1], rm peer: store [2]",
        ),
        (
            vec![peer(1, true)],
            true,
            false,
            1,
            "demote peer: store [1], rm peer: store [2]",
        ),
        (
            vec![peer(1, false), peer(2, true)],
            false,
            false,
            2,
            "transfer leader: store 1 to 2",
        ),
    ] {
        let mut b = Builder::new(
            "brief",
            BuilderContext {
                stores: &stores,
                config: &config,
                rules: &[],
                now_unix_nanos: 0,
            },
            &region,
            BuilderOptions {
                skip_placement_rules_check: true,
                ..Default::default()
            },
        );
        b.target_peers = target.into_iter().map(|p| (p.store_id, p)).collect();
        b.target_leader = leader;
        assert_eq!(
            b.operator_brief(
                SimpleBuildOptions {
                    add_light_peer: light,
                    remove_light_peer: light
                },
                joint
            ),
            expected
        );
    }
}

#[test]
fn task_062_witness_restore_brief() {
    let stores = StoresInfo::new();
    let config = ProviderSnapshot::<()>::default();
    let mut old = peer(1, false);
    old.is_witness = true;
    let region = RegionInfo::new(
        RegionMeta {
            id: 63,
            peers: vec![old.clone()],
            ..Default::default()
        },
        Some(old),
        vec![],
        vec![],
    );
    let mut b = Builder::new(
        "brief",
        BuilderContext {
            stores: &stores,
            config: &config,
            rules: &[],
            now_unix_nanos: 0,
        },
        &region,
        BuilderOptions {
            skip_placement_rules_check: true,
            ..Default::default()
        },
    );
    b.target_peers.insert(1, peer(1, false));
    assert_eq!(
        b.operator_brief(SimpleBuildOptions::default(), false),
        "mv peer: store [1] to [1]"
    );
    assert_eq!(
        b.operator_brief(SimpleBuildOptions::default(), true),
        "demote peer: store [1]"
    );
}
