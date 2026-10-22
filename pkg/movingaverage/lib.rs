// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod ema;
mod hma;
mod weight_allocator;
mod wma;

pub use ema::Ema;
pub use hma::Hma;
pub use weight_allocator::WeightAllocator;
pub use wma::Wma;

#[cfg(test)]
mod moving_average_test;
#[cfg(test)]
mod weight_allocator_test;
