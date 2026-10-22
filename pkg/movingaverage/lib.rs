// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod weight_allocator;

pub use weight_allocator::WeightAllocator;

#[cfg(test)]
mod weight_allocator_test;
