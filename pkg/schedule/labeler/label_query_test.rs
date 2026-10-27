// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::json;
use std::sync::Arc;
fn region(start: u8, end: u8) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            key_range: KeyRange {
                start_key: vec![start],
                end_key: vec![end],
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )
}
#[tokio::test]
async fn label_query_priority_boundaries_and_ttl() {
    let mut owner = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(19),
        0,
    )
    .await
    .unwrap();
    for (id, index, labels) in [
        ("base", -2, json!([{"key":"k","value":"base"}])),
        (
            "high",
            5,
            json!([{"key":"k","value":"high","ttl":"1s"},{"key":"schedule","value":"DeNy"}]),
        ),
    ] {
        let rule = LabelRule::from_json(&serde_json::to_vec(&json!({"id":id,"index":index,"labels":labels,"rule_type":"key-range","data":[{"start_key":"10","end_key":"20"}]})).unwrap(),0).unwrap();
        owner.set_validated_label_rule(rule).await.unwrap();
    }
    let r = region(0x10, 0x20);
    assert_eq!(owner.get_region_label(&r, "k", 1_000_000_000), "high");
    assert_eq!(owner.get_region_label(&r, "k", 1_000_000_001), "base");
    assert!(owner.schedule_disabled(&r, 0));
    assert_eq!(owner.get_region_label(&region(0x0f, 0x20), "k", 0), "");
    assert!(owner.get_region_labels(&region(0x10, 0x21), 0).is_empty());
    let labels = owner.get_region_labels(&r, 1_000_000_001);
    assert_eq!(
        labels
            .iter()
            .map(|l| (l.key.as_str(), l.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("k", "base"), ("schedule", "DeNy")]
    );
    assert!(
        labels
            .iter()
            .all(|l| l.ttl.is_empty() && l.start_at.is_empty())
    );
    let selected = owner.get_label_rules(&["high", "absent", "base", "high"], 1_000_000_001);
    assert_eq!(
        selected.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["high", "base", "high"]
    );
    assert_eq!(selected[0].labels.len(), 1);
    assert_eq!(owner.get_all_label_rules(1_000_000_001).len(), 2);
    assert_eq!(owner.index().rule("high").unwrap().labels.len(), 2);
    let mut count = 0;
    owner.iterate_label_rules(|_| {
        count += 1;
        false
    });
    assert_eq!(count, 1);
}

#[tokio::test]
async fn label_query_expired_snapshots_and_duplicate_labels() {
    let mut owner = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(20),
        0,
    )
    .await
    .unwrap();
    let rule = LabelRule::from_json(&serde_json::to_vec(&json!({"id":"duplicates","index":-5,"labels":[{"key":"k","value":"first","ttl":"1s"},{"key":"k","value":"last","ttl":"1s"}],"rule_type":"key-range","data":[{"start_key":"10","end_key":"20"}]})).unwrap(),0).unwrap();
    owner.set_validated_label_rule(rule).await.unwrap();
    let r = region(0x10, 0x20);
    // Go single-key scans all labels of a rule; multi-key keeps its first occurrence.
    assert_eq!(owner.get_region_label(&r, "k", 0), "last");
    assert_eq!(owner.get_region_labels(&r, 0)[0].value, "first");
    assert!(owner.get_all_label_rules(1_000_000_001).is_empty());
    assert!(owner.get_label_rule("duplicates", 1_000_000_001).is_none());
    assert_eq!(owner.get_all_label_rules(1_000_000_000).len(), 1);
    let mut raw = 0;
    owner.iterate_label_rules(|rule| {
        raw += rule.labels.len();
        true
    });
    assert_eq!(raw, 2);
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task19/go.json"
    ))
    .unwrap();
    assert_eq!(oracle["crossing"], json!([200, []]));
    assert_eq!(oracle["single"], json!([200, "high"]));
}

#[tokio::test]
async fn label_query_go_runtime_fixture() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task19/go.json"
    ))
    .unwrap();
    let mut owner = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(21),
        0,
    )
    .await
    .unwrap();
    for rule in oracle["all"][1].as_array().unwrap() {
        owner
            .set_label_rule(
                LabelRule::from_json(&serde_json::to_vec(rule).unwrap(), 0).unwrap(),
                0,
            )
            .await
            .unwrap();
    }
    let r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    assert_eq!(
        json!(owner.get_region_label(&r, "k", 0)),
        oracle["single"][1]
    );
    let mut expected = oracle["labels"][1].as_array().unwrap().clone();
    expected.sort_by_key(|v| v["key"].as_str().unwrap().to_string());
    let actual: Vec<_> = owner
        .get_region_labels(&r, 0)
        .iter()
        .map(|l| json!({"key":l.key,"value":l.value}))
        .collect();
    assert_eq!(actual, expected);
    assert!(owner.schedule_disabled(&r, 0));
    let boundary = astersql_pd_codec::encode_keyspace_boundary;
    let hex = |b: [u8; 9]| b.iter().map(|v| format!("{v:02x}")).collect::<String>();
    let rule = json!({"id":"keyspaces/0","labels":[{"key":"id","value":"0"}],"rule_type":"key-range","data":[{"start_key":hex(boundary(b'x',0)),"end_key":hex(boundary(b'x',1))}]});
    owner
        .set_label_rule(
            LabelRule::from_json(&serde_json::to_vec(&rule).unwrap(), 0).unwrap(),
            0,
        )
        .await
        .unwrap();
    assert_eq!(json!(owner.get_region_labels(&r, 0)), oracle["crossing"][1]);
    let inner = RegionInfo::new(
        RegionMeta {
            key_range: KeyRange {
                start_key: boundary(b'x', 0).to_vec(),
                end_key: boundary(b'x', 1).to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    );
    assert_eq!(owner.get_region_label(&inner, "id", 0), "0");
    assert_eq!(owner.get_region_label(&inner, "k", 0), "high");
}
