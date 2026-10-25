// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::{Value, json};
use std::sync::Arc;
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
#[tokio::test]
async fn task_048_behavior_cases() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_048.json"
    ))
    .unwrap();
    let cases = data["cases"].as_array().unwrap();
    for c in cases {
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = c["rules"] == true;
        let storage = || StorageEndpoint::new(Arc::new(MemoryStore::default()));
        let mut rules = RuleManager::new(Arc::new(storage()), ClusterPaths::new(48));
        rules
            .initialize(3, &[], "", false, &conf, &[])
            .await
            .unwrap();
        for (i, r) in c["placement"].as_array().unwrap().iter().enumerate() {
            rules
                .set_rule(
                    Rule {
                        group_id: "test".into(),
                        id: i.to_string(),
                        start_key_hex: r[0].as_str().unwrap().into(),
                        end_key_hex: r[1].as_str().unwrap().into(),
                        role: PeerRoleType::Voter,
                        count: 1,
                        ..Default::default()
                    },
                    3,
                    &conf,
                    &[],
                )
                .await
                .unwrap();
        }
        let mut labels = RegionLabeler::load(storage(), ClusterPaths::new(48), 0)
            .await
            .unwrap();
        for (i, r) in c["labels"].as_array().unwrap().iter().enumerate() {
            let rule=LabelRule::from_json(&serde_json::to_vec(&json!({"id":i.to_string(),"labels":[{"key":"test","value":"test"}],"rule_type":"key-range","data":[{"start_key":r[0],"end_key":r[1]}]})).unwrap(),0).unwrap();
            labels.set_label_rule(rule, 0).await.unwrap();
        }
        let peer = Peer {
            id: 11,
            store_id: 1,
            role: if c["joint"] == true {
                PeerRole::IncomingVoter
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        };
        let region = RegionInfo::new(
            RegionMeta {
                id: 48,
                key_range: KeyRange {
                    start_key: bytes(c["start"].as_str().unwrap()),
                    end_key: bytes(c["end"].as_str().unwrap()),
                },
                epoch: Some(RegionEpoch {
                    conf_ver: 7,
                    version: 9,
                }),
                peers: vec![peer.clone()],
                ..Default::default()
            },
            if c["leader"] == false {
                None
            } else {
                Some(peer)
            },
            vec![],
            vec![],
        );
        let checker = SplitChecker::default();
        assert_eq!(checker.get_type(), "split-checker");
        if let Some(ttl) = c["pause"].as_i64() {
            checker.pause.pause_or_resume_at(ttl, 0);
        }
        let result = checker.check(&conf, &rules, &labels, &region, c["now"].as_i64().unwrap());
        if c["error"] == "OriginJoint" {
            assert!(
                matches!(result, Err(BuilderError::OriginJoint)),
                "{}",
                c["name"]
            );
            continue;
        }
        let op = result.unwrap();
        assert_eq!(op.is_some(), c["keys"].is_array(), "{}", c["name"]);
        let Some(op) = op else { continue };
        let expected: Vec<Vec<u8>> = c["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| bytes(v.as_str().unwrap()))
            .collect();
        assert_eq!(op.desc(), c["desc"].as_str().unwrap());
        assert_eq!(op.region_id(), 48);
        assert_eq!(op.region_epoch(), region.meta().epoch.as_ref());
        assert_eq!(op.kind(), OpKind::SPLIT);
        assert_eq!(op.priority_level(), PriorityLevel::MEDIUM);
        assert_eq!(op.len(), 1);
        let OpStep::SplitRegion(step) = op.step(0).unwrap() else {
            panic!("split step")
        };
        assert_eq!(step.start_key, region.meta().key_range.start_key);
        assert_eq!(step.end_key, region.meta().key_range.end_key);
        assert_eq!(step.split_keys, expected, "{}", c["name"]);
        assert_eq!(step.policy, SplitPolicy::UseKey);
        assert_eq!(step.conf_ver_changed(&region), 0);
        assert!(!step.is_finish(&region));
        for v2 in [false, true] {
            let command = step.get_cmd(&region, v2).split_region.unwrap();
            assert_eq!(command.keys, expected);
            assert_eq!(command.policy, SplitPolicy::UseKey);
        }
        let inf = OpInfluence::default();
        step.influence(&inf, &region);
        let influence = inf.get_store_influence(1);
        assert_eq!(influence.region_count, 1);
        assert_eq!(influence.leader_count, i64::from(c["leader"] != false));
    }
    println!("task_048: {} fixed behavior cases passed", cases.len());
}
