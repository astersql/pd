// Copyright 2026 AsterSQL.
use crate::{Builder, List};
fn overlap() -> List<i32> {
    let mut b = Builder::new();
    b.set_compare_func(i32::cmp);
    for (s, e, v) in [
        ("a", "e", 4),
        ("c", "d", 1),
        ("f", "h", 3),
        ("g", "i", 2),
        ("", "b", 2),
        ("", "b", 3),
    ] {
        b.add_item(s.as_bytes(), e.as_bytes(), v);
    }
    b.build()
}
#[test]
fn range_queries_go_key_and_range() {
    let l = overlap();
    for (k, i) in [("", 0), ("a", 1), ("abc", 1), ("efg", 5), ("z", 9)] {
        assert_eq!(
            l.get_data_by_key(k.as_bytes()),
            (i, Some(l.get(i as usize).1))
        );
    }
    for (s, e, i) in [
        ("", "", -1),
        ("", "a", 0),
        ("", "aa", -1),
        ("b", "c", 2),
        ("ef", "ex", 5),
        ("e", "", -1),
    ] {
        let expected = if i < 0 {
            None
        } else {
            Some(l.get(i as usize).1)
        };
        assert_eq!(
            l.get_data(s.as_bytes(), e.as_bytes()),
            (i, expected),
            "{s}..{e}"
        );
    }
}
#[test]
fn range_queries_go_splits() {
    let l = overlap();
    for (s, e, a, b) in [("", "", 1, 10), ("a", "c", 2, 3), ("cc", "fx", 4, 7)] {
        let expected: Vec<_> = (a..b).map(|i| l.get(i).0).collect();
        assert_eq!(l.get_split_keys(s.as_bytes(), e.as_bytes()), expected);
        assert_eq!(
            l.get_data_by_range(s.as_bytes(), e.as_bytes()),
            (None, false)
        );
    }
}
#[test]
fn range_queries_empty_and_unbounded() {
    let l = Builder::<i32>::new().build();
    assert_eq!(l.get_data_by_key(b"a"), (-1, None));
    assert_eq!(l.get_data(b"a", b"b"), (-1, None));
    assert!(l.get_split_keys(b"", b"foo").is_empty());
    assert_eq!(l.get_data_by_range(b"", b"foo"), (None, true));
    let mut b = Builder::new();
    b.add_item(b"", b"", 1);
    let l = b.build();
    assert_eq!(l.get_data_by_key(b"foo"), (0, Some([1].as_slice())));
    assert_eq!(l.get_data(b"a", b"b"), (0, Some([1].as_slice())));
    assert!(l.get_split_keys(b"", b"foo").is_empty());
    assert_eq!(
        l.get_data_by_range(b"", b"foo"),
        (Some([1].as_slice()), true)
    );
}
#[test]
fn range_queries_boundaries_and_gaps() {
    let mut b = Builder::new();
    b.add_item(b"b", b"d", 7);
    let l = b.build();
    for (s, e, data, covered) in [
        ("a", "b", None, true),
        ("a", "c", None, false),
        ("b", "d", Some([7].as_slice()), true),
        ("b", "e", None, false),
        ("d", "", Some([].as_slice()), true),
        ("b", "b", Some([7].as_slice()), true),
        ("c", "a", Some([7].as_slice()), true),
    ] {
        assert_eq!(
            l.get_data_by_range(s.as_bytes(), e.as_bytes()),
            (data, covered),
            "{s}..{e}"
        );
    }
    assert_eq!(l.get_data_by_key(b"a"), (-1, None));
    assert_eq!(l.get_data_by_key(b"d"), (1, Some([].as_slice())));
    assert_eq!(l.get_data(b"a", b"b"), (-1, None));
    assert_eq!(l.get_data(b"b", b"d"), (0, Some([7].as_slice())));
    assert_eq!(l.get_data(b"d", b""), (1, Some([].as_slice())));
    assert_eq!(l.get_split_keys(b"a", b"d"), vec![b"b".as_slice()]);
    assert!(l.get_split_keys(b"b", b"d").is_empty());
    assert!(l.get_split_keys(b"c", b"a").is_empty());
}
