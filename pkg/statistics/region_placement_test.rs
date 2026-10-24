// Copyright 2026 AsterSQL.
use crate::{RegionStatisticType::*, RegionStatistics};
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionInfo, RegionMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;

fn config() -> ProviderSnapshot<()> {
    let mut c = ProviderSnapshot::default();
    c.replication.max_replicas = 3;
    c.replication.enable_placement_rules = true;
    c
}
fn manager() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(16),
    )
}
async fn initialized(c: &ProviderSnapshot<()>) -> RuleManager {
    let mut m = manager();
    m.initialize(3, &[], "", false, c, &[]).await.unwrap();
    m
}
fn region(id: u64, start: &[u8], end: &[u8], voters: usize, learners: usize) -> RegionInfo {
    let peers: Vec<_> = (0..voters + learners)
        .map(|i| Peer {
            id: i as u64 + 1,
            store_id: i as u64 + 1,
            role: if i < voters {
                PeerRole::Voter
            } else {
                PeerRole::Learner
            },
            is_witness: false,
        })
        .collect();
    RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.to_vec(),
                end_key: end.to_vec(),
            },
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        vec![],
        vec![],
    )
}
#[tokio::test]
async fn state_task_16_region_placement_go_sequence() {
    let c = config();
    let m = initialized(&c).await;
    let mut s = RegionStatistics::new();
    let peers = vec![
        Peer {
            id: 5,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: 6,
            store_id: 2,
            ..Default::default()
        },
        Peer {
            id: 4,
            store_id: 3,
            ..Default::default()
        },
        Peer {
            id: 8,
            store_id: 7,
            role: PeerRole::Learner,
            ..Default::default()
        },
        Peer {
            id: 9,
            store_id: 8,
            is_witness: true,
            ..Default::default()
        },
    ];
    for (id, start, end, ps, leader) in [
        (0, b"aa", b"bb", peers[..1].to_vec(), peers[0].clone()),
        (1, b"ee", b"ff", peers.clone(), peers[0].clone()),
        (2, b"gg", b"hh", peers[..3].to_vec(), peers[0].clone()),
        (0, b"aa", b"bb", peers[2..].to_vec(), peers[4].clone()),
    ] {
        let r = RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start.to_vec(),
                    end_key: end.to_vec(),
                },
                peers: ps,
                ..Default::default()
            },
            Some(leader),
            vec![],
            vec![],
        );
        assert!(s.observe_with_placement(&r, &[], &c, &m, 100).is_some());
        match (id, r.meta().peers.len()) {
            (0, 1) => assert_eq!(s.region_ids_by_type(MissPeer), [0]),
            (1, 5) => assert_eq!(s.region_ids_by_type(ExtraPeer), [1]),
            (2, 3) => {
                assert_eq!(s.region_ids_by_type(MissPeer), [0]);
                assert_eq!(s.region_ids_by_type(ExtraPeer), [1]);
            }
            _ => {
                assert_eq!(s.region_ids_by_type(WitnessLeader), [0]);
                assert!(s.region_ids_by_type(MissPeer).is_empty());
            }
        }
    }
}
#[tokio::test]
async fn state_task_16_region_placement_learner_and_voter_duration() {
    let c = config();
    let mut m = initialized(&c).await;
    m.set_rule(
        Rule {
            group_id: "pd".into(),
            id: "learners".into(),
            role: PeerRoleType::Learner,
            count: 2,
            ..Default::default()
        },
        1,
        &c,
        &[],
    )
    .await
    .unwrap();
    let mut s = RegionStatistics::new();
    let r = region(1, b"a", b"b", 3, 0);
    for t in [100, 110] {
        let d = s.observe_with_placement(&r, &[], &c, &m, t).unwrap();
        assert!(s.is_region_stats_type(1, MissPeer));
        assert_eq!(d.miss_voter_seconds, None); // only learners missing
    }
    let r = region(1, b"a", b"b", 2, 1);
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 120)
            .unwrap()
            .miss_voter_seconds,
        None
    );
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 127)
            .unwrap()
            .miss_voter_seconds,
        Some(7)
    );
    let r = region(1, b"a", b"b", 3, 2);
    s.observe_with_placement(&r, &[], &c, &m, 130).unwrap();
    assert!(!s.is_region_stats_type(1, MissPeer));
    assert!(!s.is_region_stats_type(1, ExtraPeer));
    assert!(s.is_region_stats_type(1, LearnerPeer));
    let r = region(1, b"a", b"b", 2, 1);
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 140)
            .unwrap()
            .miss_voter_seconds,
        None
    );
}
#[tokio::test]
async fn state_task_16_region_placement_boundary_and_rule_update() {
    let c = config();
    let mut m = initialized(&c).await;
    let mut rule = Rule {
        group_id: "pd".into(),
        id: "local".into(),
        index: 10,
        override_: true,
        start_key_hex: "6d".into(),
        end_key_hex: "7a".into(),
        role: PeerRoleType::Voter,
        count: 1,
        ..Default::default()
    };
    m.set_rule(rule.clone(), 1, &c, &[]).await.unwrap();
    let mut s = RegionStatistics::new();
    for (id, a, b, miss, extra) in [
        (1, b"a".as_slice(), b"m".as_slice(), true, false),
        (2, b"m", b"z", false, false),
        (3, b"l", b"n", false, true), // crosses boundary: no applicable rules
        (4, b"z", b"", true, false),
    ] {
        s.observe_with_placement(&region(id, a, b, 1, 0), &[], &c, &m, 100)
            .unwrap();
        assert_eq!(s.is_region_stats_type(id, MissPeer), miss);
        assert_eq!(s.is_region_stats_type(id, ExtraPeer), extra);
    }
    let r = region(2, b"m", b"z", 1, 0);
    rule.count = 2;
    m.set_rule(rule, 2, &c, &[]).await.unwrap();
    assert!(!s.region_stats_need_update(&r, &c)); // Go trigger subset unchanged
    s.observe_with_placement(&r, &[], &c, &m, 110).unwrap();
    assert!(s.is_region_stats_type(2, MissPeer));
}
#[tokio::test]
async fn state_task_16_region_placement_uninitialized_and_config_update() {
    let mut c = config();
    let m = manager();
    let r = region(1, b"a", b"b", 2, 0);
    let mut s = RegionStatistics::new();
    assert_eq!(s.observe_with_placement(&r, &[], &c, &m, 90), None);
    assert!(!s.is_observed(1));
    assert!(s.region_stats_need_update(&r, &c));
    c.replication.enable_placement_rules = false;
    assert!(s.observe_with_placement(&r, &[], &c, &m, 100).is_some());
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 107)
            .unwrap()
            .miss_voter_seconds,
        Some(7)
    );
    let before = s.bitmap(1);
    c.replication.enable_placement_rules = true;
    assert_eq!(
        s.observe_with_placement(&region(1, b"a", b"b", 5, 0), &[], &c, &m, 110),
        None
    );
    assert_eq!(s.bitmap(1), before);
    let m = initialized(&c).await;
    c.replication.max_replicas = 1; // ignored while placement enabled
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 115)
            .unwrap()
            .miss_voter_seconds,
        Some(15)
    );
    assert!(s.is_region_stats_type(1, MissPeer));
    c.replication.enable_placement_rules = false;
    s.observe_with_placement(&r, &[], &c, &m, 120).unwrap();
    assert!(!s.is_region_stats_type(1, MissPeer));
    assert!(s.is_region_stats_type(1, ExtraPeer));
}

#[tokio::test]
async fn state_task_16_region_placement_nonlearner_roles_and_empty_rules() {
    let c = config();
    let mut m = initialized(&c).await;
    for (id, role, count) in [
        ("leader", PeerRoleType::Leader, 1),
        ("follower", PeerRoleType::Follower, 2),
        ("learner", PeerRoleType::Learner, 2),
    ] {
        m.set_rule(
            Rule {
                group_id: "pd".into(),
                id: id.into(),
                role,
                count,
                ..Default::default()
            },
            1,
            &c,
            &[],
        )
        .await
        .unwrap();
    }
    let mut s = RegionStatistics::new();
    // 3 voters + 1 leader + 2 followers + 2 learners: total8, voter6.
    let r = region(1, b"a", b"b", 5, 2);
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 100)
            .unwrap()
            .miss_voter_seconds,
        None
    );
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 107)
            .unwrap()
            .miss_voter_seconds,
        Some(7)
    );
    // Missing only learner pauses sampling but preserves the timestamp while MissPeer remains.
    assert_eq!(
        s.observe_with_placement(&region(1, b"a", b"b", 6, 1), &[], &c, &m, 110)
            .unwrap()
            .miss_voter_seconds,
        None
    );
    assert_eq!(
        s.observe_with_placement(&r, &[], &c, &m, 115)
            .unwrap()
            .miss_voter_seconds,
        Some(15)
    );
    let mut empty = manager();
    empty.initialize(3, &[], "", true, &c, &[]).await.unwrap();
    assert!(empty.is_initialized());
    s.observe_with_placement(&r, &[], &c, &empty, 120).unwrap();
    assert!(s.is_region_stats_type(1, ExtraPeer));
    assert!(!s.is_region_stats_type(1, MissPeer));
}
