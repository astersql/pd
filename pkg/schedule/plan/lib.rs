// Copyright 2026 AsterSQL.
//! Scheduling diagnostic value objects.
pub mod status;
pub use status::{Status, StatusCode, status_text};
#[cfg(test)]
#[path = "plan_status_test.rs"]
mod plan_status_test;
