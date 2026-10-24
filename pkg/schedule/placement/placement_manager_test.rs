// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{KeyRange, Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
async fn manager() -> (RuleManager, ProviderSnapshot<String>) {
    let conf = ProviderSnapshot::default();
    let mut m = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(30),
    );
    m.initialize(1, &[], "", false, &conf, &[]).await.unwrap();
    (m, conf)
}
fn rule(group: &str, id: &str, start: &str, end: &str) -> Rule {
    Rule {
        group_id: group.into(),
        id: id.into(),
        start_key_hex: start.into(),
        end_key_hex: end.into(),
        role: PeerRoleType::Voter,
        count: 1,
        ..Default::default()
    }
}
fn keys(r: &[Rule]) -> Vec<(&str, &str)> {
    r.iter()
        .map(|r| (r.group_id.as_str(), r.id.as_str()))
        .collect()
}
#[tokio::test]
async fn placement_manager_priority_and_reconcile_snapshots() {
    let (mut m, c) = manager().await;
    m.set_rules(
        vec![rule("a", "z", "", ""), rule("a", "b", "", "")],
        2,
        &c,
        &[],
    )
    .await
    .unwrap();
    m.set_rule_group(
        RuleGroup {
            id: "a".into(),
            index: 100,
            override_: true,
        },
        3,
    )
    .await
    .unwrap();
    let mut r = m.get_rule("a", "z").unwrap();
    r.index = -1;
    m.set_rule(r, 4, &c, &[]).await.unwrap();
    assert_eq!(
        keys(&m.get_all_rules()),
        [("a", "z"), ("a", "b"), ("pd", "default")]
    );
    assert_eq!(keys(&m.get_rules_by_group("a")), [("a", "z"), ("a", "b")]);
    // Go GetAllRules clones before sorting; GetRulesByKey clones after sorting.
    assert!(m.get_all_rules().iter().all(|r| r.group.is_none()));
    assert_eq!(
        keys(&m.get_rules_by_key(b"")),
        [("pd", "default"), ("a", "z"), ("a", "b")]
    );
    let (rules, groups) = m.get_rule_config_for_reconcile();
    assert_eq!(
        rules.iter().map(|r| r.key()).collect::<Vec<_>>(),
        [
            ["a".to_string(), "b".to_string()],
            ["a".into(), "z".into()],
            ["pd".into(), "default".into()]
        ]
    );
    assert_eq!(
        groups,
        [RuleGroup {
            id: "a".into(),
            index: 100,
            override_: true
        }]
    );
    assert_eq!(m.get_rules_count(), 3);
    assert_eq!(m.get_groups_count(), 2);
    assert_eq!(
        m.get_rule_groups()
            .iter()
            .map(|g| g.id.as_str())
            .collect::<Vec<_>>(),
        ["pd", "a"]
    );
    assert_eq!(
        m.get_all_group_bundles()[1].rules.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .id,
        "z"
    );
    assert_eq!(
        m.get_group_bundle("a").rules.unwrap()[0]
            .as_ref()
            .unwrap()
            .group_index(),
        100
    );
    assert!(m.get_group_bundle("absent").rules.is_none());
    assert!(m.get_rule_group("absent").is_none());
    let mut copied = m.get_rules_by_key(b"");
    copied[0].count = 99;
    assert_eq!(m.get_rule("pd", "default").unwrap().count, 1);
    m.delete_rule("a", "b", 5).await.unwrap();
    assert_eq!(rules[0].id, "b");
    assert_eq!(rules[0].count, 1);
    assert_eq!(m.get_rule_config_for_reconcile().0.len(), 2);
}
#[tokio::test]
async fn placement_manager_go_keys_and_boundaries() {
    let (mut m, c) = manager().await;
    m.set_rules(
        vec![
            rule("1", "1", "", ""),
            rule("2", "2", "11", "ff"),
            rule("2", "3", "22", "dd"),
            rule("3", "4", "44", "ee"),
            rule("3", "5", "44", "dd"),
        ],
        2,
        &c,
        &[],
    )
    .await
    .unwrap();
    m.delete_rule("pd", "default", 3).await.unwrap();
    for (s, e, want) in [
        (0, 0, vec![0x11, 0x22, 0x44, 0xdd, 0xee, 0xff]),
        (0x44, 0, vec![0xdd, 0xee, 0xff]),
        (0x44, 0xdd, vec![]),
        (0x22, 0xef, vec![0x44, 0xdd, 0xee]),
    ] {
        let start = if s == 0 { vec![] } else { vec![s] };
        let end = if e == 0 { vec![] } else { vec![e] };
        assert_eq!(
            m.get_split_keys(&start, &end),
            want.into_iter().map(|x| vec![x]).collect::<Vec<_>>()
        );
    }
    assert_eq!(
        keys(&m.get_rules_by_key(&[0x33])),
        [("1", "1"), ("2", "2"), ("2", "3")]
    );
    assert_eq!(
        keys(&m.get_rules_by_key(&[0xdd])),
        [("1", "1"), ("2", "2"), ("3", "4")]
    );
    assert_eq!(keys(&m.get_rules_by_group("3")), [("3", "4"), ("3", "5")]);
    assert!(m.get_rules_by_group("4").is_empty());
    for (s, e, n) in [
        (vec![], vec![], 0),
        (vec![0xaa], vec![0xbb], 5),
        (vec![0x11], vec![0x22], 2),
        (vec![0x11], vec![0x33], 0),
    ] {
        let region = RegionInfo::new(
            RegionMeta {
                key_range: KeyRange {
                    start_key: s,
                    end_key: e,
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        assert_eq!(m.get_rules_for_apply_region(&region).len(), n);
        assert_eq!(
            m.get_rules_for_apply_range(
                &region.meta().key_range.start_key,
                &region.meta().key_range.end_key
            )
            .len(),
            n
        );
    }
}
// Compare every fit output field, including immutable metadata and candidate inputs.
fn same_fit(a: &RegionFit, b: &RegionFit) {
    assert_eq!(a.is_satisfied(), b.is_satisfied());
    assert_eq!(a.orphan_peers, b.orphan_peers);
    assert_eq!(a.rules, b.rules);
    assert_eq!(
        a.region_stores.iter().map(|s| s.id()).collect::<Vec<_>>(),
        b.region_stores.iter().map(|s| s.id()).collect::<Vec<_>>()
    );
    assert_eq!(a.rule_fits.len(), b.rule_fits.len());
    for (a, b) in a.rule_fits.iter().zip(&b.rule_fits) {
        assert_eq!(a.rule, b.rule);
        assert_eq!(a.peers, b.peers);
        assert_eq!(a.peers_with_different_role, b.peers_with_different_role);
        assert_eq!(a.isolation_score, b.isolation_score);
        assert_eq!(a.witness_score, b.witness_score);
        assert_eq!(
            a.stores.iter().map(|s| s.id()).collect::<Vec<_>>(),
            b.stores.iter().map(|s| s.id()).collect::<Vec<_>>()
        );
    }
}
#[tokio::test]
async fn placement_manager_query_fit_cache_and_mutation() {
    let (mut m, mut c) = manager().await;
    c.replication.enable_placement_rules_cache = true;
    let mut bounded = rule("special", "one", "11", "ff");
    bounded.override_ = true;
    m.set_rule(bounded, 2, &c, &[]).await.unwrap();
    m.set_rule_group(
        RuleGroup {
            id: "special".into(),
            index: 1,
            override_: true,
        },
        3,
    )
    .await
    .unwrap();
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = RegionInfo::new(
        RegionMeta {
            id: 30,
            key_range: KeyRange {
                start_key: vec![0x22],
                end_key: vec![0xdd],
            },
            epoch: Some(RegionEpoch::default()),
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    let stores = vec![Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        last_heartbeat: 1,
        ..Default::default()
    }))];
    assert_eq!(
        keys(&m.get_rules_by_key(&[0x22])),
        [("pd", "default"), ("special", "one")]
    );
    let selected = m.get_rules_for_apply_region(&region);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "one");
    let direct = m.fit_region_without_cache(&stores, &region, &c);
    assert!(direct.is_satisfied());
    assert_eq!(direct.rule_fits[0].peers, region.meta().peers);
    assert_eq!(direct.rule_fits[0].isolation_score, 0.0);
    assert_eq!(direct.rule_fits[0].witness_score, 0);
    assert!(direct.rule_fits[0].peers_with_different_role.is_empty());
    assert!(direct.orphan_peers.is_empty());
    assert_eq!(direct.rules, selected);
    let first = m.fit_region(&stores, &region, &c, 1);
    same_fit(&first, &direct);
    m.set_region_fit_cache(&stores, &region, first, 1);
    for _ in 0..10 {
        same_fit(&m.fit_region(&stores, &region, &c, 1), &direct);
    }
    let cached = m.fit_region(&stores, &region, &c, 1);
    assert!(Arc::ptr_eq(&cached, &m.fit_region(&stores, &region, &c, 1)));
    c.replication.enable_placement_rules_cache = false;
    let disabled = m.fit_region(&stores, &region, &c, 1);
    same_fit(&disabled, &cached);
    assert!(!Arc::ptr_eq(&cached, &disabled));
    let old = m.get_rule_config_for_reconcile();
    let mut updated = m.get_rule("special", "one").unwrap();
    updated.count = 2;
    m.set_rule(updated, 4, &c, &[]).await.unwrap();
    assert_eq!(
        old.0
            .iter()
            .find(|r| r.group_id == "special")
            .unwrap()
            .count,
        1
    );
    c.replication.enable_placement_rules_cache = true;
    assert!(!m.is_region_fit_cached(&stores, &region, 1));
    let changed = m.fit_region(&stores, &region, &c, 1);
    same_fit(&changed, &m.fit_region_without_cache(&stores, &region, &c));
    assert!(!changed.is_satisfied());
    assert_eq!(changed.rule_fits[0].rule.count, 2);
    assert!(!m.may_restrict_store_load(true));
    let mut constrained = m.get_rule("special", "one").unwrap();
    constrained.label_constraints = vec![LabelConstraint {
        key: "zone".into(),
        op: LabelConstraintOp::In,
        values: vec!["a".into()],
    }];
    m.set_rule(constrained, 5, &c, &[]).await.unwrap();
    assert!(m.may_restrict_store_load(true));
    assert!(!m.may_restrict_store_load(false));
    let rejected = m.fit_region(&stores, &region, &c, 1);
    assert!(rejected.rule_fits[0].peers.is_empty());
    assert_eq!(rejected.orphan_peers, region.meta().peers);
    m.delete_rule_group("special", 6).await.unwrap();
    assert!(m.get_rule_config_for_reconcile().1.is_empty());
    assert_eq!(m.get_rules_for_apply_region(&region).len(), 2);
}
#[tokio::test]
async fn placement_manager_empty_explicit_group_bundle_keeps_null_rules() {
    let (mut m, _) = manager().await;
    m.set_rule_group(
        RuleGroup {
            id: "empty".into(),
            index: 1,
            override_: false,
        },
        2,
    )
    .await
    .unwrap();
    let bundle = m.get_group_bundle("empty");
    assert_eq!(bundle.index, 1);
    assert!(
        bundle.rules.is_none(),
        "Go leaves Rules nil when an explicit group contains no rules"
    );
    assert!(
        m.get_all_group_bundles()
            .iter()
            .find(|b| b.id == "empty")
            .unwrap()
            .rules
            .is_none()
    );
    assert_eq!(m.get_rule_config_for_reconcile().1.len(), 1);
}
