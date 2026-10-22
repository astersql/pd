// Copyright 2026 AsterSQL.
use crate::*;
use std::sync::Arc;
fn region(id: u64, start: &[u8], end: &[u8]) -> Arc<RegionInfo> {
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.to_vec(),
                end_key: end.to_vec(),
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ))
}
fn id(r: Option<Arc<RegionInfo>>) -> Option<u64> {
    r.map(|r| r.meta().id)
}
#[test]
fn core_task_39_fixture_queries() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_39.json")).unwrap();
    for case in f["cases"].as_array().unwrap() {
        let tree = RegionTree::from_regions(case["regions"].as_array().unwrap().iter().map(|r| {
            region(
                r[0].as_u64().unwrap(),
                r[1].as_str().unwrap().as_bytes(),
                r[2].as_str().unwrap().as_bytes(),
            )
        }));
        let keys: Vec<_> = case["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().as_bytes())
            .collect();
        let expected: Vec<Option<u64>> = serde_json::from_value(case["search"].clone()).unwrap();
        assert_eq!(
            tree.search_by_keys(&keys)
                .into_iter()
                .map(id)
                .collect::<Vec<_>>(),
            expected
        );
        let expected: Vec<Option<u64>> = serde_json::from_value(case["prev"].clone()).unwrap();
        assert_eq!(
            tree.search_by_prev_keys(&keys)
                .into_iter()
                .map(id)
                .collect::<Vec<_>>(),
            expected
        );
    }
}
#[test]
fn core_task_39_adjacent_and_ordering() {
    let a = RegionItem::new(region(1, b"a", b"b"));
    let b = RegionItem::new(region(9, b"a", b"z"));
    assert!(!a.less(&b));
    assert!(!b.less(&a));
    assert!(a.less(&RegionItem::new(region(0, b"\xff", b""))));
    let tree = RegionTree::from_regions([
        region(4, b"d", b""),
        region(2, b"b", b"c"),
        region(1, b"a", b"b"),
    ]);
    for (key, prev, next) in [
        (b"a".as_slice(), None, Some(2)),
        (b"b", Some(1), Some(4)),
        (b"c", Some(2), Some(4)),
        (b"d", Some(2), None),
        (b"", None, Some(1)),
        (b"z", Some(4), None),
    ] {
        let (p, n) = tree.get_adjacent_regions(&region(999, key, b""));
        assert_eq!((id(p), id(n)), (prev, next));
    }
    assert!(tree.search_by_keys(&[]).is_empty());
    assert!(tree.search_by_prev_keys(&[]).is_empty());
}
#[test]
fn core_task_39_binary_bounds_and_shared_identity() {
    let r = region(1, b"\x80", b"\xff");
    let tree = RegionTree::from_regions([r.clone()]);
    assert!(tree.search(b"\x7f").is_none());
    assert!(tree.search(b"\xff").is_none());
    assert!(Arc::ptr_eq(&tree.search(b"\x80").unwrap(), &r));
    let t = RegionTree::from_regions([region(2, b"", b"")]);
    assert_eq!(id(t.search(b"")), Some(2));
    assert_eq!(id(t.search(b"\xff\xff")), Some(2));
}
