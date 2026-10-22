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
