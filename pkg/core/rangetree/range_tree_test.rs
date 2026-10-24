// Copyright 2026 AsterSQL.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Item(Vec<u8>, Vec<u8>);
impl RangeItem for Item {
    fn start_key(&self) -> &[u8] {
        &self.0
    }
    fn end_key(&self) -> &[u8] {
        &self.1
    }
}
fn item(start: &[u8], end: &[u8]) -> Item {
    Item(start.into(), end.into())
}

#[test]
fn find_len_and_index() {
    let empty = RangeTree::<Item>::new(2);
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.find(&item(b"", b"")), None);
    assert_eq!(empty.get_at(0), None);
    assert_eq!(empty.get_with_index(&item(b"a", b"")), (None, 0));
    let items = vec![item(b"b", b"d"), item(b"d", b"f"), item(b"h", b"")];
    let tree = RangeTree::from_items(2, items.iter().rev().cloned());
    assert_eq!(tree.len(), 3);
    for (key, expected) in [
        (b"a".as_slice(), None),
        (b"b", Some(0)),
        (b"c", Some(0)),
        (b"d", Some(1)),
        (b"e", Some(1)),
        (b"f", None),
        (b"g", None),
        (b"h", Some(2)),
        (b"z", Some(2)),
        (&[255], Some(2)),
    ] {
        assert_eq!(
            tree.find(&item(key, b"ignored")),
            expected.map(|i| &items[i])
        );
    }
    for (i, expected) in items.iter().enumerate() {
        assert_eq!(tree.get_at(i as isize), Some(expected));
        assert_eq!(
            tree.get_with_index(&item(&expected.0, b"different")),
            (Some(expected), i)
        );
    }
    for (key, rank) in [(b"a", 0), (b"c", 1), (b"g", 2), (b"z", 3)] {
        assert_eq!(tree.get_with_index(&item(key, b"")), (None, rank));
    }
    assert_eq!(tree.get_at(-1), None);
    assert_eq!(tree.get_at(3), None);
    assert_eq!(tree.get_at(isize::MAX), None);
    let all = RangeTree::from_items(2, [item(b"", b"")]);
    assert!(all.find(&item(b"", b"")).is_some());
    assert!(all.find(&item(&[255], b"")).is_some());
    let many = RangeTree::from_items(2, (0u8..100).rev().map(|i| item(&[i * 2], &[i * 2 + 1])));
    for i in 0u8..100 {
        assert_eq!(many.get_at(i as isize).unwrap().0, [i * 2]);
        assert_eq!(many.get_with_index(&item(&[i * 2], b"")).1, i as usize);
        assert!(many.find(&item(&[i * 2], b"")).is_some());
        assert!(many.find(&item(&[i * 2 + 1], b"")).is_none());
    }
}

#[test]
fn overlap_query() {
    let empty = RangeTree::<Item>::new(2);
    assert!(empty.get_overlaps(&item(b"", b"")).is_empty());
    let items = vec![
        item(b"b", b"d"),
        item(b"d", b"f"),
        item(b"h", b"j"),
        item(b"m", b""),
    ];
    let tree = RangeTree::from_items(2, items.iter().rev().cloned());
    for (start, end, indices) in [
        (b"".as_slice(), b"b".as_slice(), vec![]),
        (b"a", b"c", vec![0]),
        (b"c", b"e", vec![0, 1]),
        (b"d", b"h", vec![1]),
        (b"f", b"h", vec![]),
        (b"g", b"n", vec![2, 3]),
        (b"c", b"m", vec![0, 1, 2]),
        (b"", b"", vec![0, 1, 2, 3]),
        (b"e", b"", vec![1, 2, 3]),
        (b"z", b"", vec![3]),
        // Preserve Go scan semantics for equal/reversed bounds as well.
        (b"c", b"c", vec![0]),
        (b"d", b"d", vec![]),
        (b"e", b"c", vec![]),
    ] {
        let expected: Vec<_> = indices.into_iter().map(|i| items[i].clone()).collect();
        assert_eq!(
            tree.get_overlaps(&item(start, end)),
            expected,
            "{start:?}..{end:?}"
        );
    }
}

fn debris(start: &[u8], end: &[u8], old: &Item) -> Vec<Item> {
    let mut pieces = Vec::new();
    if old.start_key() < start {
        pieces.push(item(old.start_key(), start));
    }
    if !end.is_empty() && (old.end_key().is_empty() || end < old.end_key()) {
        pieces.push(item(end, old.end_key()));
    }
    pieces
}

fn contents(tree: &RangeTree<Item>) -> Vec<Item> {
    (0..tree.len())
        .map(|i| tree.get_at(i as isize).unwrap().clone())
        .collect()
}

#[test]
fn update_with_debris() {
    let mut tree = RangeTree::with_factory(2, debris);
    assert!(tree.update(item(b"002", b"100")).is_empty());
    assert!(tree.update(item(b"100", b"200")).is_empty());
    for (new, overlaps, expected) in [
        (
            item(b"010", b"090"),
            vec![item(b"002", b"100")],
            vec![
                item(b"002", b"010"),
                item(b"010", b"090"),
                item(b"090", b"100"),
                item(b"100", b"200"),
            ],
        ),
        (
            item(b"001", b"080"),
            vec![item(b"002", b"010"), item(b"010", b"090")],
            vec![
                item(b"001", b"080"),
                item(b"080", b"090"),
                item(b"090", b"100"),
                item(b"100", b"200"),
            ],
        ),
        (
            item(b"001", b"120"),
            vec![
                item(b"001", b"080"),
                item(b"080", b"090"),
                item(b"090", b"100"),
                item(b"100", b"200"),
            ],
            vec![item(b"001", b"120"), item(b"120", b"200")],
        ),
    ] {
        assert_eq!(tree.update(new), overlaps);
        assert_eq!(contents(&tree), expected);
    }
}

#[test]
fn update_with_debris_unbounded_and_many_overlaps() {
    let mut tree = RangeTree::with_factory(2, debris);
    tree.update(item(b"", b""));
    assert_eq!(tree.update(item(b"b", b"d")), vec![item(b"", b"")]);
    assert_eq!(
        contents(&tree),
        vec![item(b"", b"b"), item(b"b", b"d"), item(b"d", b"")]
    );
    assert_eq!(
        tree.update(item(b"c", b"")),
        vec![item(b"b", b"d"), item(b"d", b"")]
    );
    assert_eq!(
        contents(&tree),
        vec![item(b"", b"b"), item(b"b", b"c"), item(b"c", b"")]
    );
    let previous = contents(&tree);
    assert_eq!(tree.update(item(b"", b"")), previous);
    assert_eq!(contents(&tree), vec![item(b"", b"")]);

    let mut tree = RangeTree::with_factory(2, debris);
    for i in 1..100u8 {
        tree.update(item(&[i], &[i + 1]));
    }
    let previous = contents(&tree);
    assert_eq!(tree.update(item(&[0], &[101])), previous);
    assert_eq!(contents(&tree), vec![item(&[0], &[101])]);
}

#[test]
fn update_with_debris_filters_invalid_children_and_inserts_new_last() {
    let mut tree = RangeTree::with_factory(2, |_: &[u8], _: &[u8], _: &Item| {
        vec![
            item(b"a", b"a"),
            item(b"z", b"a"),
            item(b"", b""),
            item(b"", b"b"),
            item(b"z", b""),
            item(b"b", b"c"),
        ]
    });
    tree.update(item(b"a", b"y"));
    assert_eq!(tree.update(item(b"b", b"x")), vec![item(b"a", b"y")]);
    assert_eq!(
        contents(&tree),
        vec![item(b"", b"b"), item(b"b", b"x"), item(b"z", b"")]
    );
}

#[test]
fn remove_scan_and_adjacent() {
    let mut empty = RangeTree::<Item>::new(2);
    assert_eq!(empty.remove(&item(b"a", b"")), None);
    assert_eq!(empty.get_adjacent_item(&item(b"a", b"")), (None, None));
    empty.scan_range(&item(b"", b""), |_| panic!("empty scan"));
    let items = vec![item(b"b", b"d"), item(b"d", b"f"), item(b"h", b"j")];
    let mut tree = RangeTree::from_items(2, items.clone());
    for (key, first, prev, next) in [
        (b"".as_slice(), 0, None, Some(0)),
        (b"b", 0, None, Some(1)),
        (b"c", 0, Some(0), Some(1)),
        (b"d", 1, Some(0), Some(2)),
        (b"f", 2, Some(1), Some(2)),
        (b"g", 2, Some(1), Some(2)),
        (b"h", 2, Some(1), None),
        (b"i", 2, Some(2), None),
        (b"j", 3, Some(2), None),
        (b"z", 3, Some(2), None),
    ] {
        let query = item(key, b"ignored");
        let mut scanned = Vec::new();
        tree.scan_range(&query, |i| {
            scanned.push(i.clone());
            true
        });
        assert_eq!(scanned, items[first..], "scan {key:?}");
        assert_eq!(
            tree.get_adjacent_item(&query),
            (prev.map(|i| &items[i]), next.map(|i| &items[i])),
            "adjacent {key:?}"
        );
    }
    let mut scanned = Vec::new();
    tree.scan_range(&item(b"c", b""), |i| {
        scanned.push(i.clone());
        false
    });
    assert_eq!(scanned, items[..1]);
    assert_eq!(tree.remove(&item(b"c", b"d")), None);
    assert_eq!(contents(&tree), items);
    assert_eq!(
        tree.remove(&item(b"d", b"different")),
        Some(items[1].clone())
    );
    assert_eq!(tree.remove(&items[1]), None);
    assert_eq!(contents(&tree), vec![items[0].clone(), items[2].clone()]);
    assert_eq!(
        tree.get_adjacent_item(&items[1]),
        (Some(&items[0]), Some(&items[2]))
    );
    assert_eq!(tree.remove(&items[0]), Some(items[0].clone()));
    assert_eq!(tree.remove(&items[2]), Some(items[2].clone()));
    assert!(tree.is_empty());
    let all = RangeTree::from_items(2, [item(b"", b"")]);
    let mut scanned = Vec::new();
    all.scan_range(&item(&[255], b""), |i| {
        scanned.push(i.clone());
        true
    });
    assert_eq!(scanned, vec![item(b"", b"")]);
    assert_eq!(all.get_adjacent_item(&item(b"", b"x")), (None, None));
}

#[test]
fn owned_range_tree_can_move_to_host_worker() {
    fn assert_send<T: Send>() {}
    assert_send::<RangeTree<Item>>();
    let mut tree = RangeTree::with_factory(2, debris);
    tree.update(item(b"10", b"30"));
    let result = std::thread::spawn(move || {
        tree.update(item(b"20", b"30"));
        tree.len()
    })
    .join()
    .unwrap();
    assert_eq!(result, 2);
}
