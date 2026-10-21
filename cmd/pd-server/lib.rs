// Copyright 2026 AsterSQL.
//! Standalone process assembly for the Rust lifecycle shell.
#[path = "main.rs"]
mod entry;
pub use entry::main;

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
