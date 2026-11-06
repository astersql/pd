// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn sa_015_unknown_and_sysctl_parser() {
    for tp in [-1, 4, i32::MAX] {
        assert!(collect(tp).unwrap().items.is_empty());
    }
    let pairs = system_pairs("a: one:ignored\nno colon\nb:  two  \n");
    assert_eq!(pairs.len(), 2);
    assert_eq!((&*pairs[0].key, &*pairs[0].value), ("a", "one"));
    assert_eq!((&*pairs[1].key, &*pairs[1].value), ("b", "two"));
}
#[cfg(target_os = "macos")]
#[test]
fn sa_015_live_machine() {
    for tp in [0, 1, 2, 3] {
        let items = collect(tp).unwrap().items;
        assert!(!items.is_empty(), "request {tp}");
        assert!(
            items
                .windows(2)
                .all(|w| (&w[0].tp, &w[0].name) <= (&w[1].tp, &w[1].name))
        );
        assert!(items.iter().all(|i| !i.pairs.is_empty()));
    }
}

#[test]
fn sa_015_vm_stat_units_and_errors() {
    let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 2.\nPages inactive: 3.\nPages active: 9.\nPages wired down: 1.\n";
    assert_eq!(available_memory(text, 4096), Some(5 * 4096));
    assert_eq!(available_memory("unrecognized: invalid\n", 4096), Some(0));
    assert_eq!(
        available_memory("Pages free: 2.\nPages active: invalid\n", 4096),
        None
    );
    assert_eq!(available_memory("Pages inactive: -1\n", 4096), None);
}
