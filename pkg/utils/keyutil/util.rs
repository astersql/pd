// Copyright 2026 AsterSQL.
//! Byte-key comparisons matching Go keyutil, including its empty-boundary rules.

use std::collections::HashMap;

/// Builds a range identifier from lowercase hexadecimal bounds separated by a hyphen.
pub fn build_key_range_key(start_key: &[u8], end_key: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut key = String::with_capacity((start_key.len() + end_key.len()) * 2 + 1);
    for (i, bound) in [start_key, end_key].into_iter().enumerate() {
        if i == 1 {
            key.push('-');
        }
        for &byte in bound {
            key.push(char::from(HEX[usize::from(byte >> 4)]));
            key.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    key
}

/// Groups consecutive strings into owned start/end maps in input order.
/// Returns None for no keys, preserving Go's nil result.
///
/// # Panics
/// Panics when the number of keys is odd, matching Go's incomplete-pair panic.
pub fn build_key_range_maps(keys: &[&str]) -> Option<Vec<HashMap<&'static str, String>>> {
    assert!(keys.len().is_multiple_of(2), "incomplete key range pair");
    if keys.is_empty() {
        return None;
    }
    Some(
        keys.chunks_exact(2)
            .map(|pair| {
                HashMap::from([
                    ("start_key", pair[0].to_owned()),
                    ("end_key", pair[1].to_owned()),
                ])
            })
            .collect(),
    )
}

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
