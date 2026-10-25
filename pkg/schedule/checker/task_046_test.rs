// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_codec::{
    TXN_KEYSPACE_MODE_PREFIX, encode_bytes, generate_index_key, generate_row_key,
    generate_table_key, make_keyspace_prefix,
};
use astersql_pd_core::{
    DownPeer, KeyRange, NodeState, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
    StoresInfo,
};
use astersql_pd_core_constant::KeyType;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use astersql_pd_schedule_placement::{Rule, RuleManager};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::{Value, json};
use std::{cell::RefCell, sync::Arc};
fn region(start: Vec<u8>, end: Vec<u8>, stores: &[u64], fault: &str) -> RegionInfo {
    let mut peers: Vec<_> = stores
        .iter()
        .map(|&store_id| Peer {
            id: store_id + 10,
            store_id,
            ..Default::default()
        })
        .collect();
    if fault == "learner" {
        peers[0].role = PeerRole::Learner;
    }
    let pending = if fault == "pending" {
        vec![peers[0].clone()]
    } else {
        vec![]
    };
    let down = if fault == "down" {
        vec![DownPeer {
            peer: peers[0].clone(),
            down_seconds: 0,
        }]
    } else {
        vec![]
    };
    RegionInfo::new(
        RegionMeta {
            id: 46,
            key_range: KeyRange {
                start_key: start,
                end_key: end,
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        down,
        pending,
    )
}
fn key(ks: Option<u32>, table: i64, index: bool) -> Vec<u8> {
    let mut raw = ks
        .map(|id| make_keyspace_prefix(TXN_KEYSPACE_MODE_PREFIX, id).to_vec())
        .unwrap_or_default();
    raw.extend(if index {
        generate_index_key(table, 1)
    } else {
        generate_row_key(table, 1)
    });
    encode_bytes(&raw)
}
fn pair(c: &Value) -> (RegionInfo, RegionInfo) {
    let layout = c["layout"].as_str().unwrap_or("");
    let (mut start, mut seam) = match layout {
        "classic-same" => (key(None, 100, true), key(None, 100, false)),
        "classic-diff" => (key(None, 100, false), key(None, 101, false)),
        "ks-same" => (key(Some(42), 100, true), key(Some(42), 100, false)),
        "ks-table-diff" => (key(Some(42), 100, false), key(Some(42), 101, false)),
        "ks-diff" => (key(Some(42), 100, false), key(Some(43), 100, false)),
        "ks-both-diff" => (key(Some(42), 100, false), key(Some(43), 101, false)),
        "classic-ks0" => (key(None, 100, false), key(Some(0), 100, false)),
        "negative-table" => (key(None, -2, true), key(None, -2, false)),
        "invalid" => (vec![1], vec![2]),
        "invalid-table" => (vec![1], encode_bytes(&generate_table_key(100))),
        _ => (b"a".to_vec(), b"b".to_vec()),
    };
    let mut end = if layout.is_empty() {
        b"d".to_vec()
    } else {
        key(Some(44), 102, false)
    };
    let mut next_start = seam.clone();
    match c["range"].as_str().unwrap_or("") {
        "gap" => next_start = b"c".to_vec(),
        "overlap" => next_start = start.clone(),
        "empty" => {
            seam.clear();
            next_start.clear();
        }
        "unbounded" => {
            start.clear();
            end.clear();
        }
        _ => (),
    }
    let a = region(start, seam, &[1, 2, 3], "");
    let b = region(next_start, end, &[1, 2, 3], "");
    if c["reverse"] == true { (b, a) } else { (a, b) }
}
fn storage() -> StorageEndpoint {
    StorageEndpoint::new(Arc::new(MemoryStore::default()))
}
#[tokio::test]
async fn task_046_behavior_cases() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_046.json"
    ))
    .unwrap();
    for c in data["cases"].as_array().unwrap() {
        let mut conf = ProviderSnapshot::<()>::default();
        conf.key_type = KeyType::from_i32(c["policy"].as_i64().unwrap() as i32);
        conf.schedule.enable_cross_table_merge = c["cross"] == true;
        conf.replication.enable_placement_rules = c["rules"] == true;
        let mut m = RuleManager::new(Arc::new(storage()), ClusterPaths::new(46));
        m.initialize(3, &[], "", false, &conf, &[]).await.unwrap();
        if let Some(start) = c["rule_start"].as_str() {
            m.set_rule(
                Rule {
                    group_id: "pd".into(),
                    id: "split".into(),
                    start_key_hex: start.into(),
                    role: astersql_pd_schedule_placement::PeerRoleType::Voter,
                    count: 3,
                    ..Default::default()
                },
                3,
                &conf,
                &[],
            )
            .await
            .unwrap();
        }
        let mut labeler = RegionLabeler::load(storage(), ClusterPaths::new(46), 0)
            .await
            .unwrap();
        if let Some(start) = c["label_start"].as_str() {
            let rule=LabelRule::from_json(&serde_json::to_vec(&json!({"id":"test","labels":[{"key":"merge_option","value":c["label_value"],"ttl":c["ttl"].as_str().unwrap_or("")}],"rule_type":"key-range","data":[{"start_key":start,"end_key":c["label_end"]}]})).unwrap(),0).unwrap();
            labeler.set_label_rule(rule, 0).await.unwrap();
        }
        let (a, b) = pair(c);
        assert_eq!(
            allow_merge(
                &conf,
                if c["missing_rules"] == true {
                    None
                } else {
                    Some(&m)
                },
                Some(&labeler),
                c["now"].as_i64().unwrap_or(0) as i128,
                &a,
                &b
            ),
            c["expected"].as_bool().unwrap(),
            "{}",
            c["name"]
        );
        // Identity and adjacency are symmetric; no input snapshot is mutated.
        assert_eq!(
            allow_merge(
                &conf,
                if c["missing_rules"] == true {
                    None
                } else {
                    Some(&m)
                },
                Some(&labeler),
                c["now"].as_i64().unwrap_or(0) as i128,
                &b,
                &a
            ),
            c["expected"].as_bool().unwrap(),
            "reverse {}",
            c["name"]
        );
    }
    println!(
        "task_046: {} frozen cases, both directions",
        data["cases"].as_array().unwrap().len()
    );
}
#[tokio::test]
async fn task_046_target_safety() {
    let mut conf = ProviderSnapshot::<()>::default();
    conf.key_type = KeyType::RAW;
    conf.replication.max_replicas = 3;
    let a = region(b"a".to_vec(), b"b".to_vec(), &[1, 2, 3], "");
    let mut count = 0;
    let data: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_046.json"
    ))
    .unwrap();
    for c in data["targets"].as_array().unwrap() {
        let rules = c["rules"].as_bool().unwrap();
        conf.replication.enable_placement_rules = rules;
        let ids: Vec<u64> = c["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        let fault = c["fault"].as_str().unwrap();
        let state = c["state"].as_u64().unwrap();
        let name = c["name"].as_str().unwrap();
        let expected = c["expected"].as_bool().unwrap();
        let stores = StoresInfo::new();
        for id in 1..=4 {
            if state == 1 && (id == 3 || id == 4) {
                continue;
            }
            stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                node_state: if state == 2 && (id == 3 || id == 4) {
                    NodeState::Removing
                } else {
                    NodeState::Serving
                },
                ..Default::default()
            })));
        }
        let snapshots = stores.get_stores();
        let mut m = RuleManager::new(Arc::new(storage()), ClusterPaths::new(46));
        m.initialize(3, &[], "", false, &conf, &[]).await.unwrap();
        let ctx = RegionReplicationContext {
            conf: &conf,
            stores: &snapshots,
            manager: RefCell::new(&mut m),
            now: 0,
        };
        let b = region(b"b".to_vec(), b"d".to_vec(), &ids, fault);
        let actual = merge_target_allowed(&ctx, None, &stores, &a, Some(&b));
        assert_eq!(actual, expected, "{name}, rules={rules}");
        count += 1;
        assert!(!merge_target_allowed(&ctx, None, &stores, &a, None));
    }
    println!("task_046: {count} target cases plus absent targets");
}
