// Copyright 2026 AsterSQL.
use super::*;

#[test]
fn key_boundary_comparison() {
    let keys: &[&[u8]] = &[b"", b"\0", b"a", b"aa", b"b", b"\x80", b"\xff"];
    for &a in keys {
        for &b in keys {
            assert_eq!(max_key(a, b), if a > b { a } else { b });
            assert_eq!(min_key(a, b), if a > b { b } else { a });
            assert_eq!(
                max_start_key(a, b),
                if a.is_empty() || (!b.is_empty() && a > b) {
                    a
                } else {
                    b
                }
            );
            assert_eq!(
                min_end_key(a, b),
                if a.is_empty() || (!b.is_empty() && a > b) {
                    b
                } else {
                    a
                }
            );
            assert_eq!(less(a, b, Boundary::Left), a < b);
            assert_eq!(
                less(a, b, Boundary::Right),
                a < b || (b.is_empty() && !a.is_empty())
            );
            for &key in keys {
                assert_eq!(
                    between(a, b, key),
                    a < key && (key < b || b.is_empty()),
                    "start={a:?}, end={b:?}, key={key:?}"
                );
            }
        }
    }
    // Equal values preserve Go's choice of the original slice.
    let a = vec![42];
    let b = vec![42];
    assert!(std::ptr::eq(max_key(&a, &b), b.as_slice()));
    assert!(std::ptr::eq(min_key(&a, &b), a.as_slice()));
    assert!(std::ptr::eq(max_start_key(&a, &b), b.as_slice()));
    assert!(std::ptr::eq(min_end_key(&a, &b), a.as_slice()));
    assert!(!between(b"a", b"c", b"a"));
    assert!(!between(b"a", b"c", b"c"));
    assert!(between(b"", b"", b"\xff"));
    assert!(!between(b"", b"", b""));
}
