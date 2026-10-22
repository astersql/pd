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
