// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_storelimit::Type;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct ManualClock(Mutex<Instant>);
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}
impl ManualClock {
    fn advance(&self, d: Duration) {
        *self.0.lock().unwrap() += d;
    }
}
fn region(stores: &[u64], leader: u64, size: i64, keys: i64) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            ..Default::default()
        },
        peers.iter().find(|p| p.store_id == leader).cloned(),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = size;
    r.stats_mut().approximate_keys = keys;
    r
}
fn steps() -> Vec<OpStep> {
    vec![
        AddPeer {
            to_store: 1,
            peer_id: 1,
            ..Default::default()
        }
        .into(),
        TransferLeader {
            from_store: 2,
            to_store: 1,
            ..Default::default()
        }
        .into(),
        RemovePeer {
            from_store: 2,
            peer_id: 2,
            ..Default::default()
        }
        .into(),
    ]
}
fn op(kind: OpKind, steps: Vec<OpStep>, clock: Arc<dyn Clock>) -> Operator {
    Operator::with_clock(
        "test",
        "test",
        1,
        Some(RegionEpoch {
            version: 3,
            conf_ver: 4,
        }),
        kind,
        50,
        steps,
        clock,
    )
}
fn clock() -> Arc<ManualClock> {
    Arc::new(ManualClock(Mutex::new(Instant::now())))
}
#[test]
fn task_024_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_024.json"
    ))
    .unwrap();
    let o = op(OpKind::REGION, steps(), clock());
    assert_eq!(o.status(), CREATED);
    assert_eq!(o.desc(), "test");
    assert_eq!(o.brief(), "test");
    assert_eq!(o.region_id(), 1);
    assert_eq!(o.region_epoch().unwrap().conf_ver, 4);
    assert_eq!(o.kind(), OpKind::REGION);
    assert_eq!(o.approximate_size(), 50);
    assert!(!o.has_started());
    assert!(!o.check_success());
    assert!(o.start());
    assert!(!o.start());
    for c in f["cases"].as_array().unwrap() {
        let stores: Vec<_> = c["stores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        let r = region(&stores, c["leader"].as_u64().unwrap(), 50, 50);
        let current = c["current"].as_u64().unwrap() as usize;
        assert_eq!(o.check(&r), o.step(current));
        assert_eq!(o.current_step(), current);
        assert_eq!(o.conf_ver_changed(&r), c["confver"].as_u64().unwrap());
        assert_eq!(o.check_success(), current == 3);
    }
    assert_eq!(o.status(), SUCCESS);
    assert!(o.check_success());
    assert!(o.step(3).is_none());
    assert_eq!(o.len(), 3);
    assert!(!o.is_empty());
    assert!(o.step_finish_time(0).is_some());
    let skip = op(OpKind::ADMIN, steps(), clock());
    skip.start();
    assert!(skip.check(&region(&[1], 1, 50, 50)).is_none());
    assert_eq!(skip.current_step(), 3);
    assert_eq!(skip.priority_level(), PriorityLevel::URGENT);
    assert_eq!(o.priority_level(), PriorityLevel::MEDIUM);
    o.set_priority_level(PriorityLevel::HIGH);
    assert_eq!(o.priority_level(), PriorityLevel::HIGH);
    assert!(!o.need_store_health_check());
    o.set_store_health_check(true);
    assert!(o.need_store_health_check());
    o.set_store_health_check(false);
    assert!(!o.need_store_health_check());
}
#[test]
fn task_024_terminal_and_clock_boundaries() {
    let c = clock();
    let o = op(OpKind::LEADER, steps(), c.clone());
    c.advance(Duration::from_secs(3));
    assert!(o.check_expired());
    assert!(!o.start());
    assert!(o.check(&region(&[1], 1, 50, 50)).is_none());
    assert_eq!(o.current_step(), 0);
    let empty = op(OpKind::REGION, vec![], clock());
    assert!(!empty.check_success());
    assert_eq!(empty.conf_ver_changed(&region(&[], 0, 0, 0)), 0);
    empty.start();
    assert!(empty.check_success());
    let before = op(OpKind::REGION, steps(), clock());
    assert!(before.check(&region(&[1], 1, 50, 50)).is_none());
    assert_eq!(before.status(), CREATED);
    before.start();
    assert!(before.check_success());
    for finish in [false, true] {
        let c = clock();
        let o = op(
            OpKind::LEADER,
            vec![
                TransferLeader {
                    from_store: 2,
                    to_store: 1,
                    ..Default::default()
                }
                .into(),
            ],
            c.clone(),
        );
        o.start();
        c.advance(Duration::from_secs(59));
        assert!(!o.check_timeout());
        c.advance(Duration::from_secs(1));
        let r = region(&[1, 2], if finish { 1 } else { 2 }, 1, 2);
        assert_eq!(o.check(&r).is_none(), finish);
        assert_eq!(o.status(), if finish { SUCCESS } else { TIMEOUT });
        assert_eq!(o.check_timeout(), !finish);
        assert!(o.check(&region(&[1], 1, 1, 2)).is_none());
    }
    for terminal in [CANCELED, REPLACED] {
        let o = op(OpKind::REGION, steps(), clock());
        o.start();
        assert!(if terminal == CANCELED {
            o.cancel()
        } else {
            o.replace()
        });
        assert!(!o.check_success());
        assert!(o.check(&region(&[1], 1, 0, 0)).is_none());
        assert_eq!(o.current_step(), 0);
    }
}
#[test]
fn task_024_influence_and_confver() {
    for (size, keys, cost) in [(1, 0, 0), (1, 2, 200), (21, 2, 1000)] {
        let o = op(
            OpKind::REGION,
            vec![
                AddPeer {
                    to_store: 3,
                    peer_id: 3,
                    ..Default::default()
                }
                .into(),
                RemovePeer {
                    from_store: 2,
                    peer_id: 2,
                    ..Default::default()
                }
                .into(),
            ],
            clock(),
        );
        let r = region(&[1, 2], 1, size, keys);
        let total = OpInfluence::new();
        o.total_influence(&total, None);
        assert!(total.snapshot().is_empty());
        o.total_influence(&total, Some(&r));
        assert_eq!(total.get_store_influence(3).region_size, size);
        assert_eq!(total.get_store_influence(3).region_count, 1);
        assert_eq!(
            total.get_store_influence(3).get_step_cost(Type::ADD_PEER),
            cost
        );
        assert_eq!(total.get_store_influence(2).region_count, -1);
        let second = OpInfluence::new();
        o.total_influence(&second, Some(&region(&[1], 1, 999, 999)));
        assert_eq!(total.snapshot(), second.snapshot());
        o.total_influence(&second, None);
        assert_eq!(second.get_store_influence(3).region_count, 2);
        o.start();
        let acknowledged = region(&[1, 2, 3], 1, size, keys);
        assert_eq!(o.conf_ver_changed(&acknowledged), 1); // include current before Check
        assert!(matches!(
            o.check(&acknowledged),
            Some(OpStep::RemovePeer(_))
        ));
        let remaining = OpInfluence::new();
        o.unfinished_influence(&remaining, &acknowledged);
        assert!(!remaining.snapshot().contains_key(&3));
        assert_eq!(remaining.get_store_influence(2).region_count, -1);
        let done = region(&[1, 3], 1, size, keys);
        assert!(o.check(&done).is_none());
        assert_eq!(o.conf_ver_changed(&done), 2);
        let none = OpInfluence::new();
        o.unfinished_influence(&none, &done);
        assert!(none.snapshot().is_empty());
    }
    // A later step already acknowledged must not contribute unfinished influence.
    let o = op(OpKind::REGION, steps(), clock());
    let i = OpInfluence::new();
    o.unfinished_influence(&i, &region(&[1], 1, 50, 50));
    assert!(i.snapshot().is_empty());
    assert_eq!(o.current_step(), 0);
}
#[test]
fn task_024_concurrent_heartbeats() {
    let o = op(OpKind::REGION, steps(), clock());
    o.start();
    let r = region(&[1], 1, 50, 50);
    std::thread::scope(|s| {
        for _ in 0..10 {
            let o = &o;
            let r = &r;
            s.spawn(move || {
                for _ in 0..100 {
                    assert!(o.check(r).is_none());
                    assert!(o.check_success());
                }
            });
        }
    });
    assert_eq!(o.current_step(), 3);
    assert_eq!(o.status(), SUCCESS);
    let times: Vec<_> = (0..3).map(|i| o.step_finish_time(i).unwrap()).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]));
    o.check(&r);
    assert_eq!(
        times,
        (0..3)
            .map(|i| o.step_finish_time(i).unwrap())
            .collect::<Vec<_>>()
    );
}
