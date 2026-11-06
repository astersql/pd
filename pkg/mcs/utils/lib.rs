// Copyright 2026 AsterSQL.
//! Shared candidate selection and fenced primary transfer; no transport dependencies.
mod expected_primary;
pub use expected_primary::*;

#[cfg(test)]
mod expected_primary_test;
