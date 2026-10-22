// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod avg_over_time;
mod ema;
mod hma;
mod max_filter;
mod median_filter;
mod weight_allocator;
mod wma;

pub use avg_over_time::AvgOverTime;
pub use ema::Ema;
pub use hma::Hma;
pub use max_filter::MaxFilter;
pub use median_filter::MedianFilter;
pub use weight_allocator::WeightAllocator;
pub use wma::Wma;

#[cfg(test)]
mod moving_average_test;
#[cfg(test)]
mod weight_allocator_test;

#[cfg(test)]
mod max_filter_test;

#[cfg(test)]
mod avg_over_time_test;
