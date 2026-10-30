// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn task_056_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_056.json"
    ))
    .unwrap();
    for case in fixture["scan_limits"].as_array().unwrap() {
        assert_eq!(
            patrol_scan_limit(case[0].as_u64().unwrap() as usize),
            case[1].as_u64().unwrap() as usize
        );
    }
    println!("task_056: 7 frozen scan limit cases");
}

use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{BasicCluster, KeyRange, Peer, PeerRole, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_affinity::manager::Manager;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_operator::{ControllerAdmission, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use async_trait::async_trait;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio_util::sync::CancellationToken;

struct Domain {
    cluster: BasicCluster,
    dispatcher: CheckerDispatcher,
    rules: RuleManager,
    affinity: Manager,
    config: ProviderSnapshot<()>,
    labels: RegionLabeler,
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    operators: ControllerAdmission,
}
pub(super) struct Host {
    pending: Arc<TTLUint64<()>>,
    domain: Mutex<Domain>,
    config: Cell<PatrolConfig>,
    halt: Cell<bool>,
    total: Cell<usize>,
    events: RefCell<Vec<String>>,
    scans: RefCell<Vec<(Vec<u8>, usize, Vec<u64>)>>,
    block: Cell<bool>,
    active: Cell<usize>,
    entered: Notify,
    release: Semaphore,
    cleared: Cell<bool>,
    fail: Cell<bool>,
    split: RefCell<SplitScatterRetryController>,
    label_stats: RefCell<astersql_pd_statistics::LabelStatistics>,
}
fn region(id: u64, terminal: bool) -> Arc<RegionInfo> {
    let peers = vec![
        Peer {
            id: id * 10 + 1,
            store_id: 1,
            ..Default::default()
        },
        Peer {
            id: id * 10 + 2,
            store_id: 2,
            role: PeerRole::Learner,
            ..Default::default()
        },
    ];
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: (id as u32).to_be_bytes().to_vec(),
                end_key: if terminal {
                    vec![]
                } else {
                    ((id + 1) as u32).to_be_bytes().to_vec()
                },
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ))
}
impl Host {
    pub(super) async fn suspect_tick(&self, ranges: &SuspectRanges) -> Result<(), Fault> {
        let d = self.domain.lock().await;
        ranges.check_once(&d.cluster, &d.dispatcher);
        Ok(())
    }
    pub(super) async fn learner_waiting(&self) -> u64 {
        self.domain
            .lock()
            .await
            .operators
            .waiting_count("promote-learner")
    }

    pub(super) async fn new(count: usize, workers: usize, terminal: bool) -> Self {
        let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(56);
        let labels = RegionLabeler::load(
            StorageEndpoint::new(Arc::new(MemoryStore::default())),
            paths.clone(),
            0,
        )
        .await
        .unwrap();
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.enable_placement_rules = false;
        config.replication.max_replicas = 3;
        config.schedule.replica_schedule_limit = 10;
        config.schedule.merge_schedule_limit = 0;
        config.schedule.affinity_schedule_limit = 0;
        config.schedule.patrol_region_interval = "10ms".parse().unwrap();
        let mut rules = RuleManager::new(storage.clone(), paths.clone());
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let mut cluster = BasicCluster::new();
        for id in 1..=count {
            cluster
                .regions
                .set_region(region(id as u64, terminal && id == count));
        }
        let pending = Arc::new(TTLUint64::new(
            tokio::runtime::Handle::current(),
            CancellationToken::new(),
            Duration::from_secs(60),
            TtlDuration::from_nanos(180_000_000_000),
        ));
        Self {
            pending: pending.clone(),
            domain: Mutex::new(Domain {
                cluster,
                dispatcher: CheckerDispatcher::new(pending, 0, 0),
                rules,
                affinity: Manager::default(),
                config,
                labels,
                storage,
                paths,
                operators: ControllerAdmission::new(100_000),
            }),
            config: Cell::new(PatrolConfig {
                interval: Duration::from_millis(10),
                workers,
            }),
            halt: Cell::new(false),
            total: Cell::new(count),
            events: RefCell::new(vec![]),
            scans: RefCell::new(vec![]),
            block: Cell::new(false),
            active: Cell::new(0),
            entered: Notify::new(),
            release: Semaphore::new(0),
            cleared: Cell::new(false),
            fail: Cell::new(false),
            split: RefCell::new({
                let mut split = SplitScatterRetryController::default();
                split.record_batch(9999, 1, &[10000], Duration::ZERO, true);
                split
            }),
            label_stats: RefCell::new(astersql_pd_statistics::LabelStatistics::new()),
        }
    }
    pub(super) async fn shutdown(&self) {
        self.pending.shutdown().await;
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Injected,
}
#[async_trait(?Send)]
impl PatrolHost for Host {
    type Error = Fault;
    async fn config(&self) -> PatrolConfig {
        self.config.get()
    }
    async fn region_count(&self) -> usize {
        self.total.get()
    }
    async fn scheduling_halted(&self) -> bool {
        self.halt.get()
    }
    async fn priority_ids(&self) -> Vec<u64> {
        let d = self.domain.lock().await;
        d.dispatcher
            .priority
            .get_priority_regions(&d.config, 100_000_000_000)
    }
    async fn pending_ids(&self) -> Vec<u64> {
        self.domain
            .lock()
            .await
            .dispatcher
            .pending_processed_regions()
    }
    async fn region(&self, id: u64) -> Option<Arc<RegionInfo>> {
        self.domain.lock().await.cluster.regions.get_region(id)
    }
    async fn remove_priority(&self, id: u64) {
        self.domain
            .lock()
            .await
            .dispatcher
            .priority
            .remove_priority_region(id);
        self.events.borrow_mut().push(format!("remove:{id}"));
    }
    async fn check(&self, r: Arc<RegionInfo>, source: PatrolSource) -> Result<(), Fault> {
        self.events
            .borrow_mut()
            .push(format!("{source:?}:{}", r.meta().id));
        if source == PatrolSource::Scan && self.block.get() {
            self.active.set(self.active.get() + 1);
            self.entered.notify_one();
            self.release.acquire().await.unwrap().forget();
            self.active.set(self.active.get() - 1);
        }
        if self.fail.get() && source == PatrolSource::Scan {
            return Err(Fault::Injected);
        }
        let mut d = self.domain.lock().await;
        let Domain {
            cluster,
            dispatcher,
            rules,
            affinity,
            config,
            labels,
            storage,
            paths,
            operators,
        } = &mut *d;
        let hot = BTreeSet::new();
        let ctx = CheckerDispatchContext {
            affinity: AffinityContext {
                rules: RuleMissingContext {
                    replica: ReplicaFaultContext {
                        config,
                        stores: &cluster.stores,
                        rules: &[],
                        now_unix_nanos: 100_000_000_000,
                        supports_conf_change_v2: false,
                    },
                    witness_enabled: false,
                },
                labels,
                storage,
                paths,
                region_exists: true,
            },
            cluster,
            operators,
            hot_regions: &hot,
        };
        let out = dispatcher
            .check_patrol_region(&ctx, rules, affinity, &r, source, &mut || Ok(1000))
            .await;
        assert!(out.errors.is_empty());
        for op in out.queued {
            assert_eq!(op.desc(), "promote-learner");
            assert!(matches!(op.step(0), Some(OpStep::PromoteLearner(_))));
        }
        Ok(())
    }
    async fn dispatch_split_scatter(&self) {
        self.events.borrow_mut().push("split-scatter".into());
        let mut d = self.domain.lock().await;
        let Domain {
            cluster,
            config,
            rules,
            operators,
            labels,
            ..
        } = &mut *d;
        let stores = cluster.stores.get_stores();
        let replication = astersql_pd_schedule_filter::RegionReplicationContext {
            conf: config,
            stores: &stores,
            manager: RefCell::new(rules),
            now: 100_000_000_000,
        };
        let errors = self
            .split
            .borrow_mut()
            .dispatch(
                cluster,
                operators,
                &replication,
                Some(labels),
                Duration::ZERO,
                1,
                |_, _, _, _| panic!("absent split results cannot scatter"),
            )
            .await;
        assert!(errors.is_empty());
    }
    async fn clear_split_scatter(&self) {
        assert_eq!(self.active.get(), 0);
        self.split.borrow_mut().clear();
        assert_eq!(self.split.borrow().metrics().pending, 0);
        self.cleared.set(true);
    }
    async fn scan(&self, start: &[u8], limit: usize) -> Vec<Arc<RegionInfo>> {
        let regions =
            self.domain
                .lock()
                .await
                .cluster
                .regions
                .scan_regions(start, &[], limit as isize);
        self.scans.borrow_mut().push((
            start.to_vec(),
            limit,
            regions.iter().map(|r| r.meta().id).collect(),
        ));
        self.events.borrow_mut().push("scan".into());
        regions
    }
    async fn update_label_stats(&self, regions: &[Arc<RegionInfo>]) {
        for region in regions {
            self.label_stats.borrow_mut().observe(region, &[], &[]);
        }
        self.events
            .borrow_mut()
            .push(format!("labels:{}", regions.len()));
    }
}
async fn ticks(status: &mut tokio::sync::watch::Receiver<PatrolStatus>, n: u64) {
    while status.borrow().ticks < n {
        status.changed().await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn task_056_real_priority_pending_and_scan_order() {
    let h = Host::new(2, 0, true).await;
    {
        let mut d = h.domain.lock().await;
        let Domain {
            cluster,
            dispatcher,
            rules,
            config,
            ..
        } = &mut *d;
        dispatcher.priority.inspect(
            config,
            rules,
            &[],
            &cluster.regions.get_region(1).unwrap(),
            0,
        );
        let mut missing = region(99, false).meta().clone();
        missing.peers.clear();
        dispatcher.priority.inspect(
            config,
            rules,
            &[],
            &RegionInfo::new(missing, None, vec![], vec![]),
            0,
        );
        dispatcher.add_pending_processed_regions(false, &[2, 999]);
    }
    let mut p = Patrol::new();
    let mut status = p.subscribe();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(p.run(&h, cancel.clone()), async {
        ticks(&mut status, 1).await;
        let e = h.events.borrow();
        assert!(e.len() >= 6, "priority/pending/scan phases missing: {e:?}");
        assert_eq!(
            &e[..6],
            &[
                "remove:99",
                "Priority:1",
                "Pending:2",
                "split-scatter",
                "scan",
                "labels:2"
            ]
        );
        drop(e);
        let d = h.domain.lock().await;
        assert_eq!(d.operators.waiting_count("promote-learner"), 2);
        assert_eq!(d.dispatcher.pending_processed_regions(), vec![999]);
        assert!(d.dispatcher.priority.entry(99).is_none());
        assert_eq!(status.borrow().rounds, 1);
        assert_eq!(h.label_stats.borrow().label_counter()["none"], 2);
        drop(d);
        cancel.cancel();
    });
    assert_eq!(result, Ok(()));
    assert!(h.cleared.get());
    h.shutdown().await;
    println!(
        "task_056: real priority removal, pending recheck/absent retention, queue promotion steps, phase order"
    );
}

#[tokio::test(start_paused = true)]
async fn task_056_cursor_backlog_dynamic_tick_and_workers() {
    let h = Host::new(130, 0, false).await;
    let mut p = Patrol::new();
    let mut status = p.subscribe();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(p.run(&h, cancel.clone()), async {
        ticks(&mut status, 1).await;
        assert_eq!(h.scans.borrow()[0].2.len(), 128);
        assert_eq!(status.borrow().cursor, 129u32.to_be_bytes());
        ticks(&mut status, 2).await;
        assert_eq!(h.scans.borrow().len(), 1); // backlog prevents all phases and cursor advance
        h.config.set(PatrolConfig {
            interval: Duration::from_millis(25),
            workers: 2,
        });
        ticks(&mut status, 3).await;
        assert_eq!(status.borrow().interval, Duration::from_millis(25));
        assert_eq!(status.borrow().workers, 2);
        let changed = tokio::time::Instant::now();
        ticks(&mut status, 4).await;
        assert!(changed.elapsed() >= Duration::from_millis(25));
        // Drain and scan the trailing two regions, then empty scan resets the cursor.
        while h.scans.borrow().len() < 3 {
            let n = status.borrow().ticks + 1;
            ticks(&mut status, n).await;
        }
        let scans = h.scans.borrow();
        assert_eq!(scans[1].0, 129u32.to_be_bytes());
        assert_eq!(scans[1].2, vec![129, 130]);
        assert_eq!(scans[2].0, 131u32.to_be_bytes());
        assert!(scans[2].2.is_empty());
        drop(scans);
        assert!(status.borrow().cursor.is_empty());
        assert_eq!(status.borrow().rounds, 0);
        cancel.cancel();
    });
    assert_eq!(result, Ok(()));
    h.shutdown().await;
    println!(
        "task_056: cursor across 128+2+empty, backlog skip, workers 0->2, interval 10->25ms and reset deadline"
    );
}

#[tokio::test(start_paused = true)]
async fn task_056_shutdown_and_resize_wait_for_inflight() {
    for resize in [false, true] {
        let h = Host::new(4, 2, true).await;
        h.block.set(true);
        let mut p = Patrol::new();
        let mut status = p.subscribe();
        let cancel = CancellationToken::new();
        let (result, ()) = tokio::join!(p.run(&h, cancel.clone()), async {
            while h.active.get() < 2 {
                h.entered.notified().await;
            }
            assert_eq!(h.active.get(), 2);
            if resize {
                h.config.set(PatrolConfig {
                    workers: 1,
                    ..h.config.get()
                });
                tokio::time::advance(Duration::from_millis(10)).await;
            } else {
                cancel.cancel();
            }
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            assert!(!h.cleared.get());
            assert_eq!(h.active.get(), 2);
            h.block.set(false);
            h.release.add_permits(2);
            if resize {
                while status.borrow().workers != 1 {
                    status.changed().await.unwrap();
                }
                assert_eq!(h.active.get(), 0);
                cancel.cancel();
            }
        });
        assert_eq!(result, Ok(()));
        assert!(h.cleared.get());
        assert_eq!(status.borrow().workers, 0);
        assert_eq!(status.borrow().last_round, Duration::ZERO);
        h.shutdown().await;
    }
    println!(
        "task_056: shutdown and 2->1 resize join both blocked in-flight real checks before cleanup/restart"
    );
}

#[tokio::test(start_paused = true)]
async fn task_056_saturation_halt_and_cancel() {
    let h = Host::new(8192, 0, true).await;
    // Aggregate cluster count is a host statistics input; the local registry holds one scan batch.
    h.total.set(8_388_608);
    let mut p = Patrol::new();
    let mut status = p.subscribe();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(p.run(&h, cancel.clone()), async {
        ticks(&mut status, 1).await;
        assert_eq!(h.scans.borrow()[0].2.len(), PATROL_CHANNEL_CAPACITY);
        ticks(&mut status, 2).await;
        assert_eq!(h.scans.borrow().len(), 1);
        h.halt.set(true);
        ticks(&mut status, 3).await;
        assert_eq!(h.scans.borrow().len(), 1);
        h.halt.set(false);
        ticks(&mut status, 4).await;
        assert_eq!(h.scans.borrow().len(), 2);
        cancel.cancel();
    });
    assert_eq!(result, Ok(()));
    assert!(h.cleared.get());
    h.shutdown().await;
    println!(
        "task_056: full 8192 channel with zero workers, backlog skip, halt drains, resume scans, cancel full channel"
    );
}

#[tokio::test(start_paused = true)]
async fn task_056_worker_fault_and_invalid_config() {
    let h = Host::new(2, 2, true).await;
    h.fail.set(true);
    let mut p = Patrol::new();
    assert_eq!(
        p.run(&h, CancellationToken::new()).await,
        Err(PatrolError::Host(Fault::Injected))
    );
    assert!(h.cleared.get());
    let h2 = Host::new(0, 0, true).await;
    h2.config.set(PatrolConfig {
        interval: Duration::ZERO,
        workers: 0,
    });
    assert_eq!(
        p.run(&h2, CancellationToken::new()).await,
        Err(PatrolError::InvalidInterval)
    );
    let h3 = Host::new(0, 1, true).await;
    let mut status = p.subscribe();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(p.run(&h3, cancel), async {
        ticks(&mut status, 1).await;
        h3.config.set(PatrolConfig {
            interval: Duration::ZERO,
            workers: 1,
        });
    });
    assert_eq!(result, Err(PatrolError::InvalidInterval));
    assert!(h3.cleared.get());
    h.shutdown().await;
    h2.shutdown().await;
    h3.shutdown().await;
    println!(
        "task_056: typed worker fault joins cleanup; initial and dynamic zero interval errors"
    );
}

#[tokio::test(start_paused = true)]
async fn task_056_round_boundary_updates_limit_and_duration() {
    let h = Host::new(130, 0, true).await;
    let mut p = Patrol::new();
    let mut status = p.subscribe();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(p.run(&h, cancel.clone()), async {
        ticks(&mut status, 1).await;
        h.total.set(132096);
        h.halt.set(true);
        ticks(&mut status, 2).await;
        assert_eq!(status.borrow().scan_limit, 128); // only a nonempty completed scan updates it
        h.halt.set(false);
        ticks(&mut status, 3).await;
        assert_eq!(h.scans.borrow()[1].2, vec![129, 130]);
        assert_eq!(status.borrow().scan_limit, 129);
        assert_eq!(status.borrow().rounds, 1);
        assert_eq!(status.borrow().last_round, Duration::from_millis(30));
        cancel.cancel();
    });
    assert_eq!(result, Ok(()));
    assert_eq!(status.borrow().last_round, Duration::ZERO);
    h.shutdown().await;
    println!(
        "task_056: count snapshot changes 130->132096; limit refresh only at terminal region; exact 30ms duration and cancel reset"
    );
}

#[tokio::test]
async fn task_056_priority_merge_and_pending_admission_contracts() {
    use astersql_pd_core::{StoreInfo, StoreMeta, StoreStats};
    let h = Host::new(2, 0, true).await;
    let mut d = h.domain.lock().await;
    for id in 1..=3 {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        store.set_last_heartbeat_unix_nanos(100_000_000_000);
        store.update_raw_stats(StoreStats {
            capacity: 1_000_000_000_000,
            available: 900_000_000_000,
            used_size: 100_000_000_000,
            ..Default::default()
        });
        d.cluster.stores.put_store(Arc::new(store));
    }
    for id in 1..=2 {
        let mut meta = region(id, id == 2).meta().clone();
        meta.peers = (1..=3)
            .map(|s| Peer {
                id: id * 10 + s,
                store_id: s,
                ..Default::default()
            })
            .collect();
        let leader = Some(meta.peers[0].clone());
        let mut r = RegionInfo::new(meta, leader, vec![], vec![]);
        r.stats_mut().approximate_size = 1;
        r.stats_mut().approximate_keys = 1;
        d.cluster.regions.set_region(Arc::new(r));
    }
    d.config.schedule.merge_schedule_limit = 10;
    d.config.schedule.max_merge_region_size = 10;
    d.config.schedule.max_merge_region_keys = 10000;
    d.dispatcher.add_pending_processed_regions(false, &[1]);
    let Domain {
        cluster,
        dispatcher,
        rules,
        affinity,
        config,
        labels,
        storage,
        paths,
        operators,
    } = &mut *d;
    let hot = BTreeSet::new();
    let ctx = CheckerDispatchContext {
        affinity: AffinityContext {
            rules: RuleMissingContext {
                replica: ReplicaFaultContext {
                    config,
                    stores: &cluster.stores,
                    rules: &[],
                    now_unix_nanos: 100_000_000_000,
                    supports_conf_change_v2: false,
                },
                witness_enabled: false,
            },
            labels,
            storage,
            paths,
            region_exists: true,
        },
        cluster,
        operators,
        hot_regions: &hot,
    };
    let r = cluster.regions.get_region(1).unwrap();
    let planned = dispatcher
        .check_region(&ctx, rules, affinity, &r, &mut || Ok(1000))
        .await;
    assert_eq!(
        planned.operators.len(),
        2,
        "precondition: genuine merge pair"
    );
    assert!(
        planned.operators[0]
            .additional_info()
            .has_related_merge_region()
    );
    let priority = dispatcher
        .check_patrol_region(
            &ctx,
            rules,
            affinity,
            &r,
            PatrolSource::Priority,
            &mut || Ok(1000),
        )
        .await;
    assert!(priority.queued.is_empty());
    assert_eq!(operators.waiting_count("merge-region"), 0);
    assert_eq!(dispatcher.pending_processed_regions(), vec![1]);
    let scan = dispatcher
        .check_patrol_region(&ctx, rules, affinity, &r, PatrolSource::Scan, &mut || {
            Ok(1000)
        })
        .await;
    assert_eq!(scan.queued.len(), 2);
    assert!(dispatcher.pending_processed_regions().is_empty());
    drop(d);
    h.shutdown().await;
    // Priority enqueue rejection preserves pending; ordinary pending recheck removes it.
    let h = Host::new(1, 0, true).await;
    {
        let mut d = h.domain.lock().await;
        d.operators = ControllerAdmission::new(0);
        d.dispatcher.add_pending_processed_regions(false, &[1]);
    }
    h.check(region(1, true), PatrolSource::Priority)
        .await
        .unwrap();
    assert_eq!(
        h.domain.lock().await.dispatcher.pending_processed_regions(),
        vec![1]
    );
    h.check(region(1, true), PatrolSource::Pending)
        .await
        .unwrap();
    assert!(
        h.domain
            .lock()
            .await
            .dispatcher
            .pending_processed_regions()
            .is_empty()
    );
    h.shutdown().await;
    println!(
        "task_056: real merge pair excluded from priority/admitted from scan; priority preserves pending on admission rejection, ordinary recheck removes it"
    );
}

impl Host {
    pub(super) async fn install_checker_metrics(&self, metrics: CheckerMetrics, pause: bool) {
        let mut domain = self.domain.lock().await;
        domain.dispatcher.set_metrics(metrics.clone());
        if pause {
            for name in [
                "joint-state",
                "split",
                "learner",
                "replica",
                "affinity",
                "merge",
            ] {
                domain
                    .dispatcher
                    .pause_controller(name)
                    .unwrap()
                    .pause_or_resume_at(1000, 0);
            }
        }
        self.split.borrow_mut().set_metrics(metrics);
    }
    pub(super) fn halt_for_metrics(&self, halted: bool) {
        self.halt.set(halted);
    }
    pub(super) fn fail_for_metrics(&self) {
        self.fail.set(true);
    }
}

impl Host {
    pub(super) async fn seed_metrics_queues(&self) {
        let mut d = self.domain.lock().await;
        let Domain {
            dispatcher,
            rules,
            cluster,
            config,
            ..
        } = &mut *d;
        for id in [1, 99] {
            // Distinct deficits avoid the canonical queue's priority-only tie equivalence.
            config.replication.max_replicas = if id == 99 { 4 } else { 3 };
            dispatcher.priority.inspect(
                config,
                rules,
                &cluster.stores.get_stores(),
                &region(id, true),
                0,
            );
        }
        config.replication.max_replicas = 3;
        dispatcher.add_pending_processed_regions(false, &[1, 99]);
    }
}

#[tokio::test(start_paused = true)]
async fn task_054_progress_reads_live_patrol_duration() {
    let h = Host::new(130, 0, true).await;
    let mut patrol = Patrol::new();
    h.domain
        .lock()
        .await
        .dispatcher
        .set_patrol_status(patrol.subscribe());
    assert_eq!(
        h.domain.lock().await.dispatcher.patrol_regions_duration(),
        Duration::ZERO
    );
    let mut status = patrol.subscribe();
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(patrol.run(&h, cancel.clone()), async {
        ticks(&mut status, 1).await;
        h.halt.set(true);
        ticks(&mut status, 2).await;
        h.halt.set(false);
        ticks(&mut status, 3).await;
        assert_eq!(
            h.domain.lock().await.dispatcher.patrol_regions_duration(),
            Duration::from_millis(30)
        );
        cancel.cancel();
    });
    assert_eq!(result, Ok(()));
    assert_eq!(
        h.domain.lock().await.dispatcher.patrol_regions_duration(),
        Duration::ZERO
    );
    h.shutdown().await;
}
