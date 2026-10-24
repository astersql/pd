// Copyright 2026 AsterSQL.
//! Test-only assembly of canonical placement, label and affinity interfaces.
use astersql_pd_core::{
    DownPeer, KeyRange as CoreKeyRange, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo,
    StoreLabel, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_affinity::{manager::Manager, ranges::GroupKeyRanges};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::{Rule, RuleManager, fit_region};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{Base, MetaStore};
use std::sync::Arc;
pub async fn contract(store: Arc<dyn MetaStore>, seed: bool) {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule/placement/task27/go.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 15);
    for (case_index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let peers: Vec<_> = case["peers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Peer {
                id: p["id"].as_u64().unwrap(),
                store_id: p["store_id"].as_u64().unwrap(),
                role: match p["role"].as_u64().unwrap_or(0) {
                    1 => PeerRole::Learner,
                    2 => PeerRole::IncomingVoter,
                    3 => PeerRole::DemotingVoter,
                    _ => PeerRole::Voter,
                },
                is_witness: p["is_witness"].as_bool().unwrap_or(false),
            })
            .collect();
        let health = case["health"].as_bool().unwrap_or(false);
        let region = RegionInfo::new(
            RegionMeta {
                id: 41,
                key_range: CoreKeyRange {
                    start_key: vec![1],
                    end_key: vec![2],
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            if health {
                vec![DownPeer {
                    peer: peers[0].clone(),
                    down_seconds: 10,
                }]
            } else {
                vec![]
            },
            if health {
                vec![peers[1].clone()]
            } else {
                vec![]
            },
        );
        let stores: Vec<_> = case["stores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let mut store = StoreInfo::new(StoreMeta {
                    id: entry["store"]["id"].as_u64().unwrap(),
                    labels: entry["store"]["labels"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|l| StoreLabel {
                                    key: l["key"].as_str().unwrap().into(),
                                    value: l["value"].as_str().unwrap().into(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    ..Default::default()
                });
                store.set_witness_count(entry["status"]["witness_count"].as_i64().unwrap_or(0));
                Arc::new(store)
            })
            .collect();
        let expected_fits = case["output"]["rule-fits"].as_array().unwrap();
        let paths = ClusterPaths::new(4100 + case_index as u64);
        let ep = StorageEndpoint::new(store.clone());
        let base = Base::new(store.as_ref());
        if seed {
            for expected in expected_fits {
                let rule =
                    Rule::from_json(&serde_json::to_vec(&expected["rule"]).unwrap()).unwrap();
                base.save(
                    paths.rule_key_path(&rule.store_key()).as_bytes(),
                    &serde_json::to_vec(&rule).unwrap(),
                )
                .await
                .unwrap();
            }
        }
        let mut config: ProviderSnapshot<String> = ProviderSnapshot::default();
        config.replication.max_replicas = 3;
        config.schedule.enable_witness = case["support"].as_bool().unwrap_or(false);
        let mut manager = RuleManager::new(Arc::new(ep), paths.clone());
        manager
            .initialize(3, &["zone".into()], "", false, &config, &[])
            .await
            .unwrap();
        let rules = manager.get_rules_for_apply_region(&region);
        let result = fit_region(
            &stores,
            &region,
            &rules,
            case["support"].as_bool().unwrap_or(false),
        );
        let ids = |v: &serde_json::Value| {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| p["id"].as_u64().unwrap())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        assert_eq!(
            result.orphan_peers.iter().map(|p| p.id).collect::<Vec<_>>(),
            ids(&case["output"]["orphan-peers"]),
            "{name} orphans"
        );
        assert_eq!(result.rule_fits.len(), expected_fits.len());
        for (fit, expected) in result.rule_fits.iter().zip(expected_fits) {
            assert_eq!(
                fit.peers.iter().map(|p| p.id).collect::<Vec<_>>(),
                ids(&expected["peers"]),
                "{name} {}",
                fit.rule.id
            );
            assert_eq!(
                fit.peers_with_different_role
                    .iter()
                    .map(|p| p.id)
                    .collect::<Vec<_>>(),
                ids(&expected["peers-different-role"]),
                "{name}"
            );
            assert_eq!(
                fit.isolation_score,
                expected["isolation-score"].as_f64().unwrap(),
                "{name}"
            );
            assert_eq!(
                fit.witness_score,
                expected["witness-score"].as_i64().unwrap(),
                "{name}"
            );
        }
        assert_eq!(
            result.is_satisfied(),
            !expected_fits.is_empty()
                && ids(&case["output"]["orphan-peers"]).is_empty()
                && expected_fits.iter().all(|f| ids(&f["peers"]).len() as i64
                    == f["rule"]["count"].as_i64().unwrap()
                    && ids(&f["peers-different-role"]).is_empty()),
            "{name}"
        );
        let ep = StorageEndpoint::new(store.clone());
        let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
            .await
            .unwrap();
        let mut affinity = Manager::initialize(&ep, &paths, &labels, 100)
            .await
            .unwrap();
        if seed {
            affinity
                .create_affinity_groups(
                    &ep,
                    &paths,
                    &mut labels,
                    &[GroupKeyRanges {
                        group_id: "chain".into(),
                        key_ranges: vec![KeyRange::new(vec![1], vec![2])],
                    }],
                    100,
                )
                .await
                .unwrap();
        }
        let query = Arc::new(region);
        assert_eq!(
            labels.get_region_label(&query, "affinity_group", 0),
            "chain"
        );
        let outside = RegionInfo::new(
            RegionMeta {
                key_range: CoreKeyRange {
                    start_key: vec![2],
                    end_key: vec![3],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        );
        assert_eq!(labels.get_region_label(&outside, "affinity_group", 0), "");
        assert_eq!(
            manager.get_rules_for_apply_range(&[1], &[2]).len(),
            expected_fits.len()
        );
        let state = affinity
            .get_region_affinity_group_state(&labels, Some(&query), 100)
            .0
            .unwrap();
        assert_eq!(state.group.id, "chain");
        let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
            .await
            .unwrap();
        let restored = Manager::initialize(&ep, &paths, &labels, 100)
            .await
            .unwrap();
        assert_eq!(restored.groups["chain"].info.range_count, 1);
        assert_eq!(
            restored
                .get_region_affinity_group_state(&labels, Some(&query), 100)
                .0
                .unwrap()
                .group
                .id,
            "chain"
        );
        // Duplicate creation fails atomically: no range/label changes become visible.
        let before = serde_json::to_value(labels.get_all_label_rules(0)).unwrap();
        let error = affinity
            .create_affinity_groups(
                &ep,
                &paths,
                &mut labels,
                &[GroupKeyRanges {
                    group_id: "chain".into(),
                    key_ranges: vec![KeyRange::new(vec![3], vec![4])],
                }],
                101,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("ErrAffinityGroupExist"));
        assert_eq!(
            serde_json::to_value(labels.get_all_label_rules(0)).unwrap(),
            before
        );
        let error = affinity
            .create_affinity_groups(
                &ep,
                &paths,
                &mut labels,
                &[GroupKeyRanges {
                    group_id: "overlap".into(),
                    key_ranges: vec![KeyRange::new(vec![1], vec![3])],
                }],
                101,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("overlap"));
        assert!(!affinity.groups.contains_key("overlap"));
        assert!(
            ep.load_affinity_group(&paths, "overlap")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            serde_json::to_value(labels.get_all_label_rules(0)).unwrap(),
            before
        );
        println!("schedule_foundation oracle case={name} fit/match/restore/duplicate passed");
    }
}
