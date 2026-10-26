// Copyright 2026 AsterSQL.
//! Built-in scheduler contracts; concrete implementations register only when ported.
mod base;
pub use base::*;
#[cfg(test)]
mod task_065_test;
