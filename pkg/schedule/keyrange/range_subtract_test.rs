// Copyright 2026 AsterSQL.
use std::sync::{Arc, Barrier};
use std::thread;

use super::*;

fn r(a: &str, b: &str) -> KeyRange {
    KeyRange::new(a, b)
}

#[test]
fn range_subtract_go_complement_cases() {
    for (stored, base, expected) in [
        (vec![], r("", ""), vec![]),
        (
            vec![r("a", "d"), r("e", "f")],
            r("", ""),
            vec![r("", "a"), r("d", "e"), r("f", "")],
        ),
        (
            vec![r("a", "d"), r("e", "f")],
            r("g", "h"),
            vec![r("g", "h")],
        ),
        (
            vec![r("a", "d"), r("e", "f")],
            r("c", "g"),
            vec![r("d", "e"), r("f", "g")],
        ),
        (vec![r("a", "d")], r("b", "c"), vec![]),
    ] {
        let manager = Manager::new();
        manager.append(&stored);
        assert_eq!(manager.get_non_overlapping_key_ranges(&base), expected);
    }
}

#[test]
fn range_subtract_go_delete_cases() {
    for (stored, base, expected) in [
        (vec![], r("", ""), vec![]),
        (
            vec![r("a", "b"), r("c", "d")],
            r("f", "g"),
            vec![r("a", "b"), r("c", "d")],
        ),
        (
            vec![r("", "c"), r("e", "")],
            r("b", "d"),
            vec![r("", "b"), r("e", "")],
        ),
        (vec![r("", "")], r("b", "d"), vec![r("", "b"), r("d", "")]),
        (vec![r("", "a"), r("b", "d")], r("b", "d"), vec![r("", "a")]),
    ] {
        let manager = Manager {
            ranges: Mutex::new(KeyRanges::new(stored)),
        };
        manager.delete(&[base]);
        assert_eq!(manager.ranges.lock().unwrap().ranges(), expected);
    }
}

#[test]
fn range_subtract_batch_holes_and_owned_reads() {
    let manager = Manager::new();
    manager.append(&[r("a", "z")]);
    manager.delete(&[r("c", "e"), r("g", "i"), r("k", "m")]);
    let expected = vec![r("c", "e"), r("g", "i"), r("k", "m")];
    let mut result = manager.get_non_overlapping_key_ranges(&r("a", "z"));
    assert_eq!(result, expected);
    result[0].start_key.clear();
    assert_eq!(
        manager.get_non_overlapping_key_ranges(&r("a", "z")),
        expected
    );
    manager.delete(&[]);
    assert_eq!(
        manager.get_non_overlapping_key_ranges(&r("a", "z")),
        expected
    );
}

#[test]
fn range_subtract_concurrent_batch_is_atomic() {
    let manager = Arc::new(Manager::new());
    manager.append(&[r("a", "z")]);
    let barrier = Arc::new(Barrier::new(9));
    let readers: Vec<_> = (0..8)
        .map(|_| {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..1000 {
                    let result = manager.get_non_overlapping_key_ranges(&r("a", "z"));
                    assert!(result.is_empty() || result == vec![r("c", "e"), r("g", "i")]);
                }
            })
        })
        .collect();
    barrier.wait();
    manager.delete(&[r("c", "e"), r("g", "i")]);
    for reader in readers {
        reader.join().unwrap();
    }
    assert_eq!(
        manager.get_non_overlapping_key_ranges(&r("a", "z")),
        vec![r("c", "e"), r("g", "i")]
    );
}

#[test]
fn range_subtract_append_normalization_precedes_delete() {
    let manager = Manager::new();
    manager.append(&[r("", "c"), r("e", "")]);
    manager.delete(&[r("b", "d")]);
    assert_eq!(
        manager.ranges.lock().unwrap().ranges(),
        vec![r("", "b"), r("d", "")]
    );
}
