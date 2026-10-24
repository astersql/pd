// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod cpu;
pub use cpu::{region_read_cpu_usage, store_read_cpu_usage};

#[cfg(test)]
mod cpu_test;
