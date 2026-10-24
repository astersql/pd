// Copyright 2026 AsterSQL.
//! Owned bucket report values, independent of heartbeat protocols.
mod bucket_values;
pub use bucket_values::*;

#[cfg(test)]
#[path = "bucket_values_test.rs"]
mod bucket_values_test;

mod bucket_range;
pub use bucket_range::*;

#[cfg(test)]
#[path = "bucket_range_test.rs"]
mod bucket_range_test;

mod bucket_inherit;

#[cfg(test)]
#[path = "bucket_inherit_test.rs"]
mod bucket_inherit_test;

mod bucket_degree;
pub use bucket_degree::{MAX_HOT_DEGREE, MIN_HOT_DEGREE};

#[cfg(test)]
#[path = "bucket_degree_test.rs"]
mod bucket_degree_test;

mod bucket_worker;
pub use bucket_worker::*;

#[cfg(test)]
#[path = "bucket_worker_test.rs"]
mod bucket_worker_test;
