// Copyright 2026 AsterSQL.
use super::*;

fn ranges(bounds: &[(&str, &str)]) -> Vec<KeyRange> {
    bounds.iter().map(|(a, b)| KeyRange::new(a, b)).collect()
}

fn snapshot(manager: &Manager) -> Vec<KeyRange> {
    manager.ranges.lock().unwrap().ranges().to_vec()
}

#[test]
fn range_append_empty() {
    let manager = Manager::new();
    assert!(manager.is_empty());
    manager.append(&[]);
    assert!(manager.is_empty());
    assert!(snapshot(&manager).is_empty());
}

#[test]
fn range_append_unsorted_adjacent() {
    let manager = Manager::new();
    manager.append(&ranges(&[("c", "d"), ("a", "b"), ("b", "c")]));
    assert_eq!(snapshot(&manager), ranges(&[("a", "d")]));
    assert!(!manager.is_empty());
}

#[test]
fn range_append_duplicates_and_overlaps_are_retained() {
    // Exact inputs and outputs captured from the existing Go test via Delve.
    for (input, expected) in [
        (
            ranges(&[("a", "b"), ("a", "b")]),
            ranges(&[("a", "b"), ("a", "b")]),
        ),
        (
            ranges(&[("a", "c"), ("b", "d")]),
            ranges(&[("a", "c"), ("b", "d")]),
        ),
        (ranges(&[("c", "d"), ("a", "c")]), ranges(&[("a", "d")])),
        (ranges(&[("", "a"), ("a", "")]), ranges(&[("", "")])),
    ] {
        let manager = Manager::new();
        manager.append(&input);
        assert_eq!(snapshot(&manager), expected);
    }

    let manager = Manager::new();
    manager.append(&ranges(&[("a", "c"), ("a", "c"), ("b", "d")]));
    assert_eq!(
        snapshot(&manager),
        ranges(&[("a", "c"), ("a", "c"), ("b", "d")])
    );
}

#[test]
fn range_append_normalizes_across_batches() {
    let manager = Manager::new();
    manager.append(&ranges(&[("c", "d")]));
    manager.append(&ranges(&[("a", "b")]));
    manager.append(&ranges(&[("b", "c")]));
    assert_eq!(snapshot(&manager), ranges(&[("a", "d")]));
    manager.append(&[]);
    assert_eq!(snapshot(&manager), ranges(&[("a", "d")]));
}

#[test]
fn range_append_empty_boundaries_follow_keyutil() {
    let manager = Manager::new();
    manager.append(&ranges(&[("b", ""), ("", "a"), ("a", "b")]));
    assert_eq!(snapshot(&manager), ranges(&[("", "")]));
    assert!(!manager.is_empty());
}

#[test]
fn range_append_copies_input_and_keeps_disjoint_ranges_sorted() {
    let manager = Manager::new();
    let mut input = ranges(&[("c", "d"), ("a", "b")]);
    manager.append(&input);
    input[0].start_key[0] = b'z';
    assert_eq!(snapshot(&manager), ranges(&[("a", "b"), ("c", "d")]));
}

#[test]
fn range_append_concurrent_batches_are_atomic() {
    let manager = Manager::new();
    std::thread::scope(|scope| {
        for i in 1u8..=16 {
            let manager = &manager;
            scope.spawn(move || manager.append(&[KeyRange::new([i], [i + 1])]));
        }
    });
    assert_eq!(snapshot(&manager), vec![KeyRange::new([1], [17])]);
}
