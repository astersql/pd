// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    KeyRange, NodeState, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta,
    StoreStats, StoresInfo,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_affinity::{
    group::{Availability, Group},
    manager::Manager,
    ranges::GroupKeyRanges,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{BuilderError, OpKind, OpStep};
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use serde_json::Value;
use std::sync::Arc;
#[tokio::test]
async fn task_049_behavior_cases() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_049.json"
    ))
    .unwrap();
    for c in data["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = c["disabled"] != true;
        conf.schedule.enable_joint_consensus = false;
        conf.schedule.region_score_formula_version = "v2".into();
        conf.schedule.max_snapshot_count = 64;
        conf.label_properties.insert(
            "reject-leader".into(),
            vec![StoreLabel {
                key: "reject".into(),
                value: "leader".into(),
            }],
        );
        let stores = StoresInfo::new();
        for id in 1..=6 {
            if c["missing"] == id {
                continue;
            }
            let mut s = StoreInfo::new(StoreMeta {
                id,
                node_state: NodeState::Serving,
                labels: vec![StoreLabel {
                    key: "zone".into(),
                    value: if id == 4 {
                        "z1".into()
                    } else {
                        format!("z{id}")
                    },
                }],
                ..Default::default()
            });
            s.set_region_size(id as i64);
            s.set_leader_count(id as i64);
            s.set_last_heartbeat_unix_nanos(100_000_000_000);
            s.update_raw_stats(StoreStats {
                capacity: 1_000_000_000_000,
                available: 900_000_000_000,
                used_size: 100_000_000_000,
                ..Default::default()
            });
            if c["evict"] == id {
                s.pause_leader_transfer(astersql_pd_core_constant::Direction::IN)
            }
            if c["reject"] == id {
                s.set_labels(vec![StoreLabel {
                    key: "reject".into(),
                    value: "leader".into(),
                }])
            }
            stores.put_store(Arc::new(s));
        }
        let backend = FaultBoundary::new();
        let storage = Arc::new(StorageEndpoint::new(backend.clone()));
        let paths = ClusterPaths::new(49);
        let mut labels =
            RegionLabeler::load(StorageEndpoint::new(backend.clone()), paths.clone(), 0)
                .await
                .unwrap();
        let mut manager = Manager::default();
        if c["no_group"] != true {
            manager
                .create_affinity_groups(
                    &storage,
                    &paths,
                    &mut labels,
                    &[GroupKeyRanges {
                        group_id: "g".into(),
                        key_ranges: vec![astersql_pd_keyutil::KeyRange::new(vec![1], vec![9])],
                    }],
                    100,
                )
                .await
                .unwrap();
            let voters: Vec<u64> = c["target"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap())
                .collect();
            manager.sync_group_from_etcd(
                &Group {
                    id: "g".into(),
                    leader_store_id: c["leader"].as_u64().unwrap(),
                    voter_store_ids: voters,
                    ..Default::default()
                },
                &labels,
                100,
            );
            manager.restore_affinity_group("g", 100);
            if c["degraded"] == true {
                manager.update_group_availability("g", Availability::Degraded, 100)
            }
            if c["expired"] == true {
                manager.expire_affinity_group("g", 100)
            }
        }
        let mut peers: Vec<Peer> = c["current"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let id = v.as_u64().unwrap();
                Peer {
                    id: 100 + id,
                    store_id: id,
                    role: PeerRole::Voter,
                    ..Default::default()
                }
            })
            .collect();
        for id in c["learners"].as_array().unwrap() {
            let id = id.as_u64().unwrap();
            peers.push(Peer {
                id: 100 + id,
                store_id: id,
                role: PeerRole::Learner,
                ..Default::default()
            });
        }
        let r = Arc::new(RegionInfo::new(
            RegionMeta {
                id: 49,
                epoch: Some(astersql_pd_core::RegionEpoch {
                    version: 9,
                    conf_ver: 7,
                }),
                key_range: KeyRange {
                    start_key: vec![if c["outside"] == true { 10 } else { 2 }],
                    end_key: vec![if c["overlap"] == true { 10 } else { 3 }],
                },
                peers: peers.clone(),
                ..Default::default()
            },
            if c["no_leader"] == true {
                None
            } else {
                Some(peers[0].clone())
            },
            vec![],
            if c["pending"] == true {
                vec![peers[1].clone()]
            } else {
                vec![]
            },
        ));
        let mut rules = RuleManager::new(storage.clone(), paths.clone());
        rules
            .initialize(
                3,
                &[],
                "",
                false,
                &conf,
                &stores
                    .get_stores()
                    .iter()
                    .map(|s| (**s).clone())
                    .collect::<Vec<_>>(),
            )
            .await
            .unwrap();
        if c["location"] == true {
            rules
                .set_rule(
                    Rule {
                        group_id: "pd".into(),
                        id: "default".into(),
                        role: PeerRoleType::Voter,
                        count: 3,
                        location_labels: vec!["zone".into()],
                        isolation_level: if c["isolation"] == true {
                            "zone".into()
                        } else {
                            String::new()
                        },
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
        if !c["learners"].as_array().unwrap().is_empty() {
            rules
                .set_rule(
                    Rule {
                        group_id: "learner".into(),
                        id: "l".into(),
                        role: PeerRoleType::Learner,
                        count: c["learners"].as_array().unwrap().len() as i64,
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
        let ctx = AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config: &conf,
                    stores: &stores,
                    rules: &[],
                    now_unix_nanos: 100_000_000_000,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels: &labels,
            storage: &storage,
            paths: &paths,
            region_exists: c["deleted"] != true,
        };
        let checker = AffinityChecker::default();
        if c["paused"] == true {
            checker.pause.pause_or_resume_at(60, 100)
        }
        let mut next = 200;
        let mut allocate = || {
            if c["allocation_fail"] == true {
                return Err(BuilderError::Allocation("injected".into()));
            }
            next += 1;
            Ok(next)
        };
        if c["storage_fail"] == true {
            backend
                .fail
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let result = if c["direct"] == true {
            checker
                .create_affinity_operator(
                    &ctx,
                    &rules,
                    &r,
                    &manager.group_observation("g", 100).unwrap().state,
                    &mut allocate,
                )
                .await
        } else {
            checker
                .check(&ctx, &mut rules, &mut manager, &r, &mut allocate)
                .await
        };
        if c["allocation_fail"] == true {
            assert_eq!(
                result.err(),
                Some(BuilderError::Allocation("injected".into())),
                "{name}"
            );
            continue;
        }
        let mut op = result.unwrap();
        if c["recover"] == true {
            assert!(op.is_none());
            manager.restore_affinity_group("g", 100);
            op = checker
                .check(&ctx, &mut rules, &mut manager, &r, &mut allocate)
                .await
                .unwrap();
        }
        assert_eq!(op.is_some(), c["op"] == true, "{name}");
        if c["storage_fail"] == true {
            let disk_before: Group =
                serde_json::from_slice(&storage.load_affinity_group(&paths, "g").await.unwrap())
                    .unwrap();
            assert!(disk_before.voter_store_ids.is_empty());
            let g = manager.group_observation("g", 100).unwrap().state;
            assert!(!g.affinity_scheduling_allowed);
            assert_eq!(g.group.voter_store_ids, vec![1, 2, 3, 4]);
            backend
                .fail
                .store(false, std::sync::atomic::Ordering::SeqCst);
            assert!(
                checker
                    .check(&ctx, &mut rules, &mut manager, &r, &mut allocate)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                manager
                    .group_observation("g", 100)
                    .unwrap()
                    .state
                    .group
                    .voter_store_ids,
                vec![1, 2, 3]
            );
        }
        if c["storage_fail"] == true || c["normalize"] == true {
            let disk: Group =
                serde_json::from_slice(&storage.load_affinity_group(&paths, "g").await.unwrap())
                    .unwrap();
            assert_eq!(disk.voter_store_ids, vec![1, 2, 3], "{name}");
            assert_eq!(disk.leader_store_id, 1);
        }
        if c["normalize"] == true {
            let g = manager.group_observation("g", 100).unwrap().state;
            assert_eq!(g.group.voter_store_ids, vec![1, 2, 3], "{name}");
            assert_eq!(g.group.leader_store_id, 1);
            assert!(g.affinity_scheduling_allowed);
            assert_eq!(g.phase, "stable");
        }
        if c["deleted"] == true {
            assert_eq!(manager.groups["g"].info.region_count, 0);
        }
        if let Some(op) = op {
            assert_eq!(op.desc(), "affinity-move-region");
            assert_eq!(op.region_id(), 49);
            assert_eq!(op.region_epoch(), r.meta().epoch.as_ref());
            assert_eq!(
                op.priority_level(),
                astersql_pd_core_constant::PriorityLevel::MEDIUM
            );
            assert!(op.kind().0 & OpKind::AFFINITY.0 != 0);
            assert_eq!(op.scheduler_kind(), OpKind::AFFINITY);
            let mut final_peers = peers.clone();
            let mut leader = peers[0].store_id;
            for i in 0..op.len() {
                let step = op.step(i).unwrap();
                match step {
                    OpStep::TransferLeader(s) => {
                        assert_eq!(s.from_store, leader);
                        leader = s.to_store
                    }
                    OpStep::AddLearner(s) => final_peers.push(Peer {
                        id: s.peer_id,
                        store_id: s.to_store,
                        role: PeerRole::Learner,
                        is_witness: s.is_witness,
                    }),
                    OpStep::PromoteLearner(s) => {
                        final_peers
                            .iter_mut()
                            .find(|p| p.store_id == s.to_store)
                            .unwrap()
                            .role = PeerRole::Voter
                    }
                    OpStep::RemovePeer(s) => {
                        assert!(
                            !c["learners"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|v| *v == s.from_store),
                            "learner removed"
                        );
                        final_peers.retain(|p| p.store_id != s.from_store)
                    }
                    _ => panic!("unexpected {step:?}"),
                }
            }
            let mut voters: Vec<_> = final_peers
                .iter()
                .filter(|p| p.role == PeerRole::Voter)
                .map(|p| p.store_id)
                .collect();
            voters.sort();
            assert_eq!(
                voters,
                c["target"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect::<Vec<_>>(),
                "{name}"
            );
            assert_eq!(leader, c["leader"].as_u64().unwrap());
            let learners: Vec<_> = final_peers
                .iter()
                .filter(|p| p.role == PeerRole::Learner)
                .map(|p| p.store_id)
                .collect();
            assert_eq!(
                learners,
                c["learners"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect::<Vec<_>>(),
                "{name}"
            );
            assert_eq!(
                op.kind().0 & OpKind::LEADER.0 != 0,
                peers[0].store_id != leader
            );
        }
    }
    println!(
        "task_049: {} fixed behavior cases passed",
        data["cases"].as_array().unwrap().len()
    );
}

use astersql_pd_storage_domain::{
    Commit, Compare, Lease, LeaseId, MetaStore, MetaWatch, Mutation, RawTxnOp, RawTxnResponse,
    Read, Revision, Snapshot, StoreError,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
struct FaultBoundary {
    inner: Arc<MemoryStore>,
    fail: AtomicBool,
}
impl FaultBoundary {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(MemoryStore::default()),
            fail: AtomicBool::new(false),
        })
    }
}
#[async_trait]
impl MetaStore for FaultBoundary {
    async fn read(&self, r: Read) -> astersql_pd_storage_domain::Result<Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_domain::Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: Revision,
    ) -> astersql_pd_storage_domain::Result<Box<dyn MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> astersql_pd_storage_domain::Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> astersql_pd_storage_domain::Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> astersql_pd_storage_domain::Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> astersql_pd_storage_domain::Result<()> {
        self.inner.compact(r).await
    }
}

#[test]
fn task_049_virtual_placement_and_success_cache() {
    use astersql_pd_schedule_operator::Operator;
    let peers: Vec<_> = (1..=4)
        .map(|id| Peer {
            id: 100 + id,
            store_id: id,
            role: if id == 4 {
                PeerRole::Learner
            } else {
                PeerRole::Voter
            },
            ..Default::default()
        })
        .collect();
    let r = RegionInfo::new(
        RegionMeta {
            id: 49,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    let v = clone_region_with_replace_peer_stores(&r, 5, &[2, 3, 5]).unwrap();
    assert_eq!(v.leader().unwrap().store_id, 5);
    assert_eq!(v.get_store_peer(5).unwrap().id, 101);
    assert_eq!(v.learners(), r.learners());
    assert_eq!(r.leader().unwrap().store_id, 1);
    for (leader, target) in [(2, vec![2, 3]), (1, vec![2, 3, 5]), (2, vec![2, 2, 3])] {
        assert!(clone_region_with_replace_peer_stores(&r, leader, &target).is_none());
    }
    let mut checker = AffinityChecker::default();
    assert_eq!(checker.get_type(), "affinity-checker");
    let op = Operator::new("test", "test", 49, None, OpKind::AFFINITY, 1, vec![]);
    checker.record_op_success(&op, 1, 100);
    assert!(!checker.recently_merged(49, 100));
    for related in ["", "0", "garbage"] {
        op.additional_info()
            .set(astersql_pd_schedule_operator::RELATED_MERGE_REGION, related);
        checker.record_op_success(&op, 1, 100);
        assert!(!checker.recently_merged(49, 100));
    }
    let other = Operator::new("other", "other", 50, None, OpKind::MERGE, 1, vec![]);
    let mut op = op;
    op.sync(&other);
    checker.record_op_success(&op, 0, 100);
    assert!(!checker.recently_merged(49, 100));
    checker.record_op_success(&op, 1, 100);
    assert!(checker.recently_merged(49, 159));
    assert!(checker.recently_merged(50, 159));
    assert!(checker.recently_merged(49, 160));
    assert!(!checker.recently_merged(49, 161));
    assert!(checker.recently_merged(50, 160));
    assert!(!checker.recently_merged(50, 161));
    checker.record_op_success(&op, 1, 150);
    assert!(checker.recently_merged(49, 209));
    assert!(checker.recently_merged(49, 210));
    assert!(!checker.recently_merged(49, 211));
}
