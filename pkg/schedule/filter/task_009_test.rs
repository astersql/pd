// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::{Rule, fit_region};
use serde_json::Value;
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_009.json"
    ))
    .unwrap()
}
fn stores(case: &Value) -> Vec<Arc<StoreInfo>> {
    case["stores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            Arc::new(StoreInfo::new(StoreMeta {
                id: s["id"].as_u64().unwrap(),
                labels: s["labels"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| StoreLabel {
                        key: k.clone(),
                        value: v.as_str().unwrap().into(),
                    })
                    .collect(),
                ..Default::default()
            }))
        })
        .collect()
}
fn region(case: &Value) -> Arc<RegionInfo> {
    let peers: Vec<_> = case["peers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Peer {
            id: p["id"].as_u64().unwrap(),
            store_id: p["store_id"].as_u64().unwrap(),
            is_witness: p["is_witness"].as_bool().unwrap(),
            ..Default::default()
        })
        .collect();
    let leader = peers
        .iter()
        .find(|p| Some(p.store_id) == case["leader"].as_u64())
        .cloned();
    Arc::new(RegionInfo::new(
        RegionMeta {
            peers,
            ..Default::default()
        },
        leader,
        vec![],
        vec![],
    ))
}
#[test]
fn task_009_behavior_cases() {
    let data = fixture();
    let conf = ProviderSnapshot::<()>::default();
    let mut count = 0;
    for case in data["cases"].as_array().unwrap() {
        let stores = stores(case);
        let source = case["source"].as_u64().unwrap();
        let target = stores
            .iter()
            .find(|s| s.id() == case["target"].as_u64().unwrap())
            .unwrap();
        let filter: Box<dyn Filter<()>> = match case["kind"].as_str().unwrap() {
            "safeguard" | "improve" => {
                let labels: Vec<String> = serde_json::from_value(case["labels"].clone()).unwrap();
                let members = case["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| {
                        stores
                            .iter()
                            .find(|s| s.id() == id.as_u64().unwrap())
                            .unwrap()
                            .clone()
                    })
                    .collect::<Vec<_>>();
                Box::new(DistinctScoreFilter::new(
                    "task009",
                    &labels,
                    &members,
                    stores.iter().find(|s| s.id() == source).unwrap(),
                    if case["kind"] == "improve" {
                        LocationPolicy::Improve
                    } else {
                        LocationPolicy::Safeguard
                    },
                ))
            }
            kind => {
                let region = region(case);
                let rules: Vec<Arc<Rule>> = case["rules"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| Arc::new(serde_json::from_value(r.clone()).unwrap()))
                    .collect();
                let fit = Arc::new(fit_region(&stores, &region, &rules, false));
                match kind {
                    "peer" => Box::new(RuleFitFilter::new("task009", fit, source)),
                    "leader" => Box::new(RuleLeaderFitFilter::new(
                        "task009",
                        region,
                        fit,
                        source,
                        case["allow_move"].as_bool().unwrap(),
                    )),
                    "witness" => {
                        Box::new(RuleWitnessFitFilter::new("task009", region, fit, source))
                    }
                    _ => panic!("unknown kind"),
                }
            }
        };
        for (i, status) in [filter.source(&conf, target), filter.target(&conf, target)]
            .iter()
            .enumerate()
        {
            assert_eq!(
                status.status_code.0,
                case["expected"][i]["status"].as_i64().unwrap(),
                "{} direction {i}",
                case["id"]
            );
            assert_eq!(
                status.detailed_reason,
                case["expected"][i]["detail"].as_str().unwrap(),
                "{}",
                case["id"]
            );
        }
        assert_eq!(filter.scope(), "task009");
        assert_eq!(
            filter.filter_type().0,
            match case["kind"].as_str().unwrap() {
                "safeguard" | "improve" => 2,
                "leader" => 5,
                _ => 4,
            }
        );
        count += 1;
    }
    assert_eq!(count, 129);
    println!(
        "task_009: {count} cases, {} source/target results",
        count * 2
    );
}

#[test]
fn task_009_temporary_region_is_independent() {
    use astersql_pd_core::{KeyRange, PeerRole, RegionEpoch, RegionOption};
    let peers = vec![
        Peer {
            id: 1,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: 7,
            store_id: 7,
            is_witness: true,
            ..Default::default()
        },
        Peer {
            id: 9,
            store_id: 9,
            role: PeerRole::Learner,
            ..Default::default()
        },
    ];
    let source = RegionInfo::new(
        RegionMeta {
            id: 55,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"z".to_vec(),
            },
            epoch: Some(RegionEpoch {
                conf_ver: 8,
                version: 9,
            }),
            peers,
            ..Default::default()
        },
        Some(Peer {
            id: 1,
            store_id: 1,
            is_witness: true,
            ..Default::default()
        }),
        vec![],
        vec![],
    );
    let before = source.meta().clone();
    let temp = create_region_for_rule_fit(b"a", b"z", &source.meta().peers, source.leader(), []);
    assert_eq!(temp.meta().id, 0);
    assert_eq!(temp.meta().epoch, None);
    assert_eq!(temp.leader(), source.leader()); // leader is copied whole, unlike peers
    assert!(temp.meta().peers.iter().all(|p| !p.is_witness));
    assert_eq!(temp.meta().peers[2].role, PeerRole::Learner);
    assert_eq!(temp.meta().key_range, before.key_range);
    let changed = create_region_for_rule_fit(
        b"a",
        b"z",
        &source.meta().peers,
        source.leader(),
        [
            RegionOption::ReplacePeerStore(7, 70),
            RegionOption::Witnesses(vec![7]),
            RegionOption::Leader(None),
            RegionOption::StartKey(b"b".to_vec()),
        ],
    );
    assert_eq!(changed.meta().peers[1].store_id, 70);
    assert!(changed.meta().peers[1].is_witness);
    assert!(changed.leader().is_none());
    assert_eq!(source.meta(), &before);
    let empty = create_region_for_rule_fit(&[], &[], &[], None, []);
    assert!(empty.meta().peers.is_empty());
    assert!(empty.leader().is_none());
}

#[tokio::test]
async fn task_009_factories_and_reusable_fit() {
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    let case = &fixture()["cases"][36];
    let stores = stores(case);
    let region = region(case);
    let before = region.meta().clone();
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.location_labels = vec!["zone".into()].into();
    conf.replication.enable_placement_rules = false;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(9),
    );
    manager
        .initialize(3, &["zone".into()], "", false, &conf, &[])
        .await
        .unwrap();
    let mut context = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: region.clone(),
        now: 0,
    };
    let source = &stores[0];
    let rejected = &stores[3];
    let guard = context.safeguard("factory", source, None);
    assert_eq!(guard.filter_type(), FilterType(2));
    assert!(!guard.target(&conf, rejected).is_ok());
    assert!(context.leader_safeguard("factory", 1, None, true).is_none());
    assert!(context.witness_safeguard("factory", 1, None).is_none());
    conf.replication.enable_placement_rules = true;
    let fit = manager.fit_region(&stores, &region, &conf, 0);
    let mut context = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut manager,
        region: region.clone(),
        now: 0,
    };
    for old in [None, Some(fit.clone())] {
        let guard = context.safeguard("factory", source, old.clone());
        assert_eq!(guard.filter_type(), FilterType(4));
        assert!(!guard.target(&conf, rejected).is_ok());
        assert!(guard.target(&conf, &stores[1]).is_ok());
        let leader = context
            .leader_safeguard("factory", 1, old.clone(), false)
            .unwrap();
        assert!(!leader.target(&conf, &stores[5]).is_ok());
        assert!(!leader.target(&conf, &stores[6]).is_ok());
        assert!(leader.target(&conf, &stores[2]).is_ok());
        let witness = context.witness_safeguard("factory", 1, old).unwrap();
        assert!(!witness.target(&conf, source).is_ok());
        assert!(!witness.target(&conf, &stores[5]).is_ok());
    }
    assert_eq!(region.meta(), &before);
    // Reusable fit must bypass even an uninitialized manager, preserving caller choice.
    let mut uninitialized = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(10),
    );
    let mut context = PlacementGuardContext {
        conf: &conf,
        stores: &stores,
        manager: &mut uninitialized,
        region,
        now: 0,
    };
    assert!(
        context
            .safeguard("cached", source, Some(fit))
            .target(&conf, &stores[1])
            .is_ok()
    );
    assert!(!context.manager.is_initialized());
}

#[test]
fn task_009_empty_location_input_panics() {
    let source = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    for policy in [LocationPolicy::Safeguard, LocationPolicy::Improve] {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                DistinctScoreFilter::new("empty", &[], &[], &source, policy)
            }))
            .is_err(),
            "Go rejects negative capacity when the replica input is empty"
        );
    }
}
