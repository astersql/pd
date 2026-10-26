// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreLabel,
    StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_filter::AffinityFilter;
use astersql_pd_schedule_operator::{BuilderError, OpInfluence, OpKind};
use astersql_pd_schedule_placement::{PeerRoleType, Rule, RuleManager};
use astersql_pd_statistics::StatisticsSnapshot;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
#[test]
fn task_105_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_105.json"
    ))
    .unwrap();
    for case in fixture["balance"].as_array().unwrap() {
        assert_eq!(
            BalanceRangeScheduler::should_balance(
                case[0].as_f64().unwrap(),
                case[1].as_f64().unwrap(),
                case[2].as_i64().unwrap(),
                case[3].as_i64().unwrap()
            ),
            case[4].as_bool().unwrap()
        );
    }
    let mut s = BalanceRangeScheduler::default();
    s.scores.insert(1, 1.0);
    s.scores.insert(2, 0.0);
    s.scores.insert(3, 0.0);
    for id in 1..=3 {
        s.expected.insert(id, 1.0 / 3.0);
    }
    assert!(s.is_balanced());
    s.scores.insert(1, 6.0);
    assert!(!s.is_balanced());
}
async fn run_case(
    role: &str,
    engine: &str,
    variant: &str,
    v2: bool,
    expected_ops: usize,
    expected_errors: usize,
) {
    let mut cluster = BasicCluster::new();
    for id in 1..=if variant == "existing" {
        3
    } else if variant == "single" {
        1
    } else {
        4
    } {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            labels: if engine == "tikv" {
                vec![]
            } else {
                vec![StoreLabel {
                    key: "engine".into(),
                    value: engine.into(),
                }]
            },
            ..Default::default()
        });
        let mut labels = s.labels().to_vec();
        labels.push(StoreLabel {
            key: "zone".into(),
            value: if variant == "topology_reject" && id == 4 {
                "2".into()
            } else {
                id.to_string()
            },
        });
        s.set_labels(labels);
        s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        if variant == "topology_reject" && matches!(id, 2 | 3) {
            s.pause_leader_transfer(astersql_pd_core_constant::Direction::IN);
        }
        cluster.stores.put_store(Arc::new(s));
    }
    for id in 1..=if matches!(variant, "balanced" | "timeout_balanced") {
        1
    } else {
        6
    } {
        let peers: Vec<_> = (1..=3)
            .map(|store_id| Peer {
                id: id * 10 + store_id,
                store_id,
                role: if role == "learner-scatter" && store_id == 3 {
                    PeerRole::Learner
                } else {
                    PeerRole::Voter
                },
                ..Default::default()
            })
            .collect();
        let r = Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            if variant == "pending" {
                vec![peers[1].clone()]
            } else {
                vec![]
            },
        ));
        let (o, v, b) = cluster.regions.set_region(r.clone());
        cluster.regions.update_sub_tree(r, o.as_deref(), &v, b);
    }
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    conf.replication.location_labels = vec!["zone".into()].into();
    conf.schedule.region_schedule_limit = 4;
    conf.schedule.max_store_down_time = "1h".parse().unwrap();
    conf.replication.enable_placement_rules = true;
    let backend = Arc::new(FaultStore::default());
    let endpoint = Arc::new(StorageEndpoint::new(backend.clone()));
    let paths = ClusterPaths::new(105);
    let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
        StorageEndpoint::new(backend.clone()),
        paths.clone(),
        0,
    )
    .await
    .unwrap();
    let mut rules = RuleManager::new(endpoint.clone(), paths.clone());
    rules
        .initialize(3, &[], "", false, &conf, &[])
        .await
        .unwrap();
    let constraints = if engine == "tiflash" {
        vec![astersql_pd_schedule_placement::LabelConstraint {
            key: "engine".into(),
            op: astersql_pd_schedule_placement::LabelConstraintOp::In,
            values: vec!["tiflash".into()],
        }]
    } else {
        vec![]
    };
    rules
        .set_rule(
            Rule {
                group_id: "pd".into(),
                id: "default".into(),
                role: PeerRoleType::Voter,
                count: 3,
                label_constraints: constraints.clone(),
                location_labels: vec!["zone".into()],
                ..Default::default()
            },
            0,
            &conf,
            &[],
        )
        .await
        .unwrap();
    if role == "learner-scatter" {
        rules
            .set_rule(
                Rule {
                    group_id: "pd".into(),
                    id: "default".into(),
                    role: PeerRoleType::Voter,
                    count: 2,
                    label_constraints: constraints.clone(),
                    location_labels: vec!["zone".into()],
                    ..Default::default()
                },
                0,
                &conf,
                &[],
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
                    label_constraints: constraints.clone(),
                    location_labels: vec!["zone".into()],
                    ..Default::default()
                },
                0,
                &conf,
                &[],
            )
            .await
            .unwrap();
    }
    let mut jobs = BalanceRangeJobs::new(endpoint, paths);
    let mut job =
        BalanceRangeJob::from_args(&[role, engine, "1s", "work", "", ""], JobTime(0)).unwrap();
    if variant == "outside" {
        job.ranges = vec![astersql_pd_keyutil::KeyRange::new(b"z", b"zz")];
    }
    if variant == "engine_mismatch" {
        job.engine = "tiflash".into();
    }
    if variant == "invalid" {
        job.engine = "invalid".into();
    }
    jobs.add(job).await.unwrap();
    if variant == "timeout_balanced" {
        jobs.begin(0, JobTime(0)).await.unwrap();
    }
    let ranges = astersql_pd_schedule_keyrange::Manager::new();
    if variant == "timeout_balanced" {
        ranges.append(&jobs.jobs()[0].ranges);
    }

    let influence = OpInfluence::new();
    if variant == "influence" {
        for id in 1..=3 {
            influence.update_store(id, |i| {
                i.region_count = -6;
                i.leader_count = -6;
            });
        }
    }
    let stats = StatisticsSnapshot::default();
    let mut random = |_| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut allocate = || {
        if variant == "allocation" {
            Err(BuilderError::Allocation("injected".into()))
        } else {
            Ok(1000)
        }
    };
    let mut ctx = RegionBalanceContext {
        cluster: &cluster,
        config: &conf,
        rules: &mut rules,
        affinity: AffinityFilter {
            manager: None,
            labels: &labels,
            now: 1_000_000_000_000,
        },
        statistics: &stats,
        hot_degree: 1,
        key_ranges: &ranges,
        influence: &influence,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut random,
        shuffle: &mut shuffle,
        allocate: &mut allocate,
        supports_conf_change_v2: v2,
        allow_empty_regions: true,
    };
    let mut scheduler = BalanceRangeScheduler::default();
    assert!(
        !scheduler
            .check_job(&mut jobs, &ctx, u64::MAX)
            .await
            .unwrap()
    );
    if matches!(
        variant,
        "outside" | "engine_mismatch" | "single" | "invalid"
    ) {
        assert!(scheduler.check_job(&mut jobs, &ctx, 0).await.is_err());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Running);
        assert!(!ranges.is_empty());
        assert!(scheduler.schedule(&mut ctx).await.operators.is_empty());
        return;
    }
    if variant == "timeout_balanced" {
        assert!(matches!(
            scheduler.check_job(&mut jobs, &ctx, 0).await,
            Err(BalanceRangeError::Job(RangeJobError::State("running")))
        ));
        assert_eq!(jobs.jobs()[0].status, JobStatus::Finished);
        assert!(ranges.is_empty());
        return;
    }
    assert_eq!(jobs.jobs()[0].status, JobStatus::Pending);
    backend
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(scheduler.check_job(&mut jobs, &ctx, 0).await.is_err());
    assert_eq!(jobs.jobs()[0].status, JobStatus::Pending);
    assert!(ranges.is_empty());
    backend
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    backend
        .stall
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(1),
            scheduler.check_job(&mut jobs, &ctx, 0)
        )
        .await
        .is_err()
    );
    assert_eq!(jobs.jobs()[0].status, JobStatus::Pending);
    assert!(ranges.is_empty());
    backend
        .stall
        .store(false, std::sync::atomic::Ordering::SeqCst);
    if variant == "balanced" {
        assert!(!scheduler.check_job(&mut jobs, &ctx, 0).await.unwrap());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Finished);
        assert!(ranges.is_empty());
        return;
    }

    assert!(
        scheduler
            .check_job(&mut jobs, &ctx, 0)
            .await
            .unwrap_or_else(|e| panic!("{role} {engine} {variant}: {e:?}"))
    );
    assert_eq!(jobs.jobs()[0].status, JobStatus::Running);
    assert!(!ranges.is_empty());
    let out = scheduler.schedule(&mut ctx).await;
    assert_eq!(
        out.operators.len(),
        expected_ops,
        "{role} {engine} {variant}: {:?}",
        out.errors
    );
    assert_eq!(out.errors.len(), expected_errors);
    if let Some(op) = out.operators.first() {
        assert!(op.kind().0 & OpKind::RANGE.0 != 0);
        assert_eq!(
            op.additional_info().get("targetScore").as_deref(),
            Some("0.00")
        );
        assert_eq!(
            op.additional_info().get("sourceScore").as_deref(),
            Some("6.00")
        );
        assert!(!op.is_empty());
        assert_eq!(
            op.priority_level(),
            astersql_pd_core_constant::PriorityLevel::MEDIUM
        );
        if variant == "existing" {
            let astersql_pd_schedule_operator::OpStep::TransferLeader(step) = op.step(0).unwrap()
            else {
                panic!("transfer expected")
            };
            assert_eq!(step.from_store, 1);
            assert!([2, 3].contains(&step.to_store));
            assert_eq!(op.len(), 1);
        } else {
            let astersql_pd_schedule_operator::OpStep::AddLearner(step) = op.step(0).unwrap()
            else {
                panic!("add learner expected")
            };
            assert_eq!(step.to_store, 4);
            assert_eq!(step.peer_id, 1000);
            let region = cluster.regions.get_region(op.region_id()).unwrap();
            let delta = OpInfluence::new();
            for i in 0..op.len() {
                op.step(i).unwrap().influence(&delta, &region);
            }
            assert_eq!(delta.get_store_influence(4).region_count, 1);
            assert_eq!(
                delta
                    .snapshot()
                    .values()
                    .map(|i| i.region_count)
                    .sum::<i64>(),
                0
            );
        }
    }

    assert!(!scheduler.report_at(JobTime(ctx.now_unix_nanos + 9_999_999_999)));
    assert!(scheduler.report_at(JobTime(ctx.now_unix_nanos + 10_000_000_000)));
    if variant == "timeout" {
        ctx.now_unix_nanos += 1_000_000_000;
        assert!(scheduler.check_job(&mut jobs, &ctx, 0).await.unwrap());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Running);
        ctx.now_unix_nanos += 1;
        backend
            .fail
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(scheduler.check_job(&mut jobs, &ctx, 0).await.is_err());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Running);
        assert!(!ranges.is_empty());
        backend
            .fail
            .store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(scheduler.check_job(&mut jobs, &ctx, 0).await.unwrap());
        assert_eq!(jobs.jobs()[0].status, JobStatus::Finished);
        assert!(ranges.is_empty());
        return;
    }
    jobs.delete(1, JobTime(ctx.now_unix_nanos)).await.unwrap();
    assert!(
        !scheduler
            .check_job(&mut jobs, &ctx, 0)
            .await
            .unwrap_or_else(|e| panic!("{role} {engine} {variant}: {e:?}"))
    );
    assert!(ranges.is_empty());
    assert!(scheduler.report.scores.values().all(|v| *v == (0.0, 0.0)));
}
#[tokio::test]
async fn task_105_roles_engines_and_faults() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_105.json"
    ))
    .unwrap();
    for c in f["schedule"].as_array().unwrap() {
        run_case(
            c["role"].as_str().unwrap(),
            c["engine"].as_str().unwrap(),
            c["variant"].as_str().unwrap(),
            c["v2"].as_bool().unwrap(),
            c["expected_operators"].as_u64().unwrap() as usize,
            c["expected_errors"].as_u64().unwrap() as usize,
        )
        .await;
    }
    println!(
        "task_105 role/engine/fault cases: {}",
        f["schedule"].as_array().unwrap().len()
    );
}

/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
    stall: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl astersql_pd_storage_kv::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        if self.stall.load(std::sync::atomic::Ordering::SeqCst) {
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
    ) -> astersql_pd_storage_kv::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.compact(r).await
    }
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
