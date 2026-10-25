// Copyright 2026 AsterSQL.

use crate::*;
use astersql_pd_core_constant::PriorityLevel;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn op(id: u64, priority: i32) -> Arc<Operator> {
    let op = Operator::new("queue", "", id, None, OpKind::REGION, 1, vec![]);
    op.set_priority_level(PriorityLevel::from_i32(priority));
    Arc::new(op)
}
fn pair(id: u64, kind: OpKind) -> Vec<Arc<Operator>> {
    let mut a = Operator::new("pair", "", id, None, kind, 1, vec![]);
    let b = Operator::new("pair", "", id + 1, None, kind, 1, vec![]);
    a.sync(&b);
    vec![Arc::new(a), Arc::new(b)]
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_033.json"
    ))
    .unwrap()
}
fn ids(ops: &[Arc<Operator>]) -> Vec<u64> {
    ops.iter().map(|op| op.region_id()).collect()
}

#[test]
fn task_033_behavior_cases() {
    let f = fixture();
    assert_eq!(f["evidence_kind"], "source_assertion");
    for c in f["cases"].as_array().unwrap() {
        let queue = RandBuckets::default();
        for (i, p) in c["priorities"].as_array().unwrap().iter().enumerate() {
            queue
                .put_operator(op(i as u64 + 1, p.as_i64().unwrap() as i32))
                .unwrap();
        }
        let got = queue.get_operator(c["random"].as_f64().unwrap()).unwrap();
        assert_eq!(
            got.as_ref().map(|ops| ops[0].region_id()),
            c["expected"].as_u64(),
            "{}",
            c["name"]
        );
        let mut remaining = queue.list_operator();
        if let Some(batch) = got {
            remaining.extend(batch);
        }
        let n = c["priorities"].as_array().unwrap().len();
        assert_eq!(remaining.len(), n);
        let expected = (1..=n as u64).collect::<BTreeSet<_>>();
        assert_eq!(
            ids(&remaining).into_iter().collect::<BTreeSet<_>>(),
            expected
        );
        while queue.get_operator(0.9999999999999999).unwrap().is_some() {}
        assert!(queue.list_operator().is_empty());
    }
    // Listing is bucket order and FIFO; a list owns independent references.
    let queue = RandBuckets::default();
    for (id, p) in [(1, 1), (2, 2), (3, 0), (4, 3), (5, 1)] {
        queue.put_operator(op(id, p)).unwrap();
    }
    let listed = queue.list_operator();
    assert_eq!(ids(&listed), [3, 1, 5, 2, 4]);
    for expected in [3, 1, 5, 2, 4] {
        assert_eq!(ids(&queue.get_operator(0.0).unwrap().unwrap()), [expected]);
    }
    assert_eq!(listed.len(), 5);
    println!("task033: 10 weighted fixture cases + listing/FIFO/drain scenario passed");
}

#[test]
fn task_033_merge_and_faults() {
    let q = RandBuckets::default();
    for invalid in [
        vec![],
        vec![op(1, 0)],
        vec![op(1, 0), op(2, 0)],
        vec![op(1, 0), op(2, 0), op(3, 0)],
    ] {
        assert!(!q.put_merge_operators(&invalid).unwrap());
        assert!(q.list_operator().is_empty());
    }
    for kind in [OpKind::MERGE, OpKind::AFFINITY] {
        let ops = pair(10, kind);
        assert!(!q.put_merge_operators(&[ops[0].clone(), op(30, 0)]).unwrap());
        // Go selects the first operator's bucket even when priorities differ.
        ops[1].set_priority_level(PriorityLevel::URGENT);
        assert!(q.put_merge_operators(&ops).unwrap());
        q.put_operator(op(20, 1)).unwrap();
        assert_eq!(ids(&q.get_operator(0.0).unwrap().unwrap()), [10, 11]);
        assert_eq!(ids(&q.get_operator(0.0).unwrap().unwrap()), [20]);
    }
    assert_eq!(
        q.put_operator(op(1, -1)),
        Err(QueueError::InvalidPriority(-1))
    );
    assert_eq!(
        q.put_operator(op(1, 4)),
        Err(QueueError::InvalidPriority(4))
    );
    q.put_operator(op(1, 0)).unwrap();
    for r in [f64::NAN, f64::INFINITY, -0.1, 1.0] {
        assert!(matches!(q.get_operator(r), Err(QueueError::InvalidRandom)));
        assert_eq!(q.list_operator().len(), 1);
    }
    assert_eq!(ids(&q.get_operator(0.0).unwrap().unwrap()), [1]);
    q.put_operator(pair(50, OpKind::MERGE)[0].clone()).unwrap();
    assert!(matches!(
        q.get_operator(0.0),
        Err(QueueError::UnpairedOperator)
    ));
    assert_eq!(ids(&q.list_operator()), [50]);
    q.put_operator(pair(50, OpKind::MERGE)[1].clone()).unwrap();
    assert_eq!(ids(&q.get_operator(0.0).unwrap().unwrap()), [50, 51]);
}

#[test]
fn task_033_counts() {
    let s = WaitingOperatorStatus::default();
    let mut got = vec![s.get_count("a")];
    s.inc_count("a");
    got.push(s.get_count("a"));
    s.inc_count("a");
    got.push(s.get_count("a"));
    s.dec_count("a");
    got.push(s.get_count("a"));
    s.dec_count("a");
    got.push(s.get_count("a"));
    s.dec_count("a");
    got.push(s.get_count("a"));
    assert_eq!(serde_json::json!(got), fixture()["counts"]);
    assert_eq!(s.get_count("b"), 0);
    s.inc_count("a");
    assert_eq!(s.get_count("a"), 0);
}

#[test]
fn task_033_heap() {
    let q = ConcurrentHeapOpQueue::default();
    assert!(q.pop().is_none());
    assert_eq!(q.len(), 0);
    let base = Instant::now();
    let f = fixture();
    for row in f["heap"]["inputs"].as_array().unwrap() {
        q.push(OperatorWithTime {
            op: op(row[0].as_u64().unwrap(), 0),
            deadline: base + Duration::from_nanos(row[1].as_u64().unwrap()),
        });
    }
    assert_eq!(q.len(), 4);
    let mut deadlines = vec![];
    let mut seen = BTreeSet::new();
    while let Some(item) = q.pop() {
        deadlines.push(item.deadline.duration_since(base).as_nanos() as u64);
        assert!(seen.insert(item.op.region_id()));
    }
    assert_eq!(
        serde_json::json!(deadlines),
        f["heap"]["expected_deadlines"]
    );
    assert_eq!(seen, [1, 2, 3, 4].into_iter().collect());
    assert!(q.is_empty());
    // pop returns even a future deadline; due-time dispatch belongs to the caller.
    q.push(OperatorWithTime {
        op: op(8, 0),
        deadline: base + Duration::from_secs(100),
    });
    assert_eq!(q.pop().unwrap().op.region_id(), 8);
}

#[test]
fn task_033_concurrent_push_pop() {
    let q = RandBuckets::default();
    let heap = ConcurrentHeapOpQueue::default();
    let counts = WaitingOperatorStatus::default();
    let done = AtomicUsize::new(0);
    let seen = Mutex::new(Vec::new());
    let heap_seen = Mutex::new(Vec::new());
    let start = Barrier::new(8);
    let base = Instant::now();
    std::thread::scope(|scope| {
        for producer in 0..4 {
            let (q, heap, counts, done, start) = (&q, &heap, &counts, &done, &start);
            scope.spawn(move || {
                start.wait();
                for i in 0..100 {
                    let id = (producer * 200 + i * 2) as u64;
                    let batch = pair(
                        id,
                        if i % 2 == 0 {
                            OpKind::MERGE
                        } else {
                            OpKind::AFFINITY
                        },
                    );
                    for op in &batch {
                        op.set_priority_level(PriorityLevel::from_i32(producer));
                    }
                    q.put_merge_operators(&batch).unwrap();
                    for op in batch {
                        heap.push(OperatorWithTime {
                            op,
                            deadline: base + Duration::from_nanos(id % 7),
                        });
                        counts.inc_count("shared");
                    }
                }
                done.fetch_add(1, Ordering::Release);
            });
        }
        for _ in 0..4 {
            let (q, heap, counts, done, start, seen, heap_seen) =
                (&q, &heap, &counts, &done, &start, &seen, &heap_seen);
            scope.spawn(move || {
                start.wait();
                loop {
                    if let Some(batch) = q.get_operator(0.5).unwrap() {
                        assert_eq!(batch.len(), 2);
                        assert_eq!(batch[1].region_id(), batch[0].region_id() + 1);
                        seen.lock().unwrap().extend(ids(&batch));
                    }
                    if let Some(item) = heap.pop() {
                        heap_seen.lock().unwrap().push(item.op.region_id());
                        counts.dec_count("shared");
                    }
                    if done.load(Ordering::Acquire) == 4
                        && q.list_operator().is_empty()
                        && heap.is_empty()
                    {
                        break;
                    }
                    std::thread::yield_now();
                }
            });
        }
    });
    let expected = (0..800).collect::<Vec<u64>>();
    for output in [&seen, &heap_seen] {
        let mut output = output.lock().unwrap();
        output.sort_unstable();
        assert_eq!(*output, expected);
    }
    assert_eq!(counts.get_count("shared"), 0);
    assert!(q.list_operator().is_empty());
    assert!(heap.is_empty());
    println!(
        "task033: 400 merge/affinity batches, 800 heap entries, 4 producers + 4 consumers passed"
    );
}
