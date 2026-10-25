// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::{
    BasicCluster, Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::{
    HeartbeatCommand,
    routing::{RouteEvent, StreamRouter},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn peer(id: u64) -> Peer {
    Peer {
        id: id + 10,
        store_id: id,
        ..Default::default()
    }
}
fn region(leader: u64, conf: u64) -> RegionInfo {
    RegionInfo::new(
        RegionMeta {
            id: 1,
            epoch: Some(RegionEpoch {
                version: 3,
                conf_ver: conf,
            }),
            peers: vec![peer(1), peer(2)],
            key_range: astersql_pd_core::KeyRange {
                start_key: vec![0],
                end_key: vec![1],
            },
            ..Default::default()
        },
        (leader != 0).then(|| peer(leader)),
        vec![],
        vec![],
    )
}
fn cluster() -> BasicCluster {
    let mut c = BasicCluster::new();
    c.regions.set_region(Arc::new(region(1, 4)));
    for id in 1..=3 {
        c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    c
}
// The transport boundary uses real StoreInfo handles from the cluster snapshot.
fn router_stores(c: &BasicCluster) -> Arc<astersql_pd_core::StoresInfo> {
    let stores = Arc::new(astersql_pd_core::StoresInfo::new());
    for store in c.stores.get_stores() {
        stores.put_store(store);
    }
    stores
}
fn leader() -> OpStep {
    TransferLeader {
        from_store: 1,
        to_store: 2,
        to_stores: vec![2, 2],
    }
    .into()
}
fn op(steps: Vec<OpStep>) -> Arc<Operator> {
    Arc::new(Operator::new(
        "dispatch",
        "",
        1,
        region(1, 4).meta().epoch,
        OpKind::REGION,
        1,
        steps,
    ))
}
fn reason(o: &Operator) -> Option<String> {
    o.additional_info().get("cancel-reason")
}

#[tokio::test]
async fn task_036_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_036.json"
    ))
    .unwrap();
    assert_eq!(fixture["evidence_kind"], "source_assertion");
    for case in fixture["cases"].as_array().unwrap() {
        let c = cluster();
        let cfg = ProviderSnapshot::<()>::default();
        let a = ControllerAdmission::new(10);
        let (h, mut router) = StreamRouter::new(router_stores(&c));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<HeartbeatCommand>(10);
        h.bind(1, Arc::new(tx)).await.unwrap();
        router.step().await.unwrap();
        let d = ControllerDispatch::new(&a, &c, &cfg, &h, 42);
        let o = op(vec![leader()]);
        a.add_operators(&c, &[o.clone()]).unwrap();
        let src = match case["source"].as_str().unwrap() {
            "heartbeat" => DispatchSource::Heartbeat,
            "create" => DispatchSource::Create,
            _ => DispatchSource::Notifier,
        };
        let r = region(
            case["leader"].as_u64().unwrap(),
            case["conf_ver"].as_u64().unwrap(),
        );
        let out = d.dispatch(&r, src, Instant::now(), 0, true).await.unwrap();
        assert_eq!(out.enqueued, case["send"].as_bool().unwrap(), "{case}");
        assert_eq!(
            o.status(),
            if case["status"] == "success" {
                SUCCESS
            } else if case["status"] == "canceled" {
                CANCELED
            } else {
                STARTED
            },
            "{case}"
        );
        if out.enqueued {
            assert!(matches!(router.step().await, Some(RouteEvent::Sent(1))));
            let cmd = rx.recv().await.unwrap();
            assert_eq!(
                cmd,
                HeartbeatCommand {
                    cluster_id: 42,
                    region_id: 1,
                    region_epoch: Some(RegionEpoch {
                        version: 3,
                        conf_ver: case["conf_ver"].as_u64().unwrap()
                    }),
                    target_peer: peer(1),
                    operation: astersql_pd_schedule_hbstream::Operation {
                        transfer_leader: Some(astersql_pd_schedule_hbstream::TransferLeader {
                            peer: Some(peer(2)),
                            peers: vec![Some(peer(2)), Some(peer(2))],
                        }),
                        ..Default::default()
                    },
                }
            );
        } else {
            assert!(rx.try_recv().is_err());
        }
        if let Some(expected) = case["reason"].as_str() {
            assert_eq!(reason(&o).as_deref(), Some(expected));
        }
        assert_eq!(out.promote_waiting, !out.removed.is_empty());
    }
}

#[tokio::test]
async fn task_036_stale_target_and_health_opt_in() {
    for health in [false, true] {
        let c = cluster();
        let mut cfg = ProviderSnapshot::<()>::default();
        cfg.schedule.max_store_down_time = "1800s".parse().unwrap();
        let a = ControllerAdmission::new(10);
        let (h, _router) = StreamRouter::new(router_stores(&c));
        let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
        let o = op(vec![leader()]);
        o.set_store_health_check(health);
        a.add_operators(&c, &[o.clone()]).unwrap();
        let out = d
            .dispatch(
                &region(1, 4),
                DispatchSource::Heartbeat,
                Instant::now(),
                601_000_000_000,
                true,
            )
            .await
            .unwrap();
        assert_eq!(out.enqueued, !health);
        assert_eq!(reason(&o).as_deref(), health.then_some("stale status"));
    }
    let c = cluster();
    let mut cfg = ProviderSnapshot::<()>::default();
    cfg.schedule.max_store_down_time = "1800s".parse().unwrap();
    let a = ControllerAdmission::new(10);
    let (h, _router) = StreamRouter::new(router_stores(&c));
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
    let o = op(vec![
        TransferLeader {
            to_store: 9,
            ..Default::default()
        }
        .into(),
    ]);
    a.add_operators(&c, &[o.clone()]).unwrap();
    let out = d
        .dispatch(
            &region(1, 4),
            DispatchSource::Heartbeat,
            Instant::now(),
            0,
            true,
        )
        .await
        .unwrap();
    assert!(!out.enqueued);
    assert!(matches!(
        out.stale_error,
        Some(DispatchStepError::Leader(_))
    ));
    assert_eq!(reason(&o).as_deref(), Some("stale status"));
}

#[tokio::test]
async fn task_036_progress_and_confver_budget() {
    let c = cluster();
    let cfg = ProviderSnapshot::<()>::default();
    let a = ControllerAdmission::new(10);
    let (h, mut router) = StreamRouter::new(router_stores(&c));
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 7);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<HeartbeatCommand>(10);
    h.bind(2, Arc::new(tx)).await.unwrap();
    router.step().await;
    let o = op(vec![
        leader(),
        RemovePeer {
            from_store: 1,
            peer_id: 11,
            ..Default::default()
        }
        .into(),
    ]);
    a.add_operators(&c, &[o.clone()]).unwrap();
    let r = region(2, 4);
    let out = d
        .dispatch(&r, DispatchSource::Heartbeat, Instant::now(), 0, true)
        .await
        .unwrap();
    assert!(out.enqueued);
    assert_eq!(o.current_step(), 1);
    router.step().await;
    let cmd = rx.recv().await.unwrap();
    assert_eq!(cmd.target_peer, peer(2));
    assert_eq!(
        cmd.operation.change_peer_v2.unwrap().changes[0]
            .as_ref()
            .unwrap()
            .peer,
        Some(peer(1))
    );
    let mut meta = r.meta().clone();
    meta.peers = vec![peer(2)];
    meta.epoch.as_mut().unwrap().conf_ver = 5;
    let finished = RegionInfo::new(meta, Some(peer(2)), vec![], vec![]);
    let out = d
        .dispatch(
            &finished,
            DispatchSource::Heartbeat,
            Instant::now(),
            0,
            true,
        )
        .await
        .unwrap();
    assert_eq!(o.status(), SUCCESS);
    assert!(!out.enqueued);
    assert_eq!(out.removed.len(), 1);
    assert!(out.completed.is_some());
    assert!(a.operators().is_empty());
    let inf = OpInfluence::new();
    d.fast_operators.add_to_at(&c, &inf, Instant::now());
    assert!(!inf.snapshot().is_empty());
}

#[tokio::test]
async fn task_036_retry_deadlines_disconnect_rebind() {
    let c = cluster();
    let cfg = ProviderSnapshot::<()>::default();
    let a = ControllerAdmission::new(10);
    let (h, mut router) = StreamRouter::new(router_stores(&c));
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 42);
    let o = op(vec![leader()]);
    a.add_operators(&c, &[o.clone()]).unwrap();
    let now = Instant::now();
    d.schedule(o.clone(), now);
    assert_eq!(
        d.push_due(now + Duration::from_secs(1), 0, true)
            .await
            .unwrap()
            .len(),
        0
    );
    let (tx, rx) = tokio::sync::mpsc::channel::<HeartbeatCommand>(1);
    drop(rx);
    h.bind(1, Arc::new(tx)).await.unwrap();
    router.step().await;
    let out = d
        .push_due(now + Duration::from_secs(2), 0, true)
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    assert!(out[0].enqueued);
    assert!(matches!(
        router.step().await,
        Some(RouteEvent::SendFailed { store_id: 1, .. })
    ));
    assert_eq!(o.status(), STARTED);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<HeartbeatCommand>(1);
    h.bind(1, Arc::new(tx)).await.unwrap();
    router.step().await;
    assert_eq!(
        d.push_due(now + Duration::from_secs(3), 0, true)
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        d.push_due(now + Duration::from_secs(4), 0, true)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(router.step().await, Some(RouteEvent::Sent(1))));
    assert_eq!(rx.recv().await.unwrap().region_id, 1);
    a.remove_operator(&o, CancelReason::AdminStop);
    assert!(
        d.push_due(now + Duration::from_secs(6), 0, true)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(d.push_queue.is_empty());
}

struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
#[tokio::test]
async fn task_036_timeout_and_closed_routing() {
    let c = cluster();
    let cfg = ProviderSnapshot::<()>::default();
    let a = ControllerAdmission::new(10);
    let (h, router) = StreamRouter::new(router_stores(&c));
    drop(router);
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let o = Arc::new(Operator::with_clock(
        "timeout",
        "",
        1,
        region(1, 4).meta().epoch,
        OpKind::LEADER,
        1,
        vec![leader()],
        clock.clone(),
    ));
    a.add_operators(&c, &[o.clone()]).unwrap();
    assert!(
        d.dispatch(
            &region(1, 4),
            DispatchSource::Create,
            Instant::now(),
            0,
            true
        )
        .await
        .is_err()
    );
    assert_eq!(o.status(), STARTED);
    *clock.0.lock().unwrap() += Duration::from_secs(61);
    let out = d
        .dispatch(
            &region(1, 4),
            DispatchSource::Heartbeat,
            clock.now(),
            0,
            true,
        )
        .await
        .unwrap();
    assert!(!out.enqueued);
    assert_eq!(o.status(), TIMEOUT);
    assert_eq!(reason(&o).as_deref(), Some("timeout"));
    assert!(out.promote_waiting);
}

#[tokio::test]
async fn task_036_pending_peer_no_duplicate_command() {
    let c = cluster();
    let cfg = ProviderSnapshot::<()>::default();
    let a = ControllerAdmission::new(10);
    let (h, _router) = StreamRouter::new(router_stores(&c));
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
    let o = op(vec![
        AddLearner {
            to_store: 3,
            peer_id: 13,
            ..Default::default()
        }
        .into(),
        leader(),
    ]);
    o.set_store_health_check(true);
    a.add_operators(&c, &[o.clone()]).unwrap();
    // Pending peer has already consumed one confver, but cannot advance the step.
    let mut meta = region(1, 5).meta().clone();
    let mut p = peer(3);
    p.role = astersql_pd_core::PeerRole::Learner;
    meta.peers.push(p.clone());
    let pending = RegionInfo::new(meta, Some(peer(1)), vec![], vec![p]);
    let out = d
        .dispatch(&pending, DispatchSource::Heartbeat, Instant::now(), 0, true)
        .await
        .unwrap();
    assert!(!out.enqueued);
    assert_eq!(o.status(), STARTED);
    assert_eq!(o.current_step(), 0);
    assert_eq!(o.conf_ver_changed(&pending), 1);
    let finished = RegionInfo::new(pending.meta().clone(), Some(peer(1)), vec![], vec![]);
    let out = d
        .dispatch(
            &finished,
            DispatchSource::Heartbeat,
            Instant::now(),
            0,
            true,
        )
        .await
        .unwrap();
    assert!(out.enqueued);
    assert_eq!(o.current_step(), 1);
}

#[tokio::test]
async fn task_036_health_mask_and_down_rejection() {
    for now in [601_000_000_000, 1801_000_000_000] {
        let c = cluster();
        let mut cfg = ProviderSnapshot::<()>::default();
        cfg.schedule.max_store_down_time = "1800s".parse().unwrap();
        let a = ControllerAdmission::new(10);
        let (h, _router) = StreamRouter::new(router_stores(&c));
        let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
        let o = op(vec![
            AddLearner {
                to_store: 3,
                peer_id: 13,
                ..Default::default()
            }
            .into(),
        ]);
        o.set_store_health_check(true);
        a.add_operators(&c, &[o.clone()]).unwrap();
        let out = d
            .dispatch(
                &region(1, 4),
                DispatchSource::Heartbeat,
                Instant::now(),
                now,
                true,
            )
            .await
            .unwrap();
        assert_eq!(out.enqueued, now < 1800_000_000_000);
        assert_eq!(out.stale_error.is_some(), now > 1800_000_000_000);
    }
}

#[tokio::test]
async fn task_036_merge_light_check_and_missing_region() {
    for missing in [false, true] {
        let mut c = cluster();
        let cfg = ProviderSnapshot::<()>::default();
        let a = ControllerAdmission::new(10);
        let mut meta = region(1, 4).meta().clone();
        meta.id = 2;
        meta.key_range.start_key = vec![1];
        meta.key_range.end_key = vec![2];
        c.regions.set_region(Arc::new(RegionInfo::new(
            meta.clone(),
            Some(peer(1)),
            vec![],
            vec![],
        )));
        let x = op(vec![leader()]);
        let y = Arc::new(Operator::new(
            "pair",
            "",
            2,
            meta.epoch,
            OpKind::MERGE,
            1,
            vec![leader()],
        ));
        x.additional_info().set(RELATED_MERGE_REGION, "2");
        y.additional_info().set(RELATED_MERGE_REGION, "1");
        a.add_operators(&c, &[x.clone(), y.clone()]).unwrap();
        if missing {
            c.regions.remove_region_if_exist(1);
        } else {
            let mut m = region(1, 4).meta().clone();
            m.epoch.as_mut().unwrap().version += 1;
            c.regions
                .set_region(Arc::new(RegionInfo::new(m, Some(peer(1)), vec![], vec![])));
        }
        let (h, _router) = StreamRouter::new(router_stores(&c));
        let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
        let now = Instant::now();
        d.push_queue.push(OperatorWithTime {
            op: x.clone(),
            deadline: now + Duration::from_secs(1),
        });
        d.push_queue.push(OperatorWithTime {
            op: y.clone(),
            deadline: now + Duration::from_secs(2),
        });
        let poll = d.poll_due(now);
        assert!(poll.next);
        assert!(poll.region.is_none());
        assert_eq!(poll.removed.len(), 2);
        assert!(a.operators().is_empty());
        assert_eq!(
            reason(&x).as_deref(),
            Some(if missing {
                "region not found"
            } else {
                "epoch not match"
            })
        );
        assert_eq!(reason(&y).as_deref(), Some("related merge region"));
        assert!(d.poll_due(now).next);
        assert!(d.push_queue.is_empty());
    }
}

#[tokio::test]
async fn task_036_slow_retry_replacement_and_success_hook() {
    let c = cluster();
    let cfg = ProviderSnapshot::<()>::default();
    let a = ControllerAdmission::new(10);
    let (h, _router) = StreamRouter::new(router_stores(&c));
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 1);
    let now = Instant::now();
    let old = op(vec![
        AddPeer {
            to_store: 3,
            peer_id: 13,
            ..Default::default()
        }
        .into(),
    ]);
    a.add_operators(&c, &[old.clone()]).unwrap();
    d.schedule(old.clone(), now);
    assert!(!d.poll_due(now + Duration::from_secs(4)).next);
    let new = op(vec![leader()]);
    new.set_priority_level(astersql_pd_core_constant::PriorityLevel::HIGH);
    a.add_operators(&c, &[new.clone()]).unwrap();
    assert_eq!(old.status(), REPLACED);
    let poll = d.poll_due(now + Duration::from_secs(5));
    assert!(poll.next);
    let r = poll.region.unwrap();
    let out = d
        .dispatch(&r, DispatchSource::Notifier, now, 0, true)
        .await
        .unwrap();
    assert!(out.enqueued);
    assert!(Arc::ptr_eq(&a.get_operator(1).unwrap(), &new));
    a.remove_operator(&new, CancelReason::AdminStop);
    let done = op(vec![
        BecomeNonWitness {
            store_id: 2,
            peer_id: 12,
            ..Default::default()
        }
        .into(),
    ]);
    a.add_operators(&c, &[done.clone()]).unwrap();
    // Completed operators bypass even a future notifier deadline.
    d.schedule(done.clone(), now);
    let poll = d.poll_due(now);
    assert!(poll.next);
    assert!(poll.region.is_some());
    let out = d
        .dispatch(
            &region(1, 4),
            DispatchSource::Notifier,
            now + Duration::from_secs(11),
            0,
            true,
        )
        .await
        .unwrap();
    assert!(out.record_non_witness);
    assert!(out.completed.is_some());
    assert!(reason(&done).is_none());
    assert_eq!(done.status(), SUCCESS);
    let inf = OpInfluence::new();
    d.fast_operators
        .add_to_at(&c, &inf, now + Duration::from_secs(11));
    assert!(inf.snapshot().is_empty());
}

#[tokio::test]
async fn task_036_command_versions_and_suppression() {
    let c = cluster();
    let cfg = ProviderSnapshot::<()>::default();
    let a = ControllerAdmission::new(10);
    let (h, mut router) = StreamRouter::new(router_stores(&c));
    let d = ControllerDispatch::new(&a, &c, &cfg, &h, 9);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<HeartbeatCommand>(2);
    h.bind(1, Arc::new(tx)).await.unwrap();
    router.step().await;
    for v2 in [false, true] {
        let step = OpStep::AddLearner(AddLearner {
            to_store: 3,
            peer_id: 33,
            is_witness: true,
            ..Default::default()
        });
        assert!(
            d.send_schedule_command(&region(1, 4), &step, v2)
                .await
                .unwrap()
        );
        router.step().await;
        let cmd = rx.recv().await.unwrap();
        assert_eq!(cmd.cluster_id, 9);
        assert_eq!(cmd.region_epoch, region(1, 4).meta().epoch);
        assert_eq!(cmd.target_peer, peer(1));
        let change = if v2 {
            cmd.operation.change_peer_v2.unwrap().changes[0]
                .clone()
                .unwrap()
        } else {
            cmd.operation.change_peer.unwrap()
        };
        assert_eq!(
            change.change_type,
            astersql_pd_schedule_hbstream::ChangeType::AddLearnerNode
        );
        let p = change.peer.unwrap();
        assert_eq!((p.id, p.store_id, p.is_witness), (33, 3, true));
    }
    let passive = OpStep::MergeRegion(MergeRegion {
        is_passive: true,
        to_region: Some(region(1, 4).meta().clone()),
        ..Default::default()
    });
    assert!(
        !d.send_schedule_command(&region(1, 4), &passive, true)
            .await
            .unwrap()
    );
    let joint = OpStep::ChangePeerV2Leave(ChangePeerV2Leave::default());
    assert!(
        !d.send_schedule_command(&region(1, 4), &joint, false)
            .await
            .unwrap()
    );
    assert!(
        d.send_schedule_command(&region(1, 4), &joint, true)
            .await
            .unwrap()
    );
    router.step().await;
    assert!(
        rx.recv()
            .await
            .unwrap()
            .operation
            .change_peer_v2
            .unwrap()
            .changes
            .is_empty()
    );
    let now = Instant::now();
    for step in [
        leader(),
        PromoteLearner::default().into(),
        ChangePeerV2Enter::default().into(),
        joint,
    ] {
        assert_eq!(
            next_push_deadline(Some(&step), now),
            now + Duration::from_secs(2)
        );
    }
    assert_eq!(next_push_deadline(None, now), now + Duration::from_secs(5));
}
