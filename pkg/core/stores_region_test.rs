// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn cases() -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(include_str!("rust-fixtures/core_task_56.json"))
        .unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}
fn build(case: &serde_json::Value) -> (StoresInfo, RegionInfo) {
    let stores = StoresInfo::new();
    for id in case["stores"].as_array().unwrap() {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: id.as_u64().unwrap(),
            ..Default::default()
        })));
    }
    let peers = case["peers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Peer {
            id: p[0].as_u64().unwrap(),
            store_id: p[1].as_u64().unwrap(),
            role: match p[2].as_str().unwrap() {
                "voter" => PeerRole::Voter,
                "learner" => PeerRole::Learner,
                "incoming" => PeerRole::IncomingVoter,
                "demoting" => PeerRole::DemotingVoter,
                "unknown" => PeerRole::Unknown(99),
                _ => panic!("invalid role"),
            },
            is_witness: p[3].as_bool().unwrap(),
        })
        .collect();
    let leader = case["leader"].as_array().map(|p| Peer {
        id: p[0].as_u64().unwrap(),
        store_id: p[1].as_u64().unwrap(),
        ..Default::default()
    });
    (
        stores,
        RegionInfo::new(
            RegionMeta {
                peers,
                ..Default::default()
            },
            leader,
            vec![],
            vec![],
        ),
    )
}
fn assert_stores(
    stores: &StoresInfo,
    actual: Vec<Arc<StoreInfo>>,
    case: &serde_json::Value,
    key: &str,
) {
    for store in &actual {
        assert!(Arc::ptr_eq(store, &stores.get_store(store.id()).unwrap()));
    }
    let mut ids: Vec<_> = actual.iter().map(|s| s.id()).collect();
    ids.sort_unstable();
    assert_eq!(
        serde_json::to_value(ids).unwrap(),
        case[key],
        "{}: {key}",
        case["name"]
    );
}
#[test]
fn core_task_56_membership_intersects_registry() {
    for case in cases() {
        let (stores, region) = build(&case);
        assert_stores(&stores, stores.get_region_stores(&region), &case, "region");
        assert_stores(
            &stores,
            stores.get_follower_stores(&region),
            &case,
            "followers",
        );
        assert_stores(
            &stores,
            stores.get_non_witness_voter_stores(&region),
            &case,
            "non_witness",
        );
        assert_eq!(
            serde_json::to_value(stores.get_leader_store(&region).map(|s| s.id())).unwrap(),
            case["leader_store"],
            "{}",
            case["name"]
        );
    }
}
#[test]
fn core_task_56_handles_survive_replacement_and_deletion() {
    let case = cases().remove(0);
    let (stores, region) = build(&case);
    let old = stores.get_leader_store(&region).unwrap();
    let replacement = Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    }));
    stores.put_store(replacement.clone());
    assert!(Arc::ptr_eq(
        &stores.get_leader_store(&region).unwrap(),
        &replacement
    ));
    assert!(!Arc::ptr_eq(&old, &replacement));
    stores.delete_store(1);
    assert!(stores.get_leader_store(&region).is_none());
    assert!(
        stores
            .get_region_stores(&region)
            .iter()
            .all(|s| s.id() != 1)
    );
    stores.reset_stores();
    assert!(stores.get_follower_stores(&region).is_empty());
    assert!(stores.get_non_witness_voter_stores(&region).is_empty());
    assert_eq!(old.id(), 1);
}
