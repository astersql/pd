// Copyright 2026 AsterSQL.
//! Persistent member identity and explicitly renewed, fenced leadership.
mod member;
pub use member::*;
#[cfg(test)]
extern crate self as astersql_pd_member;
#[cfg(test)]
mod member_test;
