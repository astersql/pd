// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn mcs_026_behavior() {
    let cache = Cache::default();
    cache.save(100, "ks-100", KeyspaceState::Enabled);
    cache.save(101, "ks-101", KeyspaceState::Archived);
    cache.save(102, "ks-102", KeyspaceState::Disabled);
    cache.save(103, "ks-103", KeyspaceState::Tombstone);
    assert!(
        cache.keyspace_exist(100),
        "saved live keyspace must become visible"
    );
    assert!(cache.keyspace_exist(101));
    assert!(cache.keyspace_exist(102));
    assert!(!cache.keyspace_exist(103));
}
#[test]
fn mcs_026_ranges_tombstones_updates_limits_and_owned_snapshots() {
    let c = Cache::default();
    for (id, state) in [
        (100, KeyspaceState::Enabled),
        (101, KeyspaceState::Archived),
        (102, KeyspaceState::Disabled),
        (103, KeyspaceState::Tombstone),
    ] {
        c.save(id, format!("ks-{id}"), state);
    }
    assert_eq!(c.get(101).unwrap().name, "ks-101");
    assert_eq!(c.get(101).unwrap().id, 101);
    assert_eq!(c.get(101).unwrap().state, KeyspaceState::Archived);
    assert_eq!(c.ids_in_range(100, 103, 1), (vec![102], true));
    assert_eq!(c.ids_in_range(101, 102, 1), (vec![102], true));
    assert_eq!(c.ids_in_range(103, 104, 1), (vec![], false));
    let old = c.get(101).unwrap();
    c.save(101, "changed", KeyspaceState::Unknown(19));
    assert_eq!(old.name, "ks-101");
    assert_eq!(c.get(101).unwrap().name, "changed");
    for limit in [0, -1, isize::MIN] {
        assert_eq!(c.ids_in_range(100, 103, limit), (vec![102, 101, 100], true));
    }
    assert_eq!(c.ids_in_range(103, 100, 9), (vec![], false));
    let mut all = vec![];
    c.scan_all(|id, _| {
        all.push(id);
        true
    });
    assert_eq!(all, [100, 101, 102, 103]);
    let mut first = vec![];
    c.scan_all(|id, _| {
        first.push(id);
        false
    });
    assert_eq!(first, [100]);
    c.delete_keyspace(101);
    c.delete_keyspace(101);
    assert!(c.get(101).is_none());
    let mut after_delete = vec![];
    c.scan_all(|id, _| {
        after_delete.push(id);
        true
    });
    assert_eq!(after_delete, [100, 102, 103]);
    assert_eq!(c.ids_in_range(100, 103, 5), (vec![102, 100], true));
    c.save(102, "gone", KeyspaceState::Tombstone);
    assert!(!c.keyspace_exist(102));
    assert!(c.get(102).is_some());
}
#[test]
fn mcs_026_span_boundaries_holes_empty_infinity_and_invalid_modes() {
    let c = Cache::default();
    c.save(2, "two", KeyspaceState::Enabled);
    c.save(4, "four", KeyspaceState::Enabled);
    let two = make_region_bound(2);
    let three = make_region_bound(3);
    let four = make_region_bound(4);
    assert!(!region_spans_multiple_keyspaces(&[], &[], None));
    assert!(region_spans_multiple_keyspaces(&[], &[], Some(&c)));
    assert!(!region_spans_multiple_keyspaces(b"bad", &[], Some(&c)));
    assert!(!region_spans_multiple_keyspaces(b"bad", b"worse", Some(&c)));
    assert!(region_spans_multiple_keyspaces(
        &two.txn_left_bound,
        b"bad",
        Some(&c)
    ));
    assert!(region_spans_multiple_keyspaces(
        b"bad",
        &two.txn_left_bound,
        Some(&c)
    ));
    assert!(
        !region_spans_multiple_keyspaces(&two.txn_left_bound, &three.txn_left_bound, Some(&c)),
        "exact right boundary is within a single keyspace"
    );
    let mut payload = vec![b'x', 0, 0, 3];
    payload.extend_from_slice(b"row");
    let payload = astersql_pd_codec::encode_bytes(&payload);
    assert!(region_spans_multiple_keyspaces(
        &two.txn_left_bound,
        &payload,
        Some(&c)
    ));
    assert!(region_spans_multiple_keyspaces(
        &two.txn_left_bound,
        &[],
        Some(&c)
    ));
    c.delete_keyspace(2);
    assert!(!region_spans_multiple_keyspaces(
        &two.txn_left_bound,
        &[],
        Some(&c)
    ));
    assert!(
        !region_spans_multiple_keyspaces(
            &make_region_bound(0xffffff).txn_left_bound,
            &[],
            Some(&c)
        ),
        "max txn early equality precedes infinity existence check"
    );
    assert!(
        region_spans_multiple_keyspaces(&three.raw_left_bound, &two.txn_left_bound, Some(&c)),
        "raw to txn crosses modes regardless of ID existence"
    );
    assert!(
        !region_spans_multiple_keyspaces(&two.txn_left_bound, &payload, Some(&c)),
        "strictly interior live IDs do not affect the endpoint-only check"
    );
    assert!(
        region_spans_multiple_keyspaces(&two.txn_left_bound, &four.txn_right_bound, Some(&c))
            == c.keyspace_exist(2)
    );
}
#[test]
fn mcs_026_split_keys_scan_ten_highest_ids_and_deduplicate_adjacent_edges() {
    let c = Cache::default();
    for id in 1..=15 {
        c.save(id, format!("ks-{id}"), KeyspaceState::Enabled);
    }
    let split = get_keyspace_split_keys(&[], &[], "txn", Some(&c));
    let expected = (6..=16)
        .map(|id| make_region_bound(id).txn_left_bound.to_vec())
        .collect::<Vec<_>>();
    assert_eq!(split, expected, "highest ten live IDs, not lowest ten");
    c.save(15, "gone", KeyspaceState::Tombstone);
    let split = get_keyspace_split_keys(&[], &[], "txn", Some(&c));
    let expected = (5..=15)
        .map(|id| make_region_bound(id).txn_left_bound.to_vec())
        .collect::<Vec<_>>();
    assert_eq!(split, expected);
    let b = make_region_bound(7);
    assert!(
        get_keyspace_split_keys(&b.txn_left_bound, &b.txn_right_bound, "txn", Some(&c)).is_empty()
    );
    let raw = get_keyspace_split_keys(
        &b.raw_left_bound,
        &make_region_bound(10).raw_left_bound,
        "raw",
        Some(&c),
    );
    assert_eq!(
        raw,
        [
            make_region_bound(8).raw_left_bound.to_vec(),
            make_region_bound(9).raw_left_bound.to_vec()
        ]
    );
    assert_eq!(
        get_keyspace_split_keys(b"invalid", &[], "raw", Some(&c)),
        get_keyspace_split_keys(&[], &[], "raw", Some(&c))
    );
    assert!(get_keyspace_split_keys(&[], &[], "raw", None).is_empty());
    c.save(0xffffff, "max", KeyspaceState::Enabled);
    let keys = get_keyspace_split_keys(&[], &[], "raw", Some(&c));
    assert!(keys.contains(&make_region_bound(0xffffff).raw_right_bound.to_vec()));
}
#[test]
fn mcs_026_concurrent_mutations_publish_complete_items_and_ordered_ranges() {
    let c = std::sync::Arc::new(Cache::default());
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let c = c.clone();
            scope.spawn(move || {
                for i in 0..500 {
                    let id = worker * 1000 + i;
                    c.save(id, format!("ks-{id}"), KeyspaceState::Enabled);
                    if i % 2 == 0 {
                        c.delete_keyspace(id);
                    }
                    let (ids, _) = c.ids_in_range(0, u32::MAX, 10);
                    assert!(ids.windows(2).all(|v| v[0] > v[1]));
                    for id in ids {
                        if let Some(item) = c.get(id) {
                            assert_eq!(item.name, format!("ks-{id}"));
                        }
                    }
                }
            });
        }
    });
    let (ids, found) = c.ids_in_range(0, u32::MAX, 0);
    assert!(found);
    assert_eq!(ids.len(), 1000);
    assert!(ids.iter().all(|id| id % 2 == 1));
}
/// Fixed input/assertion pairs from the byte-identical upstream Go tests.
/// The matching Go execution receipt is preserved in task-026-resume-evidence.json.gz.
/// This replays historical Go assertions, rather than claiming a new local Go run.
#[test]
fn mcs_026_replays_upstream_fixed_span_and_split_assertions_on_real_cache() {
    use astersql_pd_codec::encode_bytes;
    let all = Cache::default();
    for id in 0..=205 {
        all.save(id, format!("ks-{id}"), KeyspaceState::Enabled);
    }
    let specific = Cache::default();
    for id in [100, 101, 105] {
        specific.save(id, format!("ks-{id}"), KeyspaceState::Enabled);
    }
    let txn = |id| make_region_bound(id).txn_left_bound.to_vec();
    let right = |id| make_region_bound(id).txn_right_bound.to_vec();
    let classical = |last| encode_bytes(&[b't', 1, 2, last]);
    for (name, start, end, c, expected) in [
        ("empty start and end key", vec![], vec![], &all, true),
        ("empty end key", txn(1), vec![], &all, true),
        ("same keyspace txn mode", txn(100), right(100), &all, false),
        (
            "adjacent keyspace boundary txn mode",
            txn(100),
            txn(101),
            &all,
            false,
        ),
        (
            "span two keyspaces txn mode",
            txn(100),
            right(101),
            &all,
            true,
        ),
        (
            "span multiple keyspaces txn mode",
            txn(100),
            right(105),
            &all,
            true,
        ),
        ("empty start key", vec![], txn(100), &all, true),
        ("non-keyspace keys", classical(3), classical(4), &all, false),
        (
            "classical start and keyspace end",
            classical(3),
            txn(100),
            &all,
            true,
        ),
        (
            "keyspace start and classical end",
            txn(100),
            classical(3),
            &all,
            true,
        ),
        (
            "deleted middle with existing endpoints",
            txn(100),
            right(102),
            &specific,
            true,
        ),
        ("existing endpoints", txn(100), right(101), &specific, true),
        (
            "infinite end with live start",
            txn(100),
            vec![],
            &specific,
            true,
        ),
        (
            "infinite end with deleted start",
            txn(200),
            vec![],
            &specific,
            false,
        ),
    ] {
        let actual = region_spans_multiple_keyspaces(&start, &end, Some(c));
        assert_eq!(actual, expected, "Go fixture: {name}");
        println!("Go span fixture {name}: {actual}");
    }
    let specific = Cache::default();
    for id in [100, 101, 102] {
        specific.save(id, format!("ks-{id}"), KeyspaceState::Enabled);
    }
    let one = Cache::default();
    one.save(101, "one", KeyspaceState::Enabled);
    let raw = |id| make_region_bound(id).raw_left_bound.to_vec();
    let raw_right = |id| make_region_bound(id).raw_right_bound.to_vec();
    for (name, start, end, mode, c, expected) in [
        (
            "classical start before first keyspace",
            vec![b't', 1, 2, 4],
            txn(99),
            "txn",
            &specific,
            vec![],
        ),
        (
            "raw range into classical tail",
            raw(102),
            vec![b't', 1, 2, 4],
            "raw",
            &specific,
            vec![raw(103)],
        ),
        (
            "classical start into txn keyspace",
            vec![b't', 1, 2, 4],
            txn(102),
            "txn",
            &specific,
            vec![txn(100), txn(101)],
        ),
        (
            "sparse existing keyspaces",
            raw(99),
            vec![b't', 1, 2, 4],
            "raw",
            &specific,
            vec![raw(100), raw(101), raw(102), raw(103)],
        ),
        (
            "span two txn keyspaces",
            txn(100),
            right(101),
            "txn",
            &all,
            vec![right(100)],
        ),
        (
            "span two raw keyspaces",
            raw(100),
            raw_right(101),
            "raw",
            &all,
            vec![raw_right(100)],
        ),
        (
            "same txn keyspace",
            txn(100),
            right(100),
            "txn",
            &all,
            vec![],
        ),
        (
            "same raw keyspace",
            raw(100),
            raw_right(100),
            "raw",
            &all,
            vec![],
        ),
        (
            "adjacent range with one keyspace",
            txn(101),
            right(102),
            "txn",
            &one,
            vec![right(101)],
        ),
        (
            "entire raw with three existing",
            vec![],
            vec![],
            "raw",
            &specific,
            vec![raw(100), raw(101), raw(102), raw(103)],
        ),
        (
            "entire txn with three existing",
            vec![],
            vec![],
            "txn",
            &specific,
            vec![txn(100), txn(101), txn(102), txn(103)],
        ),
        (
            "no keyspace keys on either side",
            vec![b't', 1, 2, 3],
            vec![b't', 1, 2, 4],
            "txn",
            &specific,
            vec![],
        ),
    ] {
        let actual = get_keyspace_split_keys(&start, &end, mode, Some(c));
        assert_eq!(actual, expected, "Go fixture: {name}");
        println!("Go split fixture {name}: {actual:?}");
    }
}
