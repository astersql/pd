// Copyright 2026 AsterSQL.

use std::time::Duration;

use super::*;

#[test]
fn comparison_helpers() {
    let strings = |values: &[&str]| {
        values
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
    };

    assert_eq!(min_u64(1, 2), 1);
    assert_eq!(min_u64(2, 1), 1);
    assert_eq!(max_u64(1, 2), 2);
    assert_eq!(max_u64(2, 1), 2);

    assert_eq!(
        min_duration(Duration::from_secs(60), Duration::from_secs(1)),
        Duration::from_secs(1)
    );
    assert_eq!(
        min_duration(Duration::from_secs(1), Duration::from_secs(60)),
        Duration::from_secs(1)
    );

    assert!(float64_equal(1.0, 1.0 + 1e-6));
    assert!(!float64_equal(1.0, 1.0 + 1.1e-6));

    assert!(are_string_slices_equivalent(&strings(&[]), &strings(&[])));
    assert!(are_string_slices_equivalent(
        &strings(&["a", "b"]),
        &strings(&["b", "a"])
    ));
    assert!(are_string_slices_equivalent(
        &strings(&["a", "a", "b"]),
        &strings(&["b", "a", "a"])
    ));
    assert!(!are_string_slices_equivalent(
        &strings(&["a", "a"]),
        &strings(&["a", "b"])
    ));
    assert!(!are_string_slices_equivalent(
        &strings(&["a"]),
        &strings(&["a", "a"])
    ));

    assert!(equal_base_urls("http://127.0.0.1", "https://127.0.0.1"));
    assert!(equal_base_urls("127.0.0.1", "http://127.0.0.1"));
    assert!(!equal_base_urls("HTTP://127.0.0.1", "127.0.0.1"));
    assert_eq!(trim_scheme("https://example.com"), "example.com");
}
