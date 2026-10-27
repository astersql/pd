// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, DownPeer, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::{Direction, PriorityLevel};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{BuilderError, OpKind, OpStep};
use astersql_pd_statistics::{HotPeerStat, StoreLoadDetail, StoreLoadPred, StoreSummaryInfo};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{collections::HashSet, sync::Arc};
#[tokio::test]
async fn task_096_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_096.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        for v2 in [false, true] {
            let mut c = BasicCluster::new();
            for id in 1..=5 {
                let mut s = StoreInfo::new(StoreMeta {
                    id,
                    ..Default::default()
                });
                s.set_last_heartbeat_unix_nanos(100);
                if case["limit"] == true && id == 2 {
                    let limit = astersql_pd_storelimit::StoreRateLimit::new(0.000000001);
                    assert!(
                        limit
                            .take(
                                1000,
                                astersql_pd_storelimit::Type::TRANSFER_LEADER_IN,
                                PriorityLevel::HIGH
                            )
                            .await
                    );
                    s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
                }
                c.stores.put_store(Arc::new(s));
            }
            let peers: Vec<_> = (1..=3)
                .map(|id| Peer {
                    id,
                    store_id: id,
                    ..Default::default()
                })
                .collect();
            let r = Arc::new(RegionInfo::new(
                RegionMeta {
                    id: 1,
                    peers: peers.clone(),
                    ..Default::default()
                },
                Some(peers[0].clone()),
                if case["down"] == true {
                    vec![DownPeer {
                        peer: peers[1].clone(),
                        down_seconds: 100,
                    }]
                } else {
                    vec![]
                },
                if case["pending"] == true {
                    vec![peers[1].clone()]
                } else {
                    vec![]
                },
            ));
            if case["missing"] != true {
                let (o, v, b) = c.regions.set_region(r.clone());
                c.regions.update_sub_tree(r, o.as_deref(), &v, b);
            }
            if case["pause"] == true {
                c.stores.pause_leader_transfer(2, Direction::IN).unwrap();
            }
            let config = ProviderSnapshot::<()>::default();
            let mut rules = astersql_pd_schedule_placement::RuleManager::new(
                Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
                ClusterPaths::new(96),
            );
            rules
                .initialize(3, &[], "", false, &config, &[])
                .await
                .unwrap();
            let source = case["source"].as_u64().unwrap();
            let scheduler = GrantHotScheduler::new(GrantHotConfig {
                leader_id: case["leader"].as_u64().unwrap(),
                store_ids: case["stores"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect(),
            });
            let detail = StoreLoadDetail {
                store_summary_info: StoreSummaryInfo {
                    store_info: c.stores.get_store(source).unwrap().as_ref().clone(),
                    ..Default::default()
                },
                load_pred: StoreLoadPred::default(),
                hot_peers: if case["empty"] == true {
                    vec![]
                } else {
                    vec![HotPeerStat {
                        region_id: 1,
                        store_id: source,
                        ..Default::default()
                    }]
                },
            };
            let mut active = HashSet::new();
            if case["active"] == true {
                active.insert(1);
            }
            let mut alloc = || {
                if case["alloc_fail"] == true {
                    Err(BuilderError::Allocation("fault".into()))
                } else {
                    Ok(100)
                }
            };
            let mut ctx = GrantHotContext {
                cluster: &c,
                config: &config,
                rules: &mut rules,
                now: 100,
                supports_conf_change_v2: v2,
                allocate: &mut alloc,
                active_regions: &active,
            };
            let mut details = vec![detail];
            if case["sort"] == true {
                let mut d = details[0].clone();
                d.load_pred.current.loads[0] = 100.;
                d.hot_peers[0].region_id = 99;
                details.push(d);
            }
            let out = scheduler
                .dispatch(
                    &details,
                    case["branch"].as_bool().unwrap(),
                    case["random"].as_u64().unwrap_or(0) as usize,
                    &mut ctx,
                )
                .await;
            assert_eq!(
                out.operators.len(),
                case["expected"].as_u64().unwrap() as usize,
                "{case} v2={v2}, errors={:?}",
                out.errors
            );
            for op in &out.operators {
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.priority_level(), PriorityLevel::HIGH);
                if case["branch"] == true {
                    assert_eq!(op.kind(), OpKind::LEADER);
                    assert_eq!(op.len(), 1);
                    let OpStep::TransferLeader(s) = op.step(0).unwrap() else {
                        panic!()
                    };
                    assert_eq!((s.from_store, s.to_store), (1, 2));
                    assert_eq!(s.conf_ver_changed(&c.regions.get_region(1).unwrap()), 0);
                    assert_eq!(
                        s.get_cmd(&c.regions.get_region(1).unwrap(), v2)
                            .transfer_leader
                            .unwrap()
                            .peer
                            .unwrap()
                            .store_id,
                        2
                    );
                } else {
                    assert_eq!(
                        op.kind().0 & (OpKind::REGION | OpKind::LEADER).0,
                        (OpKind::REGION | OpKind::LEADER).0
                    );
                    let target = case["target"].as_u64().unwrap_or(4);
                    let steps: Vec<_> = (0..op.len()).map(|i| op.step(i).unwrap()).collect();
                    assert!(
                        steps
                            .iter()
                            .any(|step| matches!(step,OpStep::RemovePeer(s) if s.from_store==2))
                    );
                    let add = steps
                        .iter()
                        .find_map(|step| match step {
                            OpStep::AddLearner(s) => Some(s),
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!((add.to_store, add.peer_id), (target, 100));
                    let cmd = add.get_cmd(&c.regions.get_region(1).unwrap(), v2).unwrap();
                    let change = if v2 {
                        cmd.change_peer_v2.unwrap().changes[0].clone().unwrap()
                    } else {
                        cmd.change_peer.unwrap()
                    };
                    assert_eq!(change.peer.unwrap().store_id, target);
                    let influence = astersql_pd_schedule_operator::OpInfluence::new();
                    op.total_influence(&influence, Some(&c.regions.get_region(1).unwrap()));
                    assert_eq!(influence.get_store_influence(2).region_count, -1);
                    assert_eq!(influence.get_store_influence(target).region_count, 1);
                    assert_eq!(op.conf_ver_changed(&c.regions.get_region(1).unwrap()), 0);
                }
            }
            if case["name"] == "leader" {
                assert!(matches!(
                    scheduler.transfer(1, 99, true, 0, &mut ctx).await,
                    Err(GrantHotError::Source)
                ));
                for resource in 0..4 {
                    let mut resources: [Vec<StoreLoadDetail>; 4] = std::array::from_fn(|_| vec![]);
                    resources[resource] = details.clone();
                    let sampled = scheduler
                        .schedule(&resources, [resource + 4, 1, 0], &mut ctx)
                        .await;
                    assert_eq!(sampled.operators.len(), 1);
                    let empty = scheduler
                        .schedule(&resources, [(resource + 1) % 4, 1, 0], &mut ctx)
                        .await;
                    assert!(empty.operators.is_empty());
                }
            }
            if case["limit"] == true {
                c.stores.get_store(2).unwrap().reset_store_limit(
                    astersql_pd_storelimit::Type::TRANSFER_LEADER_IN,
                    &[astersql_pd_storelimit::UNLIMITED],
                );
                let op = scheduler.transfer(1, 1, true, 0, &mut ctx).await.unwrap();
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!()
                };
                assert_eq!(step.to_store, 2);
            }
            if case["sort"] == true {
                assert_eq!(out.errors.len(), 1);
                assert!(matches!(out.errors[0], GrantHotError::Region));
            }
            if case["alloc_fail"] == true {
                assert!(matches!(
                    &out.errors[0],
                    GrantHotError::Builder(BuilderError::Allocation(_))
                ));
            }
        }
    }
    println!(
        "task_096: {} source-derived scenarios x 2 protocol modes",
        fixture["cases"].as_array().unwrap().len()
    );
}
#[test]
fn task_096_config_and_limits() {
    let mut c = GrantHotConfig::from_args(&["2", "1,1"]).unwrap();
    assert_eq!(c.store_ids, vec![1, 1, 2]);
    c.set_store(1, vec![1, 2]);
    assert_eq!(c.store_ids, vec![1, 2]);
    let mut clone = c.clone();
    clone.store_ids.push(9);
    assert_eq!(c.store_ids, vec![1, 2]);
    for args in [
        vec![],
        vec!["1"],
        vec!["1", ""],
        vec!["-1", "2"],
        vec!["1", " 2"],
        vec!["18446744073709551616", "2"],
        vec!["+1", "2"],
    ] {
        assert!(GrantHotConfig::from_args(&args).is_err());
    }
    for (r, rl, l, ll, want) in [
        (0, 1, 0, 1, true),
        (1, 1, 0, 1, false),
        (0, 1, 1, 1, false),
        (0, 0, 0, 1, false),
        (0, 1, 0, 0, false),
    ] {
        assert_eq!(GrantHotScheduler::is_schedule_allowed(r, rl, l, ll), want);
    }
}
#[tokio::test]
async fn task_096_persistence_faults() {
    let backend = Arc::new(FaultStore::default());
    let storage = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(96);
    let original = GrantHotConfig::from_args(&["2", "1,2,3"]).unwrap();
    let mut s = GrantHotScheduler::new(original.clone());
    s.persist(&storage, &paths).await.unwrap();
    for input in [
        serde_json::json!({"store-id":"1,2","store-leader-id":1}),
        serde_json::json!({"store-id":1,"store-leader-id":"2"}),
        serde_json::json!({"store-id":"x","store-leader-id":"2"}),
    ] {
        assert!(s.update_config(&input, &storage, &paths).await.is_err());
        assert_eq!(s.config(), &original);
    }
    backend.fail.store(true, Ordering::SeqCst);
    assert!(
        s.update_config(
            &serde_json::json!({"store-id":"3,4","store-leader-id":"4"}),
            &storage,
            &paths
        )
        .await
        .is_err()
    );
    assert_eq!(s.config().leader_id, 0);
    assert_eq!(s.config().store_ids, vec![3, 4]);
    assert!(s.reload(&storage, &paths).await.is_err());
    assert_eq!(s.config().leader_id, 0);
    backend.fail.store(false, Ordering::SeqCst);
    s.reload(&storage, &paths).await.unwrap();
    assert_eq!(s.config(), &original);
    s.update_config(
        &serde_json::json!({"store-id":"3","store-leader-id":"4"}),
        &storage,
        &paths,
    )
    .await
    .unwrap();
    assert_eq!(s.config().store_ids, vec![3, 4]);
    let saved = s.config().clone();
    storage
        .save_scheduler_config(&paths, "grant-hot-region-scheduler", b"broken")
        .await
        .unwrap();
    assert!(s.reload(&storage, &paths).await.is_err());
    assert_eq!(s.config(), &saved);
}

use std::sync::atomic::Ordering;
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    stall: std::sync::atomic::AtomicBool,
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
        if self.stall.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
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
