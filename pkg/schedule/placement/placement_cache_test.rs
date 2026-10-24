// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use std::sync::Arc;
fn input() -> (RegionInfo, Vec<Arc<StoreInfo>>, Vec<Arc<Rule>>) {
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    (
        RegionInfo::new(
            RegionMeta {
                id: 1,
                epoch: Some(RegionEpoch::default()),
                peers: vec![peer.clone()],
                ..Default::default()
            },
            Some(peer),
            vec![],
            vec![],
        ),
        vec![Arc::new(StoreInfo::new(StoreMeta {
            id: 1,
            last_heartbeat: 1,
            ..Default::default()
        }))],
        vec![Arc::new(Rule {
            id: "default".into(),
            group_id: "pd".into(),
            count: 1,
            role: PeerRoleType::Voter,
            ..Default::default()
        })],
    )
}
#[test]
fn placement_cache_promotion_invalidation_and_stale_store() {
    let (r, s, rules) = input();
    let f = Arc::new(fit_region(&s, &r, &rules, false));
    let mut c = RegionRuleFitCacheManager::new();
    c.set_cache(&s, &r, f.clone(), 1);
    assert!(c.check_and_get_cache(&r, &rules, &s, 1).0);
    assert!(c.check_and_get_cache(&r, &rules, &s, 1).1.is_none());
    for _ in 0..9 {
        c.set_cache(&s, &r, f.clone(), 1);
    }
    assert!(c.check_and_get_cache(&r, &rules, &s, 1).1.is_none());
    c.set_cache(&s, &r, f.clone(), 1);
    assert!(Arc::ptr_eq(
        &c.check_and_get_cache(&r, &rules, &s, 1).1.unwrap(),
        &f
    ));
    c.remove_store_cache(1);
    assert!(!c.check_and_get_cache(&r, &rules, &s, 1).0);
    c.set_cache(&[], &r, f.clone(), 1);
    assert!(!c.check_and_get_cache(&r, &rules, &s, 1).0);
    c.set_cache(&s, &r, f.clone(), 1);
    for _ in 0..9 {
        c.set_cache(&s, &r, f.clone(), 1);
    }
    c.set_cache(&[], &r, f, 1);
    assert!(!c.check_and_get_cache(&r, &rules, &s, 1).0);
}
#[test]
fn placement_cache_changes_and_eligibility() {
    let (r, s, rules) = input();
    let mut c = RegionRuleFitCacheManager::new();
    c.set_cache(&s, &r, Arc::new(fit_region(&s, &r, &rules, false)), 1);
    let check = |r: &RegionInfo, rules: &[Arc<Rule>], s: &[Arc<StoreInfo>], now| {
        c.check_and_get_cache(r, rules, s, now).0
    };
    assert!(check(&r, &rules, &s, 1));
    for change in 0..4 {
        let mut meta = r.meta().clone();
        let mut leader = r.leader().cloned();
        match change {
            0 => meta.epoch.as_mut().unwrap().version += 1,
            1 => meta.epoch.as_mut().unwrap().conf_ver += 1,
            2 => leader.as_mut().unwrap().store_id = 2,
            _ => meta.epoch = None,
        }
        let changed = RegionInfo::new(meta, leader, vec![], vec![]);
        assert!(!check(&changed, &rules, &s, 1));
    }
    let no_leader = RegionInfo::new(r.meta().clone(), None, vec![], vec![]);
    assert!(!check(&no_leader, &rules, &s, 1));
    let down = RegionInfo::new(
        r.meta().clone(),
        r.leader().cloned(),
        vec![astersql_pd_core::DownPeer {
            peer: r.leader().unwrap().clone(),
            down_seconds: 42,
        }],
        vec![],
    );
    assert!(!check(&down, &rules, &s, 1));
    for change in 0..4 {
        let mut rr = rules[0].as_ref().clone();
        match change {
            0 => rr.version += 1,
            1 => rr.create_timestamp += 1,
            2 => rr.id.push('x'),
            _ => rr.group_id.push('x'),
        }
        assert!(!check(&r, &[Arc::new(rr)], &s, 1));
    }
    assert!(!check(&r, &[], &s, 1));
    assert!(!check(&r, &rules, &[], 1));
    assert!(!check(&r, &rules, &s, 20_000_000_002));
    for change in 0..3 {
        let mut meta = s[0].meta().unwrap().clone();
        match change {
            0 => meta.labels.push(astersql_pd_core::StoreLabel {
                key: "zone".into(),
                value: "x".into(),
            }),
            1 => meta.state = astersql_pd_core::StoreState::Unknown(4),
            _ => meta.node_state = astersql_pd_core::NodeState::Removing,
        }
        assert!(!check(&r, &rules, &[Arc::new(StoreInfo::new(meta))], 1));
    }
    c.invalid(1);
    assert!(!c.check_and_get_cache(&r, &rules, &s, 1).0);
    let bad = Arc::new(fit_region(&[], &r, &rules, false));
    c.set_cache(&s, &r, bad, 1);
    assert!(!c.check_and_get_cache(&r, &rules, &s, 1).0);
}
#[tokio::test]
async fn placement_cache_manager_matches_uncached() {
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    let (r, s, _) = input();
    let mut conf = ProviderSnapshot::<String>::default();
    conf.replication.enable_placement_rules_cache = true;
    let mut m = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(29),
    );
    m.initialize(1, &[], "", false, &conf, &[]).await.unwrap();
    let f = m.fit_region(&s, &r, &conf, 1);
    assert!(!m.is_region_fit_cached(&s, &r, 1));
    m.set_region_fit_cache(&s, &r, f, 1);
    for _ in 0..10 {
        m.fit_region(&s, &r, &conf, 1);
    }
    let cached = m.fit_region(&s, &r, &conf, 1);
    assert!(Arc::ptr_eq(&cached, &m.fit_region(&s, &r, &conf, 1)));
    let direct = m.fit_region_without_cache(&s, &r, &conf);
    assert_eq!(cached.rule_fits[0].peers, direct.rule_fits[0].peers);
    assert_eq!(cached.is_satisfied(), direct.is_satisfied());
    assert_eq!(cached.orphan_peers, direct.orphan_peers);
    conf.replication.enable_placement_rules_cache = false;
    assert!(!Arc::ptr_eq(&cached, &m.fit_region(&s, &r, &conf, 1)));
    conf.replication.enable_placement_rules_cache = true;
    m.remove_store_cache(1);
    assert!(!m.is_region_fit_cached(&s, &r, 1));
    let mut updated = m.get_rule("pd", "default").unwrap();
    updated.count = 2;
    m.set_rule(updated, 2, &conf, &[]).await.unwrap();
    assert!(!m.fit_region(&s, &r, &conf, 1).is_satisfied());
}
