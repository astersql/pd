// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod ema;
mod weight_allocator;

pub use ema::Ema;
pub use weight_allocator::WeightAllocator;

#[cfg(test)]
mod moving_average_test;
#[cfg(test)]
mod weight_allocator_test;
