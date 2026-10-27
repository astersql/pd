// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, DownPeer, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo,
    StoreLabel, StoreMeta,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind, OpStep};
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;
#[tokio::test]
async fn task_102_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_102.json"
    ))
    .unwrap();
    let mut n = 0;
    for case in f["cases"].as_array().unwrap() {
        for v2 in [false, true] {
            let flag = |key: &str| case[key] == true;
            let mut c = BasicCluster::new();
            for id in 1..=if flag("no_target") {
                3
            } else if flag("last") {
                5
            } else {
                4
            } {
                let mut s = StoreInfo::new(StoreMeta {
                    id,
                    labels: vec![StoreLabel {
                        key: "zone".into(),
                        value: if id > 3 && flag("placement_reject") {
                            "1".into()
                        } else {
                            id.to_string()
                        },
                    }],
                    ..Default::default()
                });
                s.set_last_heartbeat_unix_nanos(if id > 3 && flag("disconnected_target") {
                    1
                } else {
                    1_000_000_000_000
                });
                if id > 3 && flag("offline_target") {
                    s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
                }
                if id > 3 && flag("special_target") {
                    s.set_labels(vec![StoreLabel {
                        key: "specialUse".into(),
                        value: "reserved".into(),
                    }]);
                }
                if id > 3 && flag("budget") {
                    let limit = astersql_pd_storelimit::StoreRateLimit::new(0.000000001);
                    assert!(
                        limit
                            .take(
                                1000,
                                astersql_pd_storelimit::Type::ADD_PEER,
                                PriorityLevel::MEDIUM
                            )
                            .await
                    );
                    s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
                }
                c.stores.put_store(Arc::new(s));
            }
            let peers: Vec<_> = (1..=if flag("replica_short") { 2 } else { 3 })
                .map(|id| Peer {
                    id,
                    store_id: id,
                    role: if id == 3 && flag("learner") {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    },
                    ..Default::default()
                })
                .collect();
            let r = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: 1,
                    key_range: KeyRange {
                        start_key: b"a".to_vec(),
                        end_key: b"b".to_vec(),
                    },
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                if flag("down") {
                    vec![DownPeer {
                        peer: peers[1].clone(),
                        down_seconds: 0,
                    }]
                } else {
                    vec![]
                },
                if flag("pending") {
                    vec![peers[1].clone()]
                } else {
                    vec![]
                },
            ));
            if !flag("no_region") {
                let (o, v, b) = c.regions.set_region(r.clone());
                c.regions.update_sub_tree(r.clone(), o.as_deref(), &v, b);
            }
            let mut conf = ProviderSnapshot::<()>::default();
            conf.replication.max_replicas = 3;
            conf.replication.enable_placement_rules = flag("rules");
            conf.replication.location_labels = vec!["zone".into()].into();
            let endpoint = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
            let mut rules = RuleManager::new(endpoint, ClusterPaths::new(102));
            rules
                .initialize(3, &["zone".into()], "", false, &conf, &[])
                .await
                .unwrap();
            if flag("learner") && !flag("legacy") {
                conf.replication.enable_placement_rules = true;
                let stores = c
                    .stores
                    .get_stores()
                    .iter()
                    .map(|s| s.as_ref().clone())
                    .collect::<Vec<_>>();
                rules
                    .set_rule(
                        Rule {
                            group_id: "pd".into(),
                            id: "default".into(),
                            role: PeerRoleType::Voter,
                            count: 2,
                            ..Default::default()
                        },
                        0,
                        &conf,
                        &stores,
                    )
                    .await
                    .unwrap();
                rules
                    .set_rule(
                        Rule {
                            group_id: "pd".into(),
                            id: "learner".into(),
                            role: PeerRoleType::Learner,
                            count: 1,
                            ..Default::default()
                        },
                        0,
                        &conf,
                        &stores,
                    )
                    .await
                    .unwrap();
            }
            let mut config = ShuffleRegionConfig::from_args(&[]).unwrap();
            // Stored Go strings are decoded without the role update validator.
            if case["roles"][0] == "voter" {
                config = decode_config(br#"{"roles":["voter"]}"#).unwrap();
            } else {
                config
                    .set_roles(
                        case["roles"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_owned())
                            .collect(),
                    )
                    .unwrap();
            }
            if let Some(a) = case["ranges"].as_array() {
                let roles = config.roles().unwrap().to_vec();
                let args = a.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>();
                config = ShuffleRegionConfig::from_args(&args).unwrap();
                if a.is_empty() {
                    config = decode_config(
                        &serde_json::to_vec(&serde_json::json!({"ranges":[],"roles":roles}))
                            .unwrap(),
                    )
                    .unwrap();
                } else {
                    config.set_roles(roles).unwrap();
                }
            }
            let scheduler = ShuffleRegionScheduler::new(config);
            let mut index = |len| if flag("last") { len - 1 } else { 0 };
            let mut shuffle = |v: &mut [usize]| {
                if flag("reverse") {
                    v.reverse()
                }
            };
            let mut ctx = EvictLeaderContext {
                cluster: &c,
                config: &conf,
                rules: &mut rules,
                now_unix_nanos: 1_000_000_000_000,
                random_index: &mut index,
                shuffle: &mut shuffle,
            };
            let mut allocations = 0;
            let out = scheduler
                .schedule(&mut ctx, v2, &mut || {
                    allocations += 1;
                    if flag("allocation_failure") {
                        Err(BuilderError::Allocation("injected".into()))
                    } else {
                        Ok(10)
                    }
                })
                .await;
            assert_eq!(
                out.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{} v2={v2}, errors={:?}",
                case["name"],
                out.errors
            );
            assert_eq!(
                out.errors.len(),
                usize::from(flag("allocation_failure")),
                "{}",
                case["name"]
            );
            assert_eq!(
                allocations,
                usize::from(case["expected"] == 1 || flag("allocation_failure"))
            );
            if let Some(op) = out.operators.first() {
                let source = case["source"].as_u64().unwrap();
                let target = case["target"].as_u64().unwrap();
                assert_eq!(op.desc(), "shuffle-region-scheduler");
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.priority_level(), PriorityLevel::LOW);
                assert!(op.kind().0 & OpKind::REGION.0 != 0);
                assert!(op.kind().0 & OpKind::ADMIN.0 == 0);
                let steps = (0..op.len())
                    .map(|i| op.step(i).unwrap())
                    .collect::<Vec<_>>();
                assert!(
                    matches!(steps[0],OpStep::AddLearner(s) if s.to_store==target && s.peer_id==10 && !s.is_witness)
                );
                assert!(
                    matches!(steps.last().unwrap(),OpStep::RemovePeer(s) if s.from_store==source && s.peer_id==source)
                );
                let promotes = steps
                    .iter()
                    .any(|s| matches!(s, OpStep::PromoteLearner(_) | OpStep::ChangePeerV2Enter(_)));
                assert_eq!(promotes, !flag("learner"));
                assert_eq!(op.kind().0 & OpKind::LEADER.0 != 0, source == 1);
                assert_eq!(r.meta().peers, peers);
                assert!(op.start());
                let mut state = peers.clone();
                let mut leader = 1;
                let name = case["name"].as_str().unwrap();
                for i in 0..op.len() {
                    let step = op.step(i).unwrap();
                    let before = snapshot(&state, leader);
                    assert_eq!(op.check(&before), Some(step), "{name} pending {i}");
                    let influence = OpInfluence::new();
                    step.influence(&influence, &before);
                    assert!(!step.timeout(10).is_zero());
                    match step {
                        OpStep::AddLearner(s) => {
                            assert!(s.get_cmd(&before, true).is_some());
                            assert!(state.iter().all(|p| p.store_id != s.to_store));
                            state.push(Peer {
                                id: s.peer_id,
                                store_id: s.to_store,
                                role: PeerRole::Learner,
                                is_witness: s.is_witness,
                            });
                        }
                        OpStep::PromoteLearner(s) => {
                            let cmd = s.get_cmd(&before, true);
                            assert!(cmd.change_peer_v2.is_some());
                            let p = state.iter_mut().find(|p| p.store_id == s.to_store).unwrap();
                            assert_eq!(p.id, s.peer_id);
                            assert!(p.is_learner());
                            p.role = PeerRole::Voter;
                        }
                        OpStep::RemovePeer(s) => {
                            let cmd = s.get_cmd(&before, true);
                            assert!(cmd.change_peer_v2.is_some());
                            assert_ne!(leader, s.from_store);
                            assert_eq!(
                                state
                                    .iter()
                                    .find(|p| p.store_id == s.from_store)
                                    .unwrap()
                                    .id,
                                s.peer_id
                            );
                            state.retain(|p| p.store_id != s.from_store);
                        }
                        OpStep::TransferLeader(s) => {
                            let cmd = s.get_cmd(&before, true);
                            assert!(cmd.transfer_leader.is_some());
                            assert_eq!(leader, s.from_store);
                            leader = s.to_store;
                            let p = state.iter().find(|p| p.store_id == leader).unwrap();
                            assert!(
                                !p.is_learner()
                                    && !p.is_witness
                                    && p.role != PeerRole::DemotingVoter
                            );
                        }
                        OpStep::ChangePeerV2Enter(s) => {
                            assert!(s.get_cmd(&before, true).unwrap().change_peer_v2.is_some());
                            let single = s.promote_learners.len() + s.demote_voters.len() == 1;
                            for n in &s.promote_learners {
                                let p =
                                    state.iter_mut().find(|p| p.store_id == n.to_store).unwrap();
                                assert_eq!(p.id, n.peer_id);
                                p.role = if single {
                                    PeerRole::Voter
                                } else {
                                    PeerRole::IncomingVoter
                                };
                            }
                            for n in &s.demote_voters {
                                if single {
                                    assert_ne!(leader, n.to_store);
                                }
                                let p =
                                    state.iter_mut().find(|p| p.store_id == n.to_store).unwrap();
                                assert_eq!(p.id, n.peer_id);
                                p.role = if single {
                                    PeerRole::Learner
                                } else {
                                    PeerRole::DemotingVoter
                                };
                            }
                        }
                        OpStep::ChangePeerV2Leave(s) => {
                            assert!(s.get_cmd(&before, true).unwrap().change_peer_v2.is_some());
                            for p in &mut state {
                                p.role = match p.role {
                                    PeerRole::IncomingVoter => PeerRole::Voter,
                                    PeerRole::DemotingVoter => PeerRole::Learner,
                                    r => r,
                                };
                            }
                        }
                        _ => unreachable!(),
                    }
                    let after = snapshot(&state, leader);
                    assert!(step.is_finish(&after), "{name} ack {i}");
                    let expected_conf = match step {
                        OpStep::TransferLeader(_) => 0,
                        OpStep::ChangePeerV2Enter(s) => {
                            (s.promote_learners.len() + s.demote_voters.len()) as u64
                        }
                        OpStep::ChangePeerV2Leave(s) => {
                            (s.promote_learners.len() + s.demote_voters.len()) as u64
                        }
                        _ => 1,
                    };
                    assert_eq!(
                        step.conf_ver_changed(&after),
                        expected_conf,
                        "{name} confver {i}"
                    );
                }

                assert!(op.check(&snapshot(&state, leader)).is_none());
                assert!(op.check_success());
                assert!(state.iter().all(|p| p.store_id != source));
                assert_eq!(state.len(), peers.len());
                let moved = state.iter().find(|p| p.store_id == target).unwrap();
                assert_eq!(moved.id, 10);
                assert_eq!(
                    moved.role,
                    if flag("learner") {
                        PeerRole::Learner
                    } else {
                        PeerRole::Voter
                    }
                );
            }
            n += 1;
        }
    }
    println!("task_102: {n} source-derived behavior cases passed");
}
#[test]
fn task_102_config_and_limits() {
    let mut config = ShuffleRegionConfig::from_args(&["a%00", "b+"]).unwrap();
    assert!(config.is_role_allowed("learner"));
    let old = encode_config(&config).unwrap();
    assert!(
        matches!(config.set_roles(vec!["follower".into(),"voter".into()]),Err(ShuffleRegionError::InvalidRole(r)) if r=="voter")
    );
    assert_eq!(encode_config(&config).unwrap(), old);
    let restored: ShuffleRegionConfig = decode_config(&old).unwrap();
    assert_eq!(restored.ranges(), config.ranges());
    assert_eq!(config.ranges()[0].start_key, b"a\0");
    assert_eq!(config.ranges()[0].end_key, b"b ");
    for (count, limit, allowed) in [
        (0, 0, false),
        (2, 3, true),
        (3, 3, false),
        (4, 3, false),
        (u64::MAX, u64::MAX, false),
    ] {
        assert_eq!(
            ShuffleRegionScheduler::is_schedule_allowed(count, limit),
            allowed
        );
    }
    assert!(ShuffleRegionConfig::from_args(&["%zz", ""]).is_err());
    for data in [
        b"{".as_slice(),
        br#"{"roles":1}"#,
        br#"{"ranges":[{"start-key":"zz"}]}"#,
    ] {
        assert!(decode_config::<ShuffleRegionConfig>(data).is_err());
    }
    assert!(!ShuffleRegionConfig::default().is_role_allowed("leader"));
}
use std::sync::atomic::Ordering;
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl astersql_pd_storage_domain::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_domain::Result<Snapshot> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage_domain::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_domain::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_domain::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_domain::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_domain::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_domain::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_domain::Result<()> {
        self.store.compact(r).await
    }
}

use astersql_pd_storage_domain::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};

#[tokio::test]
async fn task_102_config_storage_faults() {
    let backend = Arc::new(FaultStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(102);
    let initial = ShuffleRegionConfig::from_args(&["a", "b"]).unwrap();
    let mut scheduler = ShuffleRegionScheduler::new(initial);
    scheduler.persist(&endpoint, &paths).await.unwrap();
    let original = scheduler.encode_config().unwrap();
    assert!(
        scheduler
            .update_roles(vec!["bad".into()], &endpoint, &paths)
            .await
            .is_err()
    );
    assert_eq!(scheduler.encode_config().unwrap(), original);
    backend.fail.store(true, Ordering::SeqCst);
    assert!(scheduler.persist(&endpoint, &paths).await.is_err());
    assert!(
        scheduler
            .update_roles(vec!["learner".into()], &endpoint, &paths)
            .await
            .is_err()
    );
    assert!(scheduler.reload(&endpoint, &paths).await.is_err());
    assert_eq!(scheduler.encode_config().unwrap(), original);
    backend.fail.store(false, Ordering::SeqCst);
    scheduler
        .update_roles(vec!["learner".into()], &endpoint, &paths)
        .await
        .unwrap();
    let mut restored = ShuffleRegionScheduler::new(ShuffleRegionConfig::default());
    restored.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(
        restored.encode_config().unwrap(),
        scheduler.encode_config().unwrap()
    );
    assert_eq!(restored.config().ranges(), scheduler.config().ranges());
    endpoint
        .save_scheduler_config(&paths, "shuffle-region-scheduler", b"{bad")
        .await
        .unwrap();
    assert!(restored.reload(&endpoint, &paths).await.is_err());
    assert!(restored.config().is_role_allowed("learner"));
    scheduler
        .update_roles(vec![], &endpoint, &paths)
        .await
        .unwrap();
    restored.reload(&endpoint, &paths).await.unwrap();
    assert_eq!(restored.config().roles(), Some([].as_slice()));
}

fn snapshot(state: &[Peer], leader: u64) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: state.to_vec(),
            ..Default::default()
        },
        state.iter().find(|p| p.store_id == leader).cloned(),
        vec![],
        vec![],
    )
}
