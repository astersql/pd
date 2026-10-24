// Copyright 2026 AsterSQL.
use crate::{LabelConstraint, LabelConstraintOp, PeerRoleType, Rule, fit_region};
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta};
use std::sync::Arc;
fn store(id: u64) -> Arc<StoreInfo> {
    Arc::new(StoreInfo::new(StoreMeta {
        id,
        labels: [
            ("zone", id / 1000),
            ("rack", id / 100 % 10),
            ("host", id / 10 % 10),
        ]
        .into_iter()
        .map(|(k, v)| StoreLabel {
            key: k.into(),
            value: format!("{k}{v}"),
        })
        .collect(),
        ..Default::default()
    }))
}
fn rule(count: i64, role: PeerRoleType, constraint: Option<(&str, &[&str])>) -> Arc<Rule> {
    Arc::new(Rule {
        count,
        role,
        location_labels: vec!["zone".into(), "rack".into(), "host".into()],
        label_constraints: constraint
            .map(|(k, v)| {
                vec![LabelConstraint {
                    key: k.into(),
                    op: LabelConstraintOp::In,
                    values: v.iter().map(|s| s.to_string()).collect(),
                }]
            })
            .unwrap_or_default(),
        ..Default::default()
    })
}
fn region(ids: &[u64]) -> RegionInfo {
    let peers: Vec<_> = ids
        .iter()
        .map(|id| Peer {
            id: *id + 10000,
            store_id: *id,
            ..Default::default()
        })
        .collect();
    RegionInfo::new(
        RegionMeta {
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        vec![],
        vec![],
    )
}
#[test]
fn placement_isolation_replace_go_test_cases() {
    let stores: Vec<_> = [1111, 2111, 3111].map(store).into();
    let f = fit_region(
        &stores,
        &region(&[1111, 2111, 3111]),
        &[rule(3, PeerRoleType::Voter, None)],
        false,
    );
    assert!(
        f.replace(1111, Some(&store(4111))),
        "Go TestReplace: preserve zone diversity"
    );
    assert!(!f.replace(1111, Some(&store(2113))));
    assert!(f.replace(1111, Some(&stores[1])));
    assert!(!f.replace(9999, Some(&stores[1])));
    assert!(!f.replace(1111, None));
    assert_eq!(f.rule_fits[0].isolation_score, 30000.0);
    let restricted = fit_region(
        &stores,
        &region(&[1111, 2111, 3111]),
        &[rule(
            3,
            PeerRoleType::Voter,
            Some(("zone", &["zone1", "zone2", "zone3"])),
        )],
        false,
    );
    assert!(!restricted.replace(1111, Some(&store(4111))));
}
#[test]
fn placement_isolation_role_exchange_and_lookup() {
    let stores: Vec<_> = [1111, 1121, 1131].map(store).into();
    for (role, allowed) in [(PeerRoleType::Voter, true), (PeerRoleType::Leader, false)] {
        let rules = vec![
            rule(1, role, Some(("host", &["host1"]))),
            rule(1, PeerRoleType::Voter, Some(("host", &["host2"]))),
            rule(1, PeerRoleType::Voter, Some(("host", &["host3"]))),
        ];
        let f = fit_region(&stores, &region(&[1111, 1121, 1131]), &rules, false);
        assert!(f.is_satisfied());
        assert_eq!(f.replace(1111, Some(&stores[1])), allowed);
        assert!(Arc::ptr_eq(&f.get_rule_fit(11111).unwrap().rule, &rules[0]));
        assert!(f.get_rule_fit(1111).is_none());
        assert_eq!(f.extra_count(), 0);
    }
}
#[test]
fn placement_isolation_hierarchy_and_extra_count() {
    for (ids, score) in [
        (vec![1111, 1112], 0.0),
        (vec![1111, 1121], 1.0),
        (vec![1111, 1211], 100.0),
        (vec![1111, 2111], 10000.0),
    ] {
        let stores: Vec<_> = ids.iter().map(|id| store(*id)).collect();
        let mut r = (*rule(2, PeerRoleType::Voter, None)).clone();
        // Go fit only uses location labels; isolation_level is enforced by scheduling callers.
        r.isolation_level = "zone".into();
        let f = fit_region(&stores, &region(&ids), &[Arc::new(r)], false);
        assert_eq!(f.rule_fits[0].isolation_score, score);
        assert!(f.is_satisfied());
    }
    let stores = vec![store(1111), store(2111), store(9999)];
    let f = fit_region(
        &stores,
        &region(&[1111, 2111, 3111]),
        &[rule(3, PeerRoleType::Voter, None)],
        false,
    );
    assert_eq!(f.extra_count(), -1); // missing store excluded, unrelated store excluded
    let f = fit_region(
        &stores,
        &region(&[1111, 2111]),
        &[rule(1, PeerRoleType::Voter, None)],
        false,
    );
    assert_eq!(f.extra_count(), 1);
    assert!(f.get_rule_fit(12111).is_none()); // orphan
}
