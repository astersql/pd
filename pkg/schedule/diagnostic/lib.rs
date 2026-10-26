// Copyright 2026 AsterSQL.
//! Diagnostic queries over host-owned canonical scheduler state.
#![forbid(unsafe_code)]
mod diagnostic;
pub use diagnostic::*;
#[cfg(test)]
mod task_111_test;
