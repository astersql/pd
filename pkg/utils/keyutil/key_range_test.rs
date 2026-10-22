// Copyright 2026 AsterSQL.
use crate::KeyRange;

#[test]
fn key_range_codec_and_overlap() {
    for (start, end) in [
        (vec![], vec![]),
        (vec![0, 128, 255], vec![1, 254]),
        (b"a".to_vec(), vec![]),
    ] {
        let range = KeyRange::new(&start, &end);
        let json = serde_json::to_string(&range).unwrap();
        assert_eq!(serde_json::from_str::<KeyRange>(&json).unwrap(), range);
    }
    assert_eq!(
        serde_json::to_string(&KeyRange::new([0, 255], [128])).unwrap(),
        r#"{"end-key":"80","start-key":"00ff"}"#
    );
    for input in ["null", "{}", r#"{"start-key":null,"end-key":null}"#] {
        assert_eq!(
            serde_json::from_str::<KeyRange>(input).unwrap(),
            KeyRange::default()
        );
    }
    assert_eq!(
        serde_json::from_str::<KeyRange>(r#"{"start-key":"Af","other":"ignored"}"#).unwrap(),
        KeyRange::new([175], [])
    );
    for input in [
        r#"{"start-key":"a"}"#,
        r#"{"end-key":"zz"}"#,
        r#"{"start-key":1}"#,
        r#"{"other":1}"#,
        "[]",
    ] {
        assert!(serde_json::from_str::<KeyRange>(input).is_err(), "{input}");
    }
    for (a, b, overlap, adjacent) in [
        (("a", "c"), ("b", "d"), true, false),
        (("a", "b"), ("c", "d"), false, false),
        (("a", "b"), ("b", "d"), true, true),
        (("a", ""), ("z", ""), true, false),
        (("", "a"), ("z", ""), true, true),
        (("", "a"), ("z", "zz"), true, false),
        (("", ""), ("", ""), true, true),
        (("b", "b"), ("b", "c"), true, true),
        (("z", "a"), ("b", "c"), false, false),
    ] {
        let a = KeyRange::new(a.0, a.1);
        let b = KeyRange::new(b.0, b.1);
        assert_eq!(a.over_lapped(&b), overlap);
        assert_eq!(b.over_lapped(&a), overlap);
        assert_eq!(a.is_adjacent(&b), adjacent);
        assert_eq!(b.is_adjacent(&a), adjacent);
    }
}

// Expected values follow keyrange.go, including preservation of non-adjacent overlaps.
#[test]
fn sort_deduce_and_merge() {
    use crate::KeyRanges;
    fn ranges(pairs: &[(&str, &str)]) -> Vec<KeyRange> {
        pairs.iter().map(|(a, b)| KeyRange::new(a, b)).collect()
    }
    let cases: &[(&[(&str, &str)], &[(&str, &str)], &[(&str, &str)])] = &[
        (&[], &[], &[]),
        (&[("a", "b")], &[("a", "b")], &[("a", "b")]),
        (
            &[("c", "d"), ("a", "b")],
            &[("a", "b"), ("c", "d")],
            &[("c", "d"), ("a", "b")],
        ),
        (
            &[("b", "c"), ("a", "b"), ("c", "d")],
            &[("a", "d")],
            &[("b", "c"), ("a", "b"), ("c", "d")],
        ),
        (
            &[("a", "c"), ("b", "d")],
            &[("a", "c"), ("b", "d")],
            &[("a", "c"), ("b", "d")],
        ),
        (
            &[("a", "c"), ("a", "c")],
            &[("a", "c"), ("a", "c")],
            &[("a", "c"), ("a", "c")],
        ),
        (
            &[("", "b"), ("b", "c"), ("c", "")],
            &[("", "")],
            &[("", "")],
        ),
        (
            &[("a", ""), ("b", "c")],
            &[("a", ""), ("b", "c")],
            &[("a", ""), ("b", "c")],
        ),
        // IsAdjacent also compares the reverse boundaries, including empty bytes.
        (
            &[("", "a"), ("z", "")],
            &[("", "")],
            &[("", "a"), ("z", "")],
        ),
        (
            &[("a", "c"), ("c", "d"), ("f", "g")],
            &[("a", "d"), ("f", "g")],
            &[("a", "d"), ("f", "g")],
        ),
    ];
    for &(input, sorted, merged) in cases {
        let mut collection = KeyRanges::new(ranges(input));
        collection.sort_and_deduce();
        assert_eq!(collection.ranges(), ranges(sorted), "sort {input:?}");
        let mut collection = KeyRanges::new(ranges(input));
        collection.merge();
        assert_eq!(collection.ranges(), ranges(merged), "merge {input:?}");
    }
    let mut bytes = KeyRanges::new(vec![
        KeyRange::new([255], []),
        KeyRange::new([0], [128]),
        KeyRange::new([128], [255]),
    ]);
    bytes.sort_and_deduce();
    assert_eq!(bytes.ranges(), &[KeyRange::new([0], [])]);
}
