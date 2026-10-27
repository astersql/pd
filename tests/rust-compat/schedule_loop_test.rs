// Copyright 2026 AsterSQL.
//! Host composition only. TiKV command application is an external boundary, not a scheduler.
use astersql_pd_cluster::{HeartbeatEffectsState, HeartbeatRunners, StatePipeline};
use astersql_pd_core::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{pdpb, schedulingpb};
use astersql_pd_ratelimit::ConcurrentRunner;
use astersql_pd_schedule_checker::{JointStateChecker, LearnerChecker};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{
    HeartbeatCommand, SplitPolicy,
    routing::{RouteEvent, StreamRouter},
};
use astersql_pd_schedule_operator::*;
use astersql_pd_schedule_placement::{Rule, RuleManager, fit_region};
use astersql_pd_schedule_schedulers::*;
use astersql_pd_server::schedule_wire;
use astersql_pd_statistics::{HotStat, LabelStatistics, RegionStatistics};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{self as kv, MemoryStore, MetaStore};
use astersql_pd_syncer::{Downstreams, HistoryBuffer};
use prometheus::Registry;
use prost::Message;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{runtime::Handle, sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;
const NOW: i64 = 1_000_000_000_000;
type Config = ProviderSnapshot<()>;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_135.json"
    ))
    .unwrap()
}
fn peer(store_id: u64) -> Peer {
    Peer {
        id: store_id + 10,
        store_id,
        ..Default::default()
    }
}
fn initial(case: &str) -> RegionInfo {
    let mut peers: Vec<_> = (1..=3).map(peer).collect();
    if case == "learner" {
        peers[2].role = PeerRole::Learner;
    }
    if case == "joint-recovery" {
        peers[1].role = PeerRole::IncomingVoter;
        peers[2].role = PeerRole::DemotingVoter;
    }
    if case == "nonwitness" {
        peers[2].is_witness = true;
    }
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"z".to_vec(),
            },
            epoch: Some(RegionEpoch {
                version: 10,
                conf_ver: 10,
            }),
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    );
    r.set_source(RegionSource::Heartbeat);
    r.stats_mut().approximate_size = 100;
    r.stats_mut().approximate_keys = 1000;
    r
}
struct Host {
    state: Arc<Mutex<HeartbeatEffectsState<Config>>>,
    hot: HotStat,
    runners: [ConcurrentRunner; 3],
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    downstreams: Downstreams,
    config: Config,
}
impl Host {
    async fn new(store: Arc<dyn MetaStore>) -> Self {
        let storage = Arc::new(StorageEndpoint::new(store));
        let paths = ClusterPaths::new(135);
        let mut config = Config::default();
        config.replication.enable_placement_rules = false;
        config.replication.max_replicas = 3;
        config.schedule.enable_joint_consensus = true;
        let cluster = BasicCluster::new();
        for id in 1..=4 {
            let mut s = StoreInfo::new(StoreMeta {
                id,
                node_state: NodeState::Serving,
                ..Default::default()
            });
            s.set_last_heartbeat_unix_nanos(NOW);
            s.update_raw_stats(StoreStats {
                capacity: 1_000_000_000_000,
                available: 900_000_000_000,
                used_size: 100_000_000_000,
                ..Default::default()
            });
            cluster.stores.put_store(Arc::new(s));
        }
        let mut rules = RuleManager::new(storage.clone(), paths);
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let mut runners = std::array::from_fn(|_| ConcurrentRunner::new(None));
        for r in &mut runners {
            r.start(&CancellationToken::new());
        }
        Self {
            state: Arc::new(Mutex::new(HeartbeatEffectsState {
                cluster,
                region_stats: Some(RegionStatistics::new()),
                label_stats: LabelStatistics::new(),
                config: config.clone(),
                rules,
                affinity: None,
            })),
            hot: HotStat::new(&Handle::current(), &Registry::new(), 128).unwrap(),
            runners,
            storage,
            paths,
            downstreams: Downstreams::new(HistoryBuffer::new(100).unwrap()),
            config,
        }
    }
    fn pipeline(&self) -> StatePipeline<'_, Config> {
        StatePipeline {
            state: &self.state,
            hot: &self.hot,
            runners: HeartbeatRunners {
                subtree: &self.runners[0],
                misc: &self.runners[1],
                sync: &self.runners[2],
            },
            storage: self.storage.clone(),
            paths: self.paths,
            downstreams: self.downstreams.clone(),
            bucket_enabled: true,
        }
    }
    async fn heartbeat(&self, r: RegionInfo) {
        let mut receipt = self.pipeline().region(r, 1000, NOW).unwrap();
        let failures = timeout(Duration::from_secs(5), receipt.wait())
            .await
            .unwrap();
        assert!(failures.is_empty(), "{failures:?}");
    }
    fn current(&self, id: u64) -> Arc<RegionInfo> {
        self.state
            .lock()
            .unwrap()
            .cluster
            .regions
            .get_region(id)
            .unwrap()
    }
    fn router_stores(&self) -> Arc<StoresInfo> {
        let stores = Arc::new(StoresInfo::new());
        for s in self.state.lock().unwrap().cluster.stores.get_stores() {
            stores.put_store(s);
        }
        stores
    }
    async fn stop(&mut self) {
        for r in &mut self.runners {
            r.stop().await;
        }
        self.hot.shutdown().await.unwrap();
    }
}
fn context<'a>(
    state: &'a HeartbeatEffectsState<Config>,
    config: &'a Config,
) -> BuilderContext<'a, ()> {
    BuilderContext {
        stores: &state.cluster.stores,
        config,
        rules: &[],
        now_unix_nanos: NOW as i128,
    }
}
async fn plan(h: &Host, case: &str, v2: bool) -> Vec<Arc<Operator>> {
    let mut s = h.state.lock().unwrap();
    let r = s.cluster.regions.get_region(1).unwrap();
    let ctx = context(&s, &h.config);
    match case {
        "learner" => vec![Arc::new(
            LearnerChecker::default()
                .check(ctx, &r, v2)
                .await
                .operator
                .unwrap(),
        )],
        "joint-recovery" => vec![Arc::new(
            JointStateChecker::default()
                .check(ctx, &r)
                .await
                .operator
                .unwrap(),
        )],
        "move" => vec![Arc::new(
            PeerOperatorFactory::new("loop-move", ctx, &r, v2)
                .create_move_peer(3, Some(peer(4)), OpKind::REGION, || Ok(14))
                .await
                .unwrap(),
        )],
        "witness" | "nonwitness" => vec![Arc::new(
            RangeOperatorFactory::new("loop-witness", ctx, &r, v2)
                .create_witness(3, case == "witness", || Ok(14))
                .await
                .unwrap(),
        )],
        "split" => vec![Arc::new(
            create_split_region_operator(
                "loop-split",
                &r,
                OpKind::REGION,
                SplitPolicy::UseKey,
                vec![b"m".to_vec()],
            )
            .unwrap(),
        )],
        "merge" => {
            let target = s.cluster.regions.get_region(2).unwrap();
            RangeOperatorFactory::new("loop-merge", ctx, &r, v2)
                .create_merge(&target, OpKind::MERGE, || Ok(14))
                .await
                .unwrap()
                .into_iter()
                .map(Arc::new)
                .collect()
        }
        _ => {
            let HeartbeatEffectsState { cluster, rules, .. } = &mut *s;
            let mut index = |n: usize| if case == "shuffle-leader" { 1 % n } else { 0 };
            let mut shuffle = |_: &mut [usize]| {};
            let mut ctx = EvictLeaderContext {
                cluster: &*cluster,
                config: &h.config,
                rules,
                now_unix_nanos: NOW as i128,
                random_index: &mut index,
                shuffle: &mut shuffle,
            };
            let out = match case {
                "evict" => {
                    EvictLeaderScheduler::new(
                        EvictLeaderConfig::new([(1, vec![KeyRange::default()])].into(), 1).unwrap(),
                    )
                    .schedule(&mut ctx)
                    .await
                }
                "grant" => {
                    GrantLeaderScheduler::new(GrantLeaderConfig::new(
                        [(2, vec![KeyRange::default()])].into(),
                    ))
                    .schedule(&mut ctx)
                    .await
                }
                "shuffle-leader" => {
                    ShuffleLeaderScheduler::new(ShuffleLeaderConfig::from_args(&[]).unwrap())
                        .schedule(&mut ctx)
                        .await
                }
                "shuffle-region" => {
                    let mut conf = ShuffleRegionConfig::from_args(&[]).unwrap();
                    conf.set_roles(vec!["follower".into()]).unwrap();
                    ShuffleRegionScheduler::new(conf)
                        .schedule(&mut ctx, v2, &mut || Ok(14))
                        .await
                }
                _ => panic!("unknown fixture story: {case}"),
            };
            assert!(out.errors.is_empty(), "{:?}", out.errors);
            assert_eq!(out.operators.len(), 1, "{case}");
            out.operators
        }
    }
}
fn command_kind(w: &pdpb::RegionHeartbeatResponse) -> &'static str {
    if w.transfer_leader.is_some() {
        "leader"
    } else if w.change_peer.is_some() {
        "peer"
    } else if w.change_peer_v2.is_some() {
        "v2"
    } else if w.switch_witnesses.is_some() {
        "witness"
    } else if w.split_region.is_some() {
        "split"
    } else if w.merge.is_some() {
        "merge"
    } else {
        panic!("empty command")
    }
}
// Decode both actual protocol adapters and compare every action/identity field.
fn wire(c: &HeartbeatCommand) -> pdpb::RegionHeartbeatResponse {
    let pd = schedule_wire::to_pdpb(c).unwrap();
    let pd = pdpb::RegionHeartbeatResponse::decode(pd.encode_to_vec().as_slice()).unwrap();
    let ng = schedule_wire::to_scheduling(c).unwrap();
    let ng = schedulingpb::RegionHeartbeatResponse::decode(ng.encode_to_vec().as_slice()).unwrap();
    assert_eq!(
        pd.header.as_ref().unwrap().cluster_id,
        ng.header.as_ref().unwrap().cluster_id
    );
    assert_eq!(pd.region_id, ng.region_id);
    assert_eq!(pd.region_epoch, ng.region_epoch);
    assert_eq!(pd.target_peer, ng.target_peer);
    assert_eq!(pd.change_peer, ng.change_peer);
    assert_eq!(pd.change_peer_v2, ng.change_peer_v2);
    assert_eq!(pd.transfer_leader, ng.transfer_leader);
    assert_eq!(pd.merge, ng.merge);
    assert_eq!(pd.split_region, ng.split_region);
    assert_eq!(pd.switch_witnesses, ng.switch_witnesses);
    pd
}
fn region_with(meta: RegionMeta, leader: u64, old: &RegionInfo) -> RegionInfo {
    let p = meta.peers.iter().find(|p| p.store_id == leader).cloned();
    let mut r = RegionInfo::new(meta, p, vec![], vec![]);
    *r.stats_mut() = old.stats().clone();
    r.set_source(RegionSource::Heartbeat);
    r
}
// An explicit external TiKV boundary: consume decoded messages, never Operator::step/check.
// It models successful Raft application, not TiKV consensus, network or durability.
async fn apply_wire(h: &Host, w: &pdpb::RegionHeartbeatResponse) {
    let old = h.current(w.region_id);
    let mut meta = old.meta().clone();
    let mut leader = old.leader().unwrap().store_id;
    assert_eq!(
        w.region_epoch.as_ref().unwrap().version,
        meta.epoch.unwrap().version
    );
    assert_eq!(
        w.region_epoch.as_ref().unwrap().conf_ver,
        meta.epoch.unwrap().conf_ver
    );
    if let Some(t) = &w.transfer_leader {
        leader = t.peer.as_ref().unwrap().store_id;
    }
    let mut changes = vec![];
    let mut joint = false;
    if let Some(c) = &w.change_peer {
        changes.push(c.clone());
    }
    if let Some(c) = &w.change_peer_v2 {
        changes = c.changes.clone();
        joint = changes.len() > 1;
        if changes.is_empty() {
            let mut count = 0;
            for p in &mut meta.peers {
                p.role = match p.role {
                    PeerRole::IncomingVoter => {
                        count += 1;
                        PeerRole::Voter
                    }
                    PeerRole::DemotingVoter => {
                        count += 1;
                        PeerRole::Learner
                    }
                    r => r,
                };
            }
            meta.epoch.as_mut().unwrap().conf_ver += count;
        }
    }
    for c in changes {
        let p = c.peer.unwrap();
        let mut new = Peer {
            id: p.id,
            store_id: p.store_id,
            is_witness: p.is_witness,
            role: PeerRole::Voter,
        };
        match c.change_type {
            0 => {
                if joint {
                    new.role = PeerRole::IncomingVoter;
                }
                meta.peers.retain(|v| v.store_id != new.store_id);
                meta.peers.push(new);
            }
            1 => {
                assert_ne!(leader, new.store_id);
                meta.peers.retain(|v| v.store_id != new.store_id);
            }
            2 => {
                new.role = if joint {
                    PeerRole::DemotingVoter
                } else {
                    PeerRole::Learner
                };
                meta.peers.retain(|v| v.store_id != new.store_id);
                meta.peers.push(new);
            }
            n => panic!("unknown conf-change {n}"),
        }
        meta.epoch.as_mut().unwrap().conf_ver += 1;
    }
    if let Some(b) = &w.switch_witnesses {
        for switch in &b.switch_witnesses {
            meta.peers
                .iter_mut()
                .find(|p| p.id == switch.peer_id)
                .unwrap()
                .is_witness = switch.is_witness;
            meta.epoch.as_mut().unwrap().conf_ver += 1;
        }
    }
    if let Some(split) = &w.split_region {
        assert_eq!(split.keys, vec![b"m".to_vec()]);
        meta.epoch.as_mut().unwrap().version += 1;
        let mut right = meta.clone();
        right.id = 2;
        right.key_range.start_key = b"m".to_vec();
        meta.key_range.end_key = b"m".to_vec();
        let mut left = region_with(meta, leader, &old);
        let mut right = region_with(right, leader, &old);
        left.stats_mut().approximate_size = 50;
        left.stats_mut().approximate_keys = 500;
        right.stats_mut().approximate_size = 50;
        right.stats_mut().approximate_keys = 500;
        h.heartbeat(left).await;
        h.heartbeat(right).await;
        return;
    }
    if let Some(merge) = &w.merge {
        let target = h.current(merge.target.as_ref().unwrap().id);
        let mut target_meta = target.meta().clone();
        target_meta.key_range.start_key = meta.key_range.start_key;
        target_meta.epoch.as_mut().unwrap().version = meta
            .epoch
            .unwrap()
            .version
            .max(target_meta.epoch.unwrap().version)
            + 1;
        let mut merged = region_with(target_meta, target.leader().unwrap().store_id, &target);
        merged.stats_mut().approximate_size = 200;
        merged.stats_mut().approximate_keys = 2000;
        h.heartbeat(merged).await;
        return;
    }
    h.heartbeat(region_with(meta, leader, &old)).await;
}
async fn run_case(case: &Value, v2: bool) {
    let name = case["name"].as_str().unwrap();
    let mut h = Host::new(Arc::new(MemoryStore::default())).await;
    let mut initial = initial(name);
    if name == "merge" {
        let mut left = initial.meta().clone();
        left.key_range.end_key = b"m".to_vec();
        initial = region_with(left, 1, &initial);
        let mut right = initial.meta().clone();
        right.id = 2;
        right.key_range = KeyRange {
            start_key: b"m".to_vec(),
            end_key: b"z".to_vec(),
        };
        h.heartbeat(region_with(right, 1, &initial)).await;
    }
    h.heartbeat(initial.clone()).await;
    let ops = plan(&h, name, v2).await;
    let ctl = ControllerAdmission::new(100);
    ctl.add_operators(&h.state.lock().unwrap().cluster, &ops)
        .unwrap();
    let (handle, mut router) = StreamRouter::new(h.router_stores());
    let (tx, mut rx) = mpsc::channel::<HeartbeatCommand>(64);
    let tx = Arc::new(tx);
    for id in 1..=4 {
        handle.bind(id, tx.clone()).await.unwrap();
        router.step().await.unwrap();
    }
    let mut commands = vec![];
    let mut completed = 0;
    let mut disappeared = 0;
    for _ in 0..20 {
        if ctl.operators().is_empty() {
            break;
        }
        let mut queued = 0;
        {
            let s = h.state.lock().unwrap();
            let dispatch = ControllerDispatch::new(&ctl, &s.cluster, &h.config, &handle, 135);
            let mut active = ctl.operators();
            // Feed surviving target heartbeats before the notifier polls disappeared sources.
            active.sort_by_key(|op| s.cluster.regions.get_region(op.region_id()).is_none());
            for op in active {
                if ctl.get_operator(op.region_id()).is_none() {
                    continue;
                }
                if let Some(r) = s.cluster.regions.get_region(op.region_id()) {
                    let out = dispatch
                        .dispatch(
                            &r,
                            DispatchSource::Heartbeat,
                            Instant::now(),
                            NOW as i128,
                            v2,
                        )
                        .await
                        .unwrap();
                    assert!(out.stale_error.is_none(), "{name}: {:?}", out.stale_error);
                    queued += usize::from(out.enqueued);
                    completed += usize::from(out.completed.is_some());
                } else {
                    dispatch.schedule(op, Instant::now());
                    let poll = dispatch.poll_due(Instant::now());
                    disappeared += poll.removed.len();
                }
            }
        }
        for _ in 0..queued {
            assert!(matches!(
                timeout(Duration::from_secs(5), router.step())
                    .await
                    .unwrap(),
                Some(RouteEvent::Sent(_))
            ));
            let c = rx.recv().await.unwrap();
            let w = wire(&c);
            commands.push(command_kind(&w));
            // Replayed heartbeat before Raft applies cannot advance or complete the operation.
            let before = h.current(c.region_id);
            h.heartbeat((*before).clone()).await;
            assert_eq!(h.current(c.region_id).meta(), before.meta());
            let op = ctl.get_operator(c.region_id).unwrap();
            let step = op.current_step();
            {
                let state = h.state.lock().unwrap();
                let d = ControllerDispatch::new(&ctl, &state.cluster, &h.config, &handle, 135);
                let duplicate = d
                    .dispatch(
                        &before,
                        DispatchSource::Heartbeat,
                        Instant::now(),
                        NOW as i128,
                        v2,
                    )
                    .await
                    .unwrap();
                assert!(duplicate.enqueued && duplicate.completed.is_none());
                assert_eq!(
                    op.current_step(),
                    step,
                    "duplicate must not advance the step"
                );
            }
            assert!(matches!(
                timeout(Duration::from_secs(5), router.step())
                    .await
                    .unwrap(),
                Some(RouteEvent::Sent(_))
            ));
            assert_eq!(
                rx.recv().await.unwrap(),
                c,
                "duplicate dispatch must retain the entire command"
            );
            apply_wire(&h, &w).await;
            // Older version/confver reports are rejected before scheduler/controller observation.
            if h.state
                .lock()
                .unwrap()
                .cluster
                .regions
                .get_region(c.region_id)
                .is_some()
            {
                let current = h.current(c.region_id);
                let mut stale = before.meta().clone();
                stale.epoch.as_mut().unwrap().version = 1;
                assert!(
                    h.pipeline()
                        .region(region_with(stale, 1, &before), 1000, NOW)
                        .is_err()
                );
                assert!(Arc::ptr_eq(&current, &h.current(c.region_id)));
            }
        }
    }
    assert!(ctl.operators().is_empty(), "{name}: failed to converge");
    assert_eq!(
        completed,
        case["completed"].as_u64().unwrap() as usize,
        "{name}"
    );
    assert_eq!(
        disappeared,
        case["disappeared"].as_u64().unwrap_or(0) as usize,
        "{name}"
    );
    let expected = &case[if v2 { "commands_v2" } else { "commands_v1" }];
    assert_eq!(json!(commands), *expected, "{name} v2={v2}");
    assert_eq!(
        ctl.get_op_influence(Some(&h.state.lock().unwrap().cluster), &[])
            .snapshot()
            .len(),
        0
    );
    for op in &ops {
        assert_eq!(
            op.status(),
            if case["cancel_reason"].is_string() || (op.region_id() == 1 && name == "merge") {
                CANCELED
            } else {
                SUCCESS
            },
            "{name}"
        );
    }
    if let Some(reason) = case["cancel_reason"].as_str() {
        assert_eq!(
            ops[0].additional_info().get("cancel-reason").as_deref(),
            Some(reason)
        );
    }
    let snapshot = h.pipeline().snapshot().await.unwrap();
    assert_eq!(
        snapshot.regions.len(),
        case["regions"].as_u64().unwrap() as usize
    );
    let final_ranges: Vec<_> = snapshot
        .regions
        .iter()
        .map(|r| {
            json!([
                r.meta().id,
                String::from_utf8(r.meta().key_range.start_key.clone()).unwrap(),
                String::from_utf8(r.meta().key_range.end_key.clone()).unwrap()
            ])
        })
        .collect();
    assert_eq!(json!(final_ranges), case["ranges"], "{name}");
    for region in &snapshot.regions {
        assert_eq!(
            region.meta().epoch.unwrap().conf_ver,
            case[if v2 { "conf_v2" } else { "conf_v1" }]
                .as_u64()
                .unwrap(),
            "{name}"
        );
        assert_eq!(
            region.meta().epoch.unwrap().version,
            case["version"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            region.stats().approximate_size,
            case["size"].as_i64().unwrap(),
            "{name}"
        );
        assert_eq!(
            region.stats().approximate_keys,
            case["keys"].as_i64().unwrap(),
            "{name}"
        );
        assert_eq!(
            h.storage
                .load_region(&h.paths, region.meta().id)
                .await
                .unwrap(),
            Some(region.meta().clone())
        );
        let final_peers: Vec<_> = region
            .meta()
            .peers
            .iter()
            .map(|p| {
                json!([
                    p.store_id,
                    match p.role {
                        PeerRole::Voter => 0,
                        PeerRole::Learner => 1,
                        PeerRole::IncomingVoter => 2,
                        PeerRole::DemotingVoter => 3,
                        PeerRole::Unknown(n) => n,
                    },
                    p.is_witness
                ])
            })
            .collect();
        let mut final_peers = final_peers;
        final_peers.sort_by_key(|p| p[0].as_u64().unwrap());
        assert_eq!(json!(final_peers), case["peers"], "{name}");
        assert_eq!(
            region.leader().unwrap().store_id,
            case["leader"].as_u64().unwrap(),
            "{name}"
        );
        // Source-defined role/witness rule shape, checked by real placement fitting.
        let rules: Vec<_> = case["rules"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                Arc::new(Rule {
                    group_id: "pd".into(),
                    id: i.to_string(),
                    role: r[0].as_str().unwrap().to_string().into(),
                    count: r[1].as_u64().unwrap() as i64,
                    is_witness: r[2].as_bool().unwrap(),
                    ..Default::default()
                })
            })
            .collect();
        assert!(
            fit_region(
                &h.state.lock().unwrap().cluster.stores.get_stores(),
                region,
                &rules,
                true
            )
            .is_satisfied(),
            "{name}: invalid placement"
        );
        h.heartbeat((**region).clone()).await;
    }
    if name == "merge" {
        assert!(h.storage.load_region(&h.paths, 1).await.unwrap().is_none());
    }
    if name == "learner" {
        assert!(
            LearnerChecker::default()
                .check(
                    context(&h.state.lock().unwrap(), &h.config),
                    &snapshot.regions[0],
                    v2
                )
                .await
                .operator
                .is_none()
        );
    }
    if name == "joint-recovery" {
        assert!(
            JointStateChecker::default()
                .check(
                    context(&h.state.lock().unwrap(), &h.config),
                    &snapshot.regions[0]
                )
                .await
                .operator
                .is_none()
        );
    }
    println!(
        "task135 {name} v2={v2}: commands={commands:?}, completed={completed}, disappeared={disappeared}, influence=0, placement=true"
    );
    h.stop().await;
}
#[tokio::test]
async fn task_135_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    assert_eq!(f["go_execution"], false);
    let mut count = 0;
    for case in f["cases"].as_array().unwrap() {
        for v2 in [false, true] {
            if !v2 && case["v2_only"] == true {
                continue;
            }
            let mut resolved = case.clone();
            if !v2 && let Some(overrides) = case["expected_v1"].as_object() {
                for (k, v) in overrides {
                    resolved[k] = v.clone();
                }
            }
            run_case(&resolved, v2).await;
            count += 1;
        }
    }
    assert_eq!(count, 21);
    println!("task135 effective closed-loop cases={count}");
}

// Fault injection is confined to the actual metadata I/O boundary.
#[derive(Default)]
struct FaultStore {
    inner: MemoryStore,
    fail: AtomicBool,
}
#[async_trait::async_trait]
impl MetaStore for FaultStore {
    async fn read(&self, r: kv::Read) -> kv::Result<kv::Snapshot> {
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<kv::Compare>, w: Vec<kv::Mutation>) -> kv::Result<kv::Commit> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(kv::StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<kv::Compare>,
        t: Vec<kv::Mutation>,
        e: Vec<kv::Mutation>,
    ) -> kv::Result<kv::Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<kv::Compare>,
        t: Vec<kv::RawTxnOp>,
        e: Vec<kv::RawTxnOp>,
    ) -> kv::Result<kv::RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: kv::Revision) -> kv::Result<Box<dyn kv::MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> kv::Result<kv::Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: kv::LeaseId) -> kv::Result<kv::Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: kv::LeaseId) -> kv::Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: kv::LeaseId) -> kv::Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: kv::Revision) -> kv::Result<()> {
        self.inner.compact(r).await
    }
}
#[tokio::test]
async fn task_135_metadata_failure_is_not_rolled_back_or_retried_by_duplicate() {
    use astersql_pd_cluster::HeartbeatEffectFailure;
    let store = Arc::new(FaultStore::default());
    let mut h = Host::new(store.clone()).await;
    store.fail.store(true, Ordering::SeqCst);
    let r = initial("learner");
    let mut receipt = h.pipeline().region(r.clone(), 1000, NOW).unwrap();
    let failures = timeout(Duration::from_secs(5), receipt.wait())
        .await
        .unwrap();
    assert!(
        matches!(
            failures.as_slice(),
            [HeartbeatEffectFailure::SaveRegion(1, _)]
        ),
        "{failures:?}"
    );
    assert_eq!(h.current(1).meta(), r.meta());
    assert!(h.storage.load_region(&h.paths, 1).await.unwrap().is_none());
    store.fail.store(false, Ordering::SeqCst);
    h.heartbeat(r.clone()).await;
    assert!(h.storage.load_region(&h.paths, 1).await.unwrap().is_none());
    let mut meta = r.meta().clone();
    meta.epoch.as_mut().unwrap().conf_ver += 1;
    let updated = region_with(meta, 1, &r);
    h.heartbeat(updated.clone()).await;
    assert_eq!(
        h.storage.load_region(&h.paths, 1).await.unwrap(),
        Some(updated.meta().clone())
    );
    h.stop().await;
    assert!(h.pipeline().snapshot().await.is_err());
    println!(
        "task135 metadata faults: write failure retained root, duplicate did not retry, changed epoch repaired; stopped workers rejected snapshot"
    );
}
#[tokio::test]
async fn task_135_route_failure_retry_and_stale_confver() {
    for stale in [false, true] {
        let mut h = Host::new(Arc::new(MemoryStore::default())).await;
        h.heartbeat(initial("grant")).await;
        let ops = plan(&h, "grant", true).await;
        let ctl = ControllerAdmission::new(10);
        ctl.add_operators(&h.state.lock().unwrap().cluster, &ops)
            .unwrap();
        let (closed, router) = StreamRouter::new(h.router_stores());
        drop(router);
        {
            let s = h.state.lock().unwrap();
            let d = ControllerDispatch::new(&ctl, &s.cluster, &h.config, &closed, 135);
            assert!(
                d.dispatch(
                    &s.cluster.regions.get_region(1).unwrap(),
                    DispatchSource::Heartbeat,
                    Instant::now(),
                    NOW as i128,
                    true
                )
                .await
                .is_err()
            );
        }
        assert_eq!(ops[0].status(), STARTED);
        assert_eq!(ctl.operators().len(), 1);
        let (handle, mut router) = StreamRouter::new(h.router_stores());
        let (tx, mut rx) = mpsc::channel(4);
        handle.bind(1, Arc::new(tx)).await.unwrap();
        router.step().await.unwrap();
        if stale {
            let r = h.current(1);
            let mut meta = r.meta().clone();
            meta.epoch.as_mut().unwrap().conf_ver += 1;
            h.heartbeat(region_with(meta, 1, &r)).await;
        }
        let r = h.current(1);
        {
            let s = h.state.lock().unwrap();
            let d = ControllerDispatch::new(&ctl, &s.cluster, &h.config, &handle, 135);
            let out = d
                .dispatch(
                    &r,
                    DispatchSource::Heartbeat,
                    Instant::now(),
                    NOW as i128,
                    true,
                )
                .await
                .unwrap();
            assert_eq!(out.enqueued, !stale);
            assert_eq!(out.removed.len(), usize::from(stale));
        }
        if stale {
            assert_eq!(ops[0].status(), CANCELED);
            assert_eq!(
                ops[0].additional_info().get("cancel-reason").as_deref(),
                Some("epoch not match")
            );
            assert!(rx.try_recv().is_err());
        } else {
            assert!(matches!(router.step().await, Some(RouteEvent::Sent(1))));
            let cmd = rx.recv().await.unwrap();
            apply_wire(&h, &wire(&cmd)).await;
            let r = h.current(1);
            let s = h.state.lock().unwrap();
            let d = ControllerDispatch::new(&ctl, &s.cluster, &h.config, &handle, 135);
            assert!(
                d.dispatch(
                    &r,
                    DispatchSource::Heartbeat,
                    Instant::now(),
                    NOW as i128,
                    true
                )
                .await
                .unwrap()
                .completed
                .is_some()
            );
            assert_eq!(ops[0].status(), SUCCESS);
        }
        assert!(ctl.operators().is_empty());
        assert!(
            ctl.get_op_influence(Some(&h.state.lock().unwrap().cluster), &[])
                .snapshot()
                .is_empty()
        );
        h.stop().await;
    }
    println!(
        "task135 route faults: closed queue retained STARTED, rebind completed; unexpected confver canceled without sending"
    );
}
