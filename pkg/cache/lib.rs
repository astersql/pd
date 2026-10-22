// Copyright 2026 AsterSQL.
//! In-memory cache algorithms with caller-owned values.

mod lru;
pub use lru::{Item, Lru};

#[cfg(test)]
#[path = "cache_test.rs"]
mod cache_test;
