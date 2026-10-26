// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_schedule_core::BasicCluster as _;
use astersql_pd_schedule_operator::{ControllerAdmission, OpStep};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
fn now() -> i64 {
    1_000_000_000_000
}
fn region(id: u64, witness: bool) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            is_witness: id == 1 && witness,
            ..Default::default()
        })
        .collect();
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: astersql_pd_core::KeyRange {
                start_key: vec![id as u8],
                end_key: vec![(id + 1) as u8],
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ))
}
fn cluster() -> Arc<BasicCluster> {
    let mut c = BasicCluster::new();
    for id in 1..=3 {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(now());
        c.stores.put_store(Arc::new(s));
    }
    for id in 1..=4 {
        c.regions.put_region(region(id, true));
    }
    Arc::new(c)
}
fn policy() -> WitnessLoopScheduler {
    WitnessLoopScheduler {
        policy: TransferWitnessLeaderScheduler::new(),
        config: Default::default(),
        rules: astersql_pd_schedule_placement::RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            astersql_pd_keypath::ClusterPaths::new(108),
        ),
        now_nanos: now,
        random_index: |_| 0,
        faults: vec![],
    }
}
fn controller() -> ScheduleController<WitnessLoopScheduler> {
    ScheduleController::new(policy(), cluster(), LoopControl::default(), now)
}
#[tokio::test]
async fn task_108_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_108.json"
    ))
    .unwrap();
    for row in f["gates"].as_array().unwrap() {
        let control = LoopControl::default();
        control.halt(row[1].as_bool().unwrap());
        control.set_delay_until(row[2].as_i64().unwrap());
        assert_eq!(
            format!("{:?}", control.gate(row[0].as_bool().unwrap(), 100)),
            row[3].as_str().unwrap()
        );
    }
    let mut c = controller();
    assert!(c.schedule(true).await.unwrap().is_empty());
    assert_eq!(c.interval(), f["empty_interval"].as_i64().unwrap());
    let sender = c.scheduler.policy.recv_region_info();
    assert!(!send_witness_event(&sender, region(1, false)));
    for id in 1..=4 {
        assert!(send_witness_event(&sender, region(id, true)));
    }
    let ops = c.schedule(false).await.unwrap();
    assert_eq!(ops.len(), f["witness_batch"].as_u64().unwrap() as usize);
    assert_eq!(c.interval(), f["success_interval"].as_i64().unwrap());
    for op in &ops {
        assert!(
            matches!(op.step(0),Some(OpStep::TransferLeader(s)) if s.from_store==1 && s.to_store!=1)
        );
    }
    assert_eq!(c.schedule(false).await.unwrap().len(), 1);
    c.set_interval(MAX_SCHEDULE_INTERVAL);
    assert!(c.schedule(false).await.unwrap().is_empty());
    assert_eq!(c.interval(), MAX_SCHEDULE_INTERVAL);
    c.control.stop();
    c.set_interval(123);
    assert!(c.schedule(false).await.unwrap().is_empty());
    assert_eq!(c.interval(), 123);
    println!(
        "task_108: 4 gate rows, empty/backoff/cap, 4 witness events in 3+1 batches, cancellation"
    );
}
#[tokio::test]
async fn task_108_cache_and_faults() {
    let c = cluster();
    let cache = CacheCluster::new(&c);
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 4,
        ..Default::default()
    })));
    assert_eq!(cache.get_stores().len(), 3);
    assert_eq!(c.get_stores().len(), 4);
    assert!(cache.get_store(4).is_some());
    assert_eq!(CacheCluster::new(&c).get_stores().len(), 4);
    let mut c = controller();
    let sender = c.scheduler.policy.recv_region_info();
    // Missing region must discard the entire result and retry, not emit a stale operator.
    sender.send(region(99, true)).unwrap();
    assert!(c.schedule(false).await.unwrap().is_empty());
    assert_eq!(c.interval(), 13_000_000);
    // Malformed event is consumed, and a healthy event in the same batch survives.
    sender
        .send(Arc::new(RegionInfo::new(
            region(2, true).meta().clone(),
            None,
            vec![],
            vec![],
        )))
        .unwrap();
    sender.send(region(1, true)).unwrap();
    let ops = c.schedule(false).await.unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].region_id(), 1);
    assert_eq!(c.scheduler.faults.len(), 1);
    let disconnected = policy().policy.recv_region_info();
    assert!(!send_witness_event(&disconnected, region(1, true)));
    let mut bad = controller();
    for _ in 0..10_000 {
        bad.scheduler
            .policy
            .recv_region_info()
            .try_send(region(1, true))
            .unwrap();
    }
    assert!(!send_witness_event(
        &bad.scheduler.policy.recv_region_info(),
        region(1, true)
    ));
    bad.set_interval(0);
    let out = bad.run(Arc::new(ControllerAdmission::new(10))).await;
    assert!(out.invalid_interval);
    assert!(out.cleanup.is_ok());
}
#[tokio::test(start_paused = true)]
async fn task_108_timer_stop_wait() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let c = controller();
            let sender = c.scheduler.policy.recv_region_info();
            sender.send(region(1, true)).unwrap();
            let admission = Arc::new(ControllerAdmission::new(10));
            let task = c.start_local(admission.clone());
            tokio::task::yield_now().await;
            assert!(
                admission
                    .promote_waiting(&cluster(), 0.0)
                    .unwrap()
                    .is_none()
            );
            tokio::time::advance(std::time::Duration::from_millis(9)).await;
            tokio::task::yield_now().await;
            assert!(
                admission
                    .promote_waiting(&cluster(), 0.0)
                    .unwrap()
                    .is_none()
            );
            tokio::time::advance(std::time::Duration::from_millis(1)).await;
            tokio::task::yield_now().await;
            assert_eq!(
                admission.operator_count(astersql_pd_schedule_operator::OpKind::LEADER),
                1
            );
            assert_eq!(admission.get_operator(1).unwrap().region_id(), 1);
            task.stop();
            task.stop();
            let out = task.wait().await.unwrap();
            assert!(out.cleanup.is_ok());
            assert!(out.errors.is_empty());
            assert!(out.admission_errors.is_empty());
            // Admission rejection is a real controller state transition, retained at exit.
            let c = controller();
            c.scheduler
                .policy
                .recv_region_info()
                .send(region(1, true))
                .unwrap();
            let t = c.start_local(Arc::new(ControllerAdmission::new(0)));
            tokio::task::yield_now().await;
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
            t.stop();
            let out = t.wait().await.unwrap();
            assert_eq!(out.admission_errors.len(), 1);
            assert!(out.cleanup.is_ok());
        })
        .await;
}

async fn labeler() -> Arc<astersql_pd_schedule_labeler::RegionLabeler> {
    use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
    let mut l = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        astersql_pd_keypath::ClusterPaths::new(108),
        now().into(),
    )
    .await
    .unwrap();
    let rule=LabelRule::from_json(br#"{"id":"deny","labels":[{"key":"schedule","value":"deny"}],"rule_type":"key-range","data":[{"start_key":"","end_key":""}]}"#,now().into()).unwrap();
    l.set_label_rule(rule, now().into()).await.unwrap();
    Arc::new(l)
}
fn eviction(c: &BasicCluster) -> EvictLoopScheduler {
    let w = policy();
    let policy = EvictLeaderScheduler::new(
        EvictLeaderConfig::new([(1, vec![astersql_pd_core::KeyRange::default()])].into(), 3)
            .unwrap(),
    );
    policy.prepare(c).unwrap();
    let mut config = w.config;
    config.schedule.leader_schedule_limit = 4;
    EvictLoopScheduler {
        policy,
        config,
        rules: w.rules,
        now_nanos: now,
        random_index: |_| 0,
        admission: Arc::new(ControllerAdmission::new(10)),
        faults: vec![],
    }
}
#[tokio::test]
async fn task_108_label_and_retry_budget() {
    let mut c = controller();
    c.labeler = Some(labeler().await);
    c.scheduler
        .policy
        .recv_region_info()
        .send(region(1, true))
        .unwrap();
    assert!(c.schedule(false).await.unwrap().is_empty());
    assert_eq!(c.interval(), 13_000_000);
    // Thirty missing-region events fill ten three-event attempts. Remaining event
    // must stay queued and succeed in the next invocation.
    let mut c = controller();
    let sender = c.scheduler.policy.recv_region_info();
    for _ in 0..30 {
        sender.send(region(99, true)).unwrap();
    }
    sender.send(region(1, true)).unwrap();
    assert!(c.schedule(true).await.unwrap().is_empty());
    assert_eq!(c.schedule(false).await.unwrap().len(), 1);
    let mut core = BasicCluster::new();
    for s in cluster().stores.get_stores() {
        core.stores.put_store(s);
    }
    core.regions.put_region(region(1, false));
    let core = Arc::new(core);
    let p = eviction(&core);
    assert!(!core.get_store(1).unwrap().allow_leader_transfer_in());
    let mut c = ScheduleController::new(p, core.clone(), LoopControl::default(), now);
    c.labeler = Some(labeler().await);
    assert!(
        !c.schedule(false).await.unwrap().is_empty(),
        "eviction bypasses deny label"
    );
    c.control.stop();
    let out = c.run(Arc::new(ControllerAdmission::new(10))).await;
    assert!(out.cleanup.is_ok());
    assert!(core.get_store(1).unwrap().allow_leader_transfer_in());
}
/// Delay only the external I/O boundary. The actual policy still creates real
/// operators and releases real store pause references.
struct DelayedEviction {
    panic_io: bool,
    policy: EvictLoopScheduler,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl LoopScheduler for DelayedEviction {
    type Error = astersql_pd_core::StoreControlError;
    type Config = astersql_pd_schedule_config::ProviderSnapshot<u64>;
    fn admission_config(&self) -> &Self::Config {
        self.policy.admission_config()
    }
    fn waiting_random(&mut self) -> f64 {
        self.policy.waiting_random()
    }

    fn allowed(&self) -> bool {
        self.policy.allowed()
    }
    fn evict_leader(&self) -> bool {
        true
    }
    async fn schedule(
        &mut self,
        c: &CacheCluster<'_>,
        d: bool,
    ) -> Result<Vec<Arc<astersql_pd_schedule_operator::Operator>>, Self::Error> {
        self.entered.notify_one();
        self.release.notified().await;
        assert!(!self.panic_io, "external I/O panic");
        self.policy.schedule(c, d).await
    }
    async fn clean(&mut self, c: &BasicCluster) -> Result<(), Self::Error> {
        self.policy.clean(c).await
    }
}
#[tokio::test(start_paused = true)]
async fn task_108_inflight_cleanup_wait() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let c = cluster();
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let s = DelayedEviction {
                panic_io: false,
                policy: eviction(&c),
                entered: entered.clone(),
                release: release.clone(),
            };
            let task = ScheduleController::new(s, c.clone(), LoopControl::default(), now)
                .start_local(Arc::new(ControllerAdmission::new(10)));
            tokio::task::yield_now().await;
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            entered.notified().await;
            task.stop();
            let waiter = tokio::task::spawn_local(task.wait());
            tokio::task::yield_now().await;
            assert!(!waiter.is_finished());
            assert!(!c.get_store(1).unwrap().allow_leader_transfer_in());
            release.notify_one();
            let out = waiter.await.unwrap().unwrap();
            assert!(out.cleanup.is_ok());
            assert!(c.get_store(1).unwrap().allow_leader_transfer_in());
            // Parent cancellation is observed before any scheduling, and still cleans.
            let parent = tokio_util::sync::CancellationToken::new();
            let control = LoopControl::from_parent(&parent);
            let s = eviction(&c);
            let task = ScheduleController::new(s, c.clone(), control, now)
                .start_local(Arc::new(ControllerAdmission::new(10)));
            parent.cancel();
            let out = task.wait().await.unwrap();
            assert!(out.cleanup.is_ok());
            assert!(c.get_store(1).unwrap().allow_leader_transfer_in());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn task_108_timer_backoff_and_gates() {
    tokio::task::LocalSet::new()
        .run_until(async {
            use astersql_pd_schedule_operator::OpKind;
            let c = controller();
            let sender = c.scheduler.policy.recv_region_info();
            let admission = Arc::new(ControllerAdmission::new(10));
            let t = c.start_local(admission.clone());
            tokio::task::yield_now().await;
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
            sender.send(region(1, true)).unwrap();
            tokio::time::advance(std::time::Duration::from_millis(12)).await;
            tokio::task::yield_now().await;
            assert_eq!(admission.operator_count(OpKind::LEADER), 0);
            tokio::time::advance(std::time::Duration::from_millis(1)).await;
            tokio::task::yield_now().await;
            assert_eq!(admission.operator_count(OpKind::LEADER), 1);
            t.control.halt(true);
            sender.send(region(2, true)).unwrap();
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
            assert_eq!(admission.operator_count(OpKind::LEADER), 1);
            t.control.halt(false);
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
            assert_eq!(admission.operator_count(OpKind::LEADER), 2);
            t.control.set_delay_until(1001);
            sender.send(region(3, true)).unwrap();
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
            assert_eq!(admission.operator_count(OpKind::LEADER), 2);
            t.control.set_delay_until(1000);
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
            assert_eq!(admission.operator_count(OpKind::LEADER), 3);
            t.stop();
            let out = t.wait().await.unwrap();
            assert_eq!(out.controller.interval(), 10_000_000);
            assert!(out.cleanup.is_ok());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn task_108_panic_cleans_before_join() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let c = cluster();
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let s = DelayedEviction {
                panic_io: true,
                policy: eviction(&c),
                entered: entered.clone(),
                release: release.clone(),
            };
            let task = ScheduleController::new(s, c.clone(), LoopControl::default(), now)
                .start_local(Arc::new(ControllerAdmission::new(10)));
            tokio::task::yield_now().await;
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            entered.notified().await;
            release.notify_one();
            let result = task.wait().await;
            assert!(result.is_err());
            assert!(c.get_store(1).unwrap().allow_leader_transfer_in());
        })
        .await;
}
