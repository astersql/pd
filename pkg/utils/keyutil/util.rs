// Copyright 2026 AsterSQL.
//! Byte-key comparisons matching Go keyutil, including its empty-boundary rules.

/// Returns the lexicographically larger key, choosing `b` on equality.
pub fn max_key<'a>(a: &'a [u8], b: &'a [u8]) -> &'a [u8] {
    if a > b { a } else { b }
}

/// Returns the lexicographically smaller key, choosing `a` on equality.
pub fn min_key<'a>(a: &'a [u8], b: &'a [u8]) -> &'a [u8] {
    if a > b { b } else { a }
}

/// Returns the larger start key, treating an empty key as the largest.
///
/// This preserves Go's MaxStartKey behavior despite ordinary start comparison
/// treating empty bytes as the smallest key.
pub fn max_start_key<'a>(a: &'a [u8], b: &'a [u8]) -> &'a [u8] {
    if a.is_empty() {
        a
    } else if b.is_empty() {
        b
    } else {
        max_key(a, b)
    }
}

/// Returns the smaller end key, treating an empty end as positive infinity.
pub fn min_end_key<'a>(a: &'a [u8], b: &'a [u8]) -> &'a [u8] {
    if a.is_empty() {
        b
    } else if b.is_empty() {
        a
    } else {
        min_key(a, b)
    }
}

#[derive(Clone, Copy)]
enum Boundary {
    Left,
    Right,
}

fn less(a: &[u8], b: &[u8], boundary: Boundary) -> bool {
    a < b || (matches!(boundary, Boundary::Right) && b.is_empty() && !a.is_empty())
}

/// Reports whether `start_key < key < end_key`, with empty end unbounded.
///
/// Both endpoints are excluded, matching Go's Between implementation. An empty
/// candidate never lies after a start key, including an empty start key.
pub fn between(start_key: &[u8], end_key: &[u8], key: &[u8]) -> bool {
    less(start_key, key, Boundary::Left) && less(key, end_key, Boundary::Right)
}

#[cfg(test)]
#[path = "util_test.rs"]
mod tests;
