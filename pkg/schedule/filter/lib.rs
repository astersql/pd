// Copyright 2026 AsterSQL.
//! Ordered scheduling filters over canonical store and configuration snapshots.
mod candidates;
pub use candidates::*;
mod selection;
pub use selection::*;

#[cfg(test)]
#[path = "task_005_test.rs"]
mod task_005_test;

#[cfg(test)]
#[path = "task_006_test.rs"]
mod task_006_test;

mod store_state;
pub use store_state::*;

#[cfg(test)]
#[path = "task_007_test.rs"]
mod task_007_test;

#[cfg(test)]
#[path = "task_008_test.rs"]
mod task_008_test;

mod store_constraints;
pub use store_constraints::*;

#[cfg(test)]
#[path = "task_009_test.rs"]
mod task_009_test;

mod placement_guard;
pub use placement_guard::*;

#[cfg(test)]
#[path = "task_010_test.rs"]
mod task_010_test;

mod region_health;
pub use region_health::*;

#[cfg(test)]
#[path = "task_011_test.rs"]
mod task_011_test;

mod filter_metrics;
pub use filter_metrics::*;
