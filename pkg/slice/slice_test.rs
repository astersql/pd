// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn predicate_quantifiers() {
    let cases = [
        (&[][..], false, true, true),
        (&[1, 2, 3][..], true, false, false),
        (&[1, 3][..], false, true, false),
        (&[2, 2, 4][..], true, false, true),
    ];

    for (values, expected_any, expected_none, expected_all) in cases {
        assert_eq!(any_of(values, |index| values[index] % 2 == 0), expected_any);
        assert_eq!(
            none_of(values, |index| values[index] % 2 == 0),
            expected_none
        );
        assert_eq!(all_of(values, |index| values[index] % 2 == 0), expected_all);
    }
}

#[test]
fn contains_and_remove() {
    assert!(contains(&["a", "b", "c"], &"a"));
    assert!(!contains(&["a", "b", "c"], &"d"));
    assert!(!contains::<i64>(&[], &1));

    let mut values = vec![1, 2, 1, 3, 1, 4];
    remove(&mut values, &1);
    assert_eq!(values, vec![2, 3, 4]);

    remove(&mut values, &9);
    assert_eq!(values, vec![2, 3, 4]);

    remove(&mut values, &2);
    assert_eq!(values, vec![3, 4]);
}

#[test]
fn multiset_equal_and_sorted_duplicate() {
    assert!(equal_without_order(&[1, 2, 1], &[2, 1, 1]));
    assert!(!equal_without_order(&[1, 1, 2], &[1, 2, 2]));
    assert!(!equal_without_order(&[1, 2], &[1, 2, 2]));
    assert!(equal_without_order::<i32>(&[], &[]));

    assert!(!has_dup_in_sorted::<i32>(&[]));
    assert!(!has_dup_in_sorted(&[1]));
    assert!(!has_dup_in_sorted(&[1, 2, 3]));
    assert!(has_dup_in_sorted(&[1, 1, 2]));
    assert!(has_dup_in_sorted(&[1, 2, 2]));
}
