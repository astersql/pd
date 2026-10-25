// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_affinity::{group::Group, manager::Manager, ranges::GroupKeyRanges};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
use astersql_pd_schedule_operator::{BuilderError, OpKind, OpStep};
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use serde_json::{Value, json};
use std::sync::Arc;
fn num(c: &Value, k: &str, d: i64) -> i64 {
    c[k].as_i64().unwrap_or(d)
}
fn region(
    id: u64,
    start: u8,
    end: u8,
    size: i64,
    keys: i64,
    leader: u64,
    pending: bool,
    missing: bool,
    learner: u64,
) -> Arc<RegionInfo> {
    let mut peers: Vec<_> = (1..=if missing { 2 } else { 3 })
        .map(|s| Peer {
            id: id * 10 + s,
            store_id: s,
            ..Default::default()
        })
        .collect();
    if learner != 0 {
        peers.push(Peer {
            id: id * 10 + learner,
            store_id: learner,
            role: astersql_pd_core::PeerRole::Learner,
            ..Default::default()
        });
    }
    let mut r = RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: vec![start],
                end_key: vec![end],
            },
            peers: peers.clone(),
            epoch: Some(astersql_pd_core::RegionEpoch {
                version: 9,
                conf_ver: 7,
            }),
            ..Default::default()
        },
        Some(peers[(leader - 1) as usize].clone()),
        vec![],
        if pending {
            vec![peers[0].clone()]
        } else {
            vec![]
        },
    );
    r.stats_mut().approximate_size = size;
    r.stats_mut().approximate_keys = keys;
    Arc::new(r)
}
#[tokio::test]
async fn task_050_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_050.json"
    ))
    .unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = c["disabled"] != true;
        conf.schedule.max_affinity_merge_region_size = num(c, "limit", 10) as u64;
        conf.schedule.max_merge_region_size = num(c, "global_size", 2) as u64;
        conf.schedule.merge_schedule_limit = num(c, "schedule_limit", 8) as u64;
        conf.schedule.enable_one_way_merge = c["one_way"] == true;
        let stores = StoresInfo::new();
        for id in 1..=5 {
            stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                labels: vec![astersql_pd_core::StoreLabel {
                    key: "zone".into(),
                    value: "z".into(),
                }],
                ..Default::default()
            })));
        }
        let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(50);
        let mut labels = RegionLabeler::load(
            StorageEndpoint::new(Arc::new(MemoryStore::default())),
            paths.clone(),
            0,
        )
        .await
        .unwrap();
        let mut manager = Manager::default();
        let ranges = if c["other_group"] == true {
            vec![("g", 1, 3), ("h", 3, 9)]
        } else if c["incomplete"] == true {
            vec![("g", 1, 3)]
        } else {
            vec![("g", 1, 9)]
        };
        for (id, a, b) in ranges {
            manager
                .create_affinity_groups(
                    &storage,
                    &paths,
                    &mut labels,
                    &[GroupKeyRanges {
                        group_id: id.into(),
                        key_ranges: vec![astersql_pd_keyutil::KeyRange::new(vec![a], vec![b])],
                    }],
                    0,
                )
                .await
                .unwrap();
            manager.sync_group_from_etcd(
                &Group {
                    id: id.into(),
                    leader_store_id: 1,
                    voter_store_ids: vec![1, 2, 3],
                    ..Default::default()
                },
                &labels,
                0,
            );
            manager.restore_affinity_group(id, 0);
        }
        if c["label_split"] == true || c["deny"] == true {
            let rule=LabelRule::from_json(&serde_json::to_vec(&json!({"id":"merge","labels":[{"key":"merge_option","value":"deny"}],"rule_type":"key-range","data":[{"start_key":if c["label_split"]==true {"03"}else{"01"},"end_key":"09"}]})).unwrap(),0).unwrap();
            labels.set_label_rule(rule, 0).await.unwrap();
        }
        let mut rules = RuleManager::new(storage.clone(), paths.clone());
        rules
            .initialize(3, &[], "", false, &conf, &[])
            .await
            .unwrap();
        if c["rule_split"] == true {
            rules
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "boundary".into(),
                        start_key: vec![3],
                        end_key: vec![9],
                        role: PeerRoleType::Voter,
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
        if c["learners"] == true {
            rules
                .set_rule(
                    Rule {
                        group_id: "learner".into(),
                        id: "l".into(),
                        role: PeerRoleType::Learner,
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
        if c["target_isolation"] == true {
            rules
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "default".into(),
                        role: PeerRoleType::Voter,
                        count: 3,
                        location_labels: vec!["zone".into()],
                        isolation_level: "zone".into(),
                        ..Default::default()
                    },
                    3,
                    &conf,
                    &stores
                        .get_stores()
                        .iter()
                        .map(|s| (**s).clone())
                        .collect::<Vec<_>>(),
                )
                .await
                .unwrap();
        }
        let source = region(
            1,
            2,
            3,
            num(c, "size", 2),
            num(c, "keys", 20000),
            1,
            c["source_pending"] == true,
            false,
            if c["learners"] == true { 4 } else { 0 },
        );
        let prev = region(
            2,
            1,
            2,
            num(c, "prev_size", 2),
            20000,
            1,
            false,
            false,
            if c["learners"] == true { 4 } else { 0 },
        );
        let next = region(
            3,
            if c["gap"] == true { 4 } else { 3 },
            5,
            num(c, "next_size", 2),
            num(c, "next_keys", 20000),
            num(c, "target_leader", 1) as u64,
            c["pending"] == true,
            c["target_missing"] == true,
            if c["learners"] == true { 5 } else { 0 },
        );
        let now = num(c, "now_ns", 100000000000) as i128;
        let ctx = AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &conf,
                    stores: &stores,
                    rules: &[],
                    now_unix_nanos: now,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: c["deleted"] != true,
        };
        let mut checker = AffinityChecker::new(num(c, "start_ns", 0) as i128);
        if c["paused"] == true {
            checker.pause.pause_or_resume_at(200, 0);
        }
        if c["cache_source"] == true || c["cache_target"] == true {
            let op = astersql_pd_schedule_operator::Operator::new(
                "cached",
                "",
                if c["cache_source"] == true { 1 } else { 3 },
                None,
                OpKind::AFFINITY,
                0,
                vec![],
            );
            op.additional_info().set("related merge region", "99");
            checker.record_op_success(&op, 1, 100);
        }
        let result = checker
            .check_with_neighbors(
                &ctx,
                &mut rules,
                &mut manager,
                &source,
                Some(&prev),
                if c["no_next"] == true {
                    None
                } else {
                    Some(&next)
                },
                &mut || -> Result<u64, BuilderError> {
                    if c["allocation_fail"] == true {
                        Err(BuilderError::Allocation("injected".into()))
                    } else {
                        assert_eq!(c["learners"], true);
                        Ok(999)
                    }
                },
            )
            .await;
        if c["error"] == "allocation" {
            assert_eq!(
                result.err(),
                Some(BuilderError::Allocation("injected".into())),
                "{name}"
            );
            continue;
        }
        let ops = result.unwrap();
        assert_eq!(
            ops.last().map(|o| o.region_id()).unwrap_or(0),
            c["expected"].as_u64().unwrap(),
            "{name}"
        );
        if !ops.is_empty() {
            assert_eq!(ops.len(), 2, "{name}");
            for (i, op) in ops.iter().enumerate() {
                assert_eq!(op.desc(), "affinity-merge-region");
                assert_eq!(
                    op.kind(),
                    if c["learners"] == true {
                        OpKind::AFFINITY | OpKind::REGION
                    } else {
                        OpKind::AFFINITY
                    }
                );
                assert_eq!(op.region_epoch(), source.meta().epoch.as_ref());
                assert_eq!(
                    op.additional_info().related_merge_region(),
                    ops[1 - i].region_id()
                );
                if c["learners"] != true || i == 1 {
                    assert_eq!(op.len(), 1);
                } else {
                    assert!(op.len() > 1);
                }
                let OpStep::MergeRegion(s) = op.step(op.len() - 1).unwrap() else {
                    panic!("merge step")
                };
                assert_eq!(s.is_passive, i == 1);
                assert_eq!(s.from_region.as_ref(), Some(source.meta()));
                assert_eq!(s.to_region.as_ref().unwrap().id, ops[1].region_id());
                let command = s.get_cmd(
                    if i == 0 {
                        &source
                    } else if op.region_id() == 2 {
                        &prev
                    } else {
                        &next
                    },
                    false,
                );
                assert_eq!(command.is_some(), i == 0);
            }
            if c["learners"] == true {
                let mut peers = source.meta().peers.clone();
                for i in 0..ops[0].len() - 1 {
                    match ops[0].step(i).unwrap() {
                        OpStep::AddLearner(step) => {
                            assert_eq!(step.to_store, 5);
                            assert_eq!(step.peer_id, 999);
                            peers.push(Peer {
                                id: step.peer_id,
                                store_id: step.to_store,
                                role: astersql_pd_core::PeerRole::Learner,
                                ..Default::default()
                            });
                        }
                        OpStep::RemovePeer(step) => {
                            assert_eq!(step.from_store, 4);
                            peers.retain(|p| p.store_id != step.from_store);
                        }
                        step => panic!("unexpected learner alignment step {step:?}"),
                    }
                }
                assert_eq!(
                    peers
                        .iter()
                        .filter(|p| p.role == astersql_pd_core::PeerRole::Learner)
                        .map(|p| p.store_id)
                        .collect::<Vec<_>>(),
                    vec![5]
                );
                assert_eq!(
                    peers
                        .iter()
                        .filter(|p| p.role == astersql_pd_core::PeerRole::Voter)
                        .map(|p| p.store_id)
                        .collect::<Vec<_>>(),
                    vec![1, 2, 3]
                );
            }
            if name == "same-group-next" {
                use astersql_pd_schedule_operator::{ControllerRecords, SUCCESS};
                let feedback = Arc::new(std::sync::Mutex::new(AffinityChecker::new(0)));
                let sink = feedback.clone();
                let mut records = ControllerRecords::default();
                records.set_success_callbacks(vec![Some(Arc::new(move |op| {
                    sink.lock().unwrap().record_op_success(op, 1, 100)
                }))]);
                let passive = Arc::new(ops.into_iter().nth(1).unwrap());
                assert!(passive.start());
                assert!(!feedback.lock().unwrap().recently_merged(1, 100));
                let merged = region(3, 2, 5, 4, 40000, 1, false, false, 0);
                passive.check(&merged);
                assert_eq!(passive.status(), SUCCESS);
                let now = std::time::Instant::now();
                records.bury_operator(passive.clone(), now).unwrap();
                assert!(feedback.lock().unwrap().recently_merged(1, 100));
                assert!(feedback.lock().unwrap().recently_merged(3, 100));
                assert!(records.bury_operator(passive, now).is_none());
                continue;
            }
            checker.record_op_success(&ops[0], 1, now as u64 / 1000000000);
            assert!(checker.recently_merged(1, now as u64 / 1000000000));
            assert!(checker.recently_merged(ops[1].region_id(), now as u64 / 1000000000));
        }
    }
    println!(
        "task_050: {} frozen cases passed",
        fixture["cases"].as_array().unwrap().len()
    );
}
