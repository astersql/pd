// Copyright 2026 AsterSQL.

use crate::{TopNItem, TtlTopN};
use std::{collections::BTreeSet, time::Duration};

#[derive(Debug)]
struct Item(u64, [i64; 4]);
impl TopNItem for Item {
    fn id(&self) -> u64 {
        self.0
    }
    fn less(&self, k: usize, other: &Self) -> bool {
        self.1[k] < other.1[k]
    }
}
fn time(ms: u64) -> Duration {
    Duration::from_millis(ms)
}
fn item(id: u64) -> Item {
    Item(id, [-(id as i64), id as i64, (id % 7) as i64, 5])
}
fn verify(tn: &TtlTopN<Item>, n: usize, expected: BTreeSet<u64>) {
    let all = tn.items().get_all();
    assert_eq!(tn.items().len(), expected.len());
    assert_eq!(all.iter().map(|v| v.0).collect::<BTreeSet<_>>(), expected);
    for k in 0..4 {
        let top = tn.items().get_all_top_n(k);
        let mut actual: Vec<_> = top.iter().map(|v| v.1[k]).collect();
        actual.sort_unstable_by(|a, b| b.cmp(a));
        let mut sorted: Vec<_> = all.iter().map(|v| v.1[k]).collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(actual, sorted[..n.min(sorted.len())]);
        assert_eq!(
            tn.items().get_top_n_min(k).map(|v| v.1[k]),
            actual.last().copied()
        );
        for v in top {
            assert!(std::sync::Arc::ptr_eq(&v, &tn.items().get(v.0).unwrap()));
        }
    }
}
#[test]
fn state_task_03_topn_ttl_boundary_and_explicit_cleanup() {
    let mut tn = TtlTopN::new(4, 2, time(10));
    assert!(!tn.put(item(7), time(5)));
    assert!(
        tn.remove_expired(time(15) - Duration::from_nanos(1))
            .is_empty()
    );
    verify(&tn, 2, BTreeSet::from([7]));
    assert_eq!(tn.remove_expired(time(15)), vec![7; 4]);
    verify(&tn, 2, BTreeSet::new());
    assert!(tn.remove_expired(time(20)).is_empty());
    let mut zero = TtlTopN::new(4, 1, Duration::ZERO);
    zero.put(item(1), time(0));
    assert_eq!(zero.items().len(), 1);
    assert_eq!(zero.remove_expired(time(0)), vec![1; 4]);
}
#[test]
fn state_task_03_topn_ttl_renewal_never_deletes_latest_version() {
    let mut tn = TtlTopN::new(4, 1, time(10));
    tn.put(item(1), time(0));
    tn.put(item(2), time(0));
    let old = tn.items().get(1).unwrap();
    // Renewal after the old deadline, before explicit cleanup, is still update.
    assert!(tn.put(Item(1, [100; 4]), time(10)));
    assert_eq!(tn.remove_expired(time(10)), vec![2; 4]);
    verify(&tn, 1, BTreeSet::from([1]));
    assert_eq!(old.1[0], -1);
    assert_eq!(tn.items().get(1).unwrap().1, [100; 4]);
    for t in 11..1000 {
        assert!(tn.put(Item(1, [t as i64; 4]), time(t)));
    }
    assert!(tn.remove_expired(time(1008)).is_empty());
    assert_eq!(tn.remove_expired(time(1009)), vec![1; 4]);
    verify(&tn, 1, BTreeSet::new());
}
#[test]
fn state_task_03_topn_ttl_remove_reinsert_and_partial_batch() {
    let mut tn = TtlTopN::new(4, 2, time(10));
    for id in 0..8 {
        tn.put(item(id), time(id));
    }
    assert!(tn.remove(0).is_some());
    assert!(tn.remove(0).is_none());
    assert!(!tn.put(Item(0, [99; 4]), time(8)));
    assert_eq!(
        tn.remove_expired(time(13)),
        [vec![1; 4], vec![2; 4], vec![3; 4]].concat()
    );
    verify(&tn, 2, BTreeSet::from([0, 4, 5, 6, 7]));
    assert!(!tn.put(item(1), time(14)));
    assert_eq!(tn.remove_expired(time(18)).len(), 5 * 4);
    verify(&tn, 2, BTreeSet::from([1]));
    assert_eq!(tn.remove_expired(time(24)), vec![1; 4]);
    verify(&tn, 2, BTreeSet::new());
}
#[test]
fn state_task_03_topn_ttl_go_test_ttl_contract() {
    let mut tn = TtlTopN::new(4, 50, time(900));
    for id in 0..1000 {
        assert!(!tn.put(item(id), time(0)));
    }
    assert!(tn.put(Item(0, [100; 4]), time(900)));
    assert_eq!(tn.remove_expired(time(900)).len(), 999 * 4);
    verify(&tn, 50, BTreeSet::from([0]));
    for id in (3..1000).step_by(3) {
        let mut it = item(id);
        it.1[0] += 100;
        assert!(!tn.put(it, time(900)));
    }
    verify(&tn, 50, (0..1000).step_by(3).collect());
    assert_eq!(tn.items().get_top_n_min(0).unwrap().1[0], 100 - 49 * 3);
    assert_eq!(tn.remove_expired(time(1800)).len(), 334 * 4);
    verify(&tn, 50, BTreeSet::new());
}
