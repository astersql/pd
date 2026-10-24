// Copyright 2026 AsterSQL.

use std::{collections::BTreeSet, sync::Arc};

use crate::{TopN, TopNItem};

#[derive(Debug, PartialEq)]
struct Item {
    id: u64,
    values: [i64; 4],
}
impl TopNItem for Item {
    fn id(&self) -> u64 {
        self.id
    }
    fn less(&self, k: usize, than: &Self) -> bool {
        self.values[k] < than.values[k]
    }
}
fn item(id: u64, values: [i64; 4]) -> Item {
    Item { id, values }
}
fn ids(items: Vec<Arc<Item>>) -> BTreeSet<u64> {
    items.iter().map(|v| v.id).collect()
}
fn verify(tn: &TopN<Item>, n: usize) {
    let all = tn.get_all();
    assert_eq!(all.len(), tn.len());
    assert_eq!(ids(all.clone()).len(), all.len());
    for k in 0..4 {
        let top = tn.get_all_top_n(k);
        assert_eq!(top.len(), n.min(all.len()));
        let mut expected: Vec<_> = all.iter().map(|x| x.values[k]).collect();
        expected.sort_unstable_by(|a, b| b.cmp(a));
        let mut actual: Vec<_> = top.iter().map(|x| x.values[k]).collect();
        actual.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(actual, expected[..actual.len()], "dimension {k}");
        assert_eq!(
            tn.get_top_n_min(k).map(|v| v.values[k]),
            actual.last().copied()
        );
        for v in top {
            assert!(Arc::ptr_eq(&v, &tn.get(v.id).unwrap()));
        }
    }
}

#[test]
fn state_task_02_topn_cross_rank_updates_and_removals() {
    let mut tn = TopN::new(4, 2);
    let inputs = [[9, 1, 5, 2], [6, 8, 4, 3], [2, 3, 9, 4], [1, 7, 1, 9]];
    for (id, values) in inputs.into_iter().enumerate() {
        assert!(!tn.put(item(id as u64, values)));
        assert_eq!(tn.len(), id + 1);
        verify(&tn, 2);
    }
    assert_eq!(ids(tn.get_all_top_n(0)), BTreeSet::from([0, 1]));
    assert_eq!(ids(tn.get_all_top_n(1)), BTreeSet::from([1, 3]));
    let held = tn.get(0).unwrap();
    assert!(tn.put(item(0, [0, 10, 10, 0])));
    assert_eq!(held.values, [9, 1, 5, 2]);
    assert_eq!(ids(tn.get_all_top_n(0)), BTreeSet::from([1, 2]));
    assert_eq!(ids(tn.get_all_top_n(1)), BTreeSet::from([0, 1]));
    verify(&tn, 2);
    assert_eq!(tn.remove(0).unwrap().values, [0, 10, 10, 0]);
    verify(&tn, 2);
    for id in [1, 3, 2] {
        assert_eq!(tn.remove(id).unwrap().id, id);
        verify(&tn, 2);
    }
    assert!(tn.is_empty());
    assert!(tn.remove(0).is_none());
    assert!(tn.get(u64::MAX).is_none());
}

#[test]
fn state_task_02_topn_go_put_remove_contract() {
    const TOTAL: usize = 10_000;
    const N: usize = 50;
    // Fixed permutations of the exact Go TestPut/TestRemove input range/formula.
    // All multipliers are coprime to TOTAL; dimension zero keeps ID's -x mapping.
    let values =
        |id: usize, offset: i64| [1, 3, 7, 11].map(|m| -(((id * m) % TOTAL) as i64) + offset);
    let mut tn = TopN::new(4, N);
    for offset in [1, 0] {
        for i in 0..TOTAL {
            let id = (i * 17) % TOTAL;
            assert_eq!(tn.put(item(id as u64, values(id, offset))), offset == 0);
        }
        assert_eq!(tn.len(), TOTAL);
        verify(&tn, N);
    }
    assert_eq!(tn.get_top_n_min(0).unwrap().values[0], -49);
    assert_eq!(ids(tn.get_all_top_n(0)), (0..50).collect());
    for id in 0..TOTAL {
        assert_eq!(tn.get(id as u64).unwrap().values, values(id, 0));
    }
    for id in 0..TOTAL {
        if id % 3 != 0 {
            assert_eq!(tn.remove(id as u64).unwrap().values, values(id, 0));
        }
    }
    for id in 0..TOTAL {
        if id % 3 != 0 {
            assert!(tn.remove(id as u64).is_none());
        }
    }
    assert_eq!(tn.len(), 3334);
    assert_eq!(tn.get_top_n_min(0).unwrap().id, 147);
    verify(&tn, N);
}

#[test]
fn state_task_02_topn_equal_loads_n_boundaries_and_max_id() {
    for n in [1, 2, 20] {
        let mut tn = TopN::new(4, n);
        verify(&tn, n);
        for id in [0, 1, 2, u64::MAX] {
            assert!(!tn.put(item(id, [5; 4])));
            verify(&tn, n);
        }
        assert!(tn.put(item(1, [5; 4])));
        verify(&tn, n);
        // Boundary ties promise values/cardinality, never a specific winner ID.
        for id in [2, u64::MAX, 0, 1] {
            assert!(tn.remove(id).is_some());
            verify(&tn, n);
        }
    }
}

#[test]
fn state_task_02_topn_repeated_heap_repairs() {
    let mut tn = TopN::new(4, 7);
    let mut present = BTreeSet::new();
    let mut seed = 42u64;
    for _ in 0..2000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let id = (seed >> 32) % 40;
        if seed % 5 == 0 {
            assert_eq!(tn.remove(id).is_some(), present.remove(&id));
        } else {
            assert_eq!(
                tn.put(item(
                    id,
                    [seed as i64 % 101, (seed >> 8) as i64 % 19, -(id as i64), 7]
                )),
                !present.insert(id)
            );
        }
        assert_eq!(ids(tn.get_all()), present);
        verify(&tn, 7);
    }
}

#[test]
fn state_task_02_topn_invalid_arguments() {
    for (k, n) in [(0, 1), (1, 0), (0, 0)] {
        assert!(std::panic::catch_unwind(|| TopN::<Item>::new(k, n)).is_err());
    }
    let tn = TopN::<Item>::new(4, 2);
    assert!(std::panic::catch_unwind(|| tn.get_all_top_n(4)).is_err());
    assert!(std::panic::catch_unwind(|| tn.get_top_n_min(usize::MAX)).is_err());
}
