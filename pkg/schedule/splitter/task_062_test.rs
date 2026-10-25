// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    DownPeer, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, RegionsInfo, StoreInfo, StoreMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

async fn check_case(c: &serde_json::Value) {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    conf.replication.enable_placement_rules = c["rules"] == true;
    let stores: Vec<_> = (1..=4)
        .map(|id| {
            Arc::new(StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }))
        })
        .collect();
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(62),
    );
    manager
        .initialize(
            3,
            &[],
            "",
            false,
            &conf,
            &stores.iter().map(|s| (**s).clone()).collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    let mut regions = RegionsInfo::new();
    let ranges = if c["unbounded"] == true {
        vec![("", "")]
    } else {
        vec![("aaa", "ccc"), ("ccc", "eee"), ("fff", "ggg")]
    };
    for (i, (start, end)) in ranges.into_iter().enumerate() {
        let id = i as u64 + 1;
        let peers: Vec<_> = (1..=c["peers"].as_u64().unwrap_or(3))
            .map(|store_id| Peer {
                id: id * 10 + store_id,
                store_id,
                role: if store_id == 1 && c["learner"] == true {
                    PeerRole::Learner
                } else {
                    PeerRole::Voter
                },
                ..Default::default()
            })
            .collect();
        regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: start.as_bytes().to_vec(),
                    end_key: end.as_bytes().to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            if c["leader"] == false {
                None
            } else {
                Some(peers[0].clone())
            },
            if c["down"] == true {
                vec![DownPeer {
                    peer: peers[0].clone(),
                    down_seconds: 600,
                }]
            } else {
                vec![]
            },
            if c["pending"] == true {
                vec![peers[0].clone()]
            } else {
                vec![]
            },
        )));
    }
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let keys: Vec<Vec<u8>> = c["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().as_bytes().to_vec())
        .collect();
    let mut suspect = Vec::new();
    let result = group_keys_by_region(&regions, &context, &keys, |merge, id| {
        assert!(!merge);
        suspect.push(id);
    });
    let actual: BTreeMap<String, Vec<String>> = result
        .iter()
        .map(|(id, group)| {
            assert!(Arc::ptr_eq(
                &group.region,
                &regions.get_region(*id).unwrap()
            ));
            (
                id.to_string(),
                group
                    .keys
                    .iter()
                    .map(|k| String::from_utf8(k.clone()).unwrap())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        c["groups"],
        "{}",
        c["id"]
    );
    let expected: Vec<u64> = c["suspect"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_u64().unwrap()).collect())
        .unwrap_or_default();
    assert_eq!(suspect, expected, "{} suspect", c["id"]);
}
#[tokio::test]
async fn task_062_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_062.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        check_case(case).await;
    }
    println!("task_062: 17 source-derived scenarios passed");
}
#[tokio::test]
async fn task_062_refresh_after_split() {
    let mut regions = RegionsInfo::new();
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 1;
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    let region = |id, start: &[u8], end: &[u8]| {
        Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                peers: vec![peer.clone()],
                key_range: KeyRange {
                    start_key: start.to_vec(),
                    end_key: end.to_vec(),
                },
                ..Default::default()
            },
            Some(peer.clone()),
            vec![],
            vec![],
        ))
    };
    regions.set_region(region(1, b"a", b"z"));
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(62),
    );
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let keys = vec![b"m".to_vec(), b"n".to_vec()];
    let before = group_keys_by_region(&regions, &context, &keys, |_, _| panic!("replicated"));
    assert_eq!(before[&1].keys, keys);
    regions.set_region(region(2, b"a", b"m"));
    regions.set_region(region(3, b"m", b"z"));
    let after = group_keys_by_region(&regions, &context, &keys, |_, _| panic!("replicated"));
    assert_eq!(after.len(), 1);
    assert_eq!(after[&3].keys, vec![b"n".to_vec()]);
    assert_eq!(before[&1].region.meta().key_range.end_key, b"z");
    assert!(regions.get_region(1).is_none());
}
