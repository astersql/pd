// Copyright 2026 AsterSQL.
use super::Builder;
#[test]
fn range_build_go_overlap_comparator() {
    let mut b = Builder::new();
    b.set_compare_func(i32::cmp);
    for (s, e, d) in [
        ("a", "e", 4),
        ("c", "d", 1),
        ("f", "h", 3),
        ("g", "i", 2),
        ("", "b", 2),
        ("", "b", 3),
    ] {
        b.add_item(s.as_bytes(), e.as_bytes(), d);
    }
    let l = b.build();
    let expected = [
        ("", vec![2, 3]),
        ("a", vec![2, 3, 4]),
        ("b", vec![4]),
        ("c", vec![1, 4]),
        ("d", vec![4]),
        ("e", vec![]),
        ("f", vec![3]),
        ("g", vec![2, 3]),
        ("h", vec![2]),
        ("i", vec![]),
    ];
    assert_eq!(l.len(), expected.len());
    for (i, (k, v)) in expected.iter().enumerate() {
        assert_eq!(l.get(i), (k.as_bytes(), v.as_slice()));
    }
}
#[test]
fn range_build_empty_and_unbounded() {
    let mut b = Builder::new();
    assert!(b.build().is_empty());
    b.add_item(b"", b"", 1);
    let l = b.build();
    assert_eq!(l.len(), 1);
    assert_eq!(l.get(0), (&b""[..], &[1][..]));
}
#[test]
fn range_build_shared_events_duplicates_and_snapshot() {
    let mut b = Builder::new();
    b.add_item(b"a", b"c", 1);
    b.add_item(b"a", b"b", 1);
    b.add_item(b"b", b"d", 2);
    let l = b.build();
    assert_eq!(l.len(), 4);
    assert_eq!(l.get(0), (&b"a"[..], &[1, 1][..]));
    assert_eq!(l.get(1), (&b"b"[..], &[1, 2][..]));
    assert_eq!(l.get(2), (&b"c"[..], &[2][..]));
    assert!(l.get(3).1.is_empty());
    assert_eq!(b.build(), l);
    b.add_item(b"e", b"", 3);
    assert_eq!(l.len(), 4);
}
#[test]
fn range_build_comparator_equal_and_reverse() {
    let mut b = Builder::new();
    b.set_compare_func(|a: &i32, b: &i32| b.cmp(a));
    for n in [2, 1, 3, 2] {
        b.add_item(b"", b"", n);
    }
    assert_eq!(b.build().get(0).1, &[3, 2, 2, 1]);
}

#[test]
fn range_build_equal_comparator_preserves_event_order() {
    let mut b = Builder::new();
    b.set_compare_func(|a: &(i32, char), b: &(i32, char)| a.0.cmp(&b.0));
    for item in [(2, 'a'), (1, 'b'), (2, 'c')] {
        b.add_item(b"", b"", item);
    }
    assert_eq!(b.build().get(0).1, &[(1, 'b'), (2, 'a'), (2, 'c')]);
}

#[test]
fn range_build_large_event_sort_preserves_permutation() {
    for n in [13, 49, 50, 127, 1024] {
        for pattern in 0..5 {
            let mut points: Vec<_> = (0..n)
                .map(|i| {
                    let key = match pattern {
                        0 => i,
                        1 => n - i,
                        2 => i % 7,
                        3 => (i * 7919) % n,
                        _ => i.min(n - i),
                    };
                    ((key as u32).to_be_bytes().to_vec(), true, i)
                })
                .collect();
            crate::event_sort::sort(&mut points);
            assert!(points.windows(2).all(|p| p[0].0 <= p[1].0));
            let mut ids: Vec<_> = points.iter().map(|p| p.2).collect();
            ids.sort();
            assert_eq!(ids, (0..n).collect::<Vec<_>>());
        }
    }
}
