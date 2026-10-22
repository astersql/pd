// Copyright 2026 AsterSQL.

mod limit_types;

pub use limit_types::*;

#[cfg(test)]
#[path = "limit_types_test.rs"]
mod limit_types_test;

#[cfg(test)]
#[path = "rate_limit_test.rs"]
mod rate_limit_test;

mod rate_limit;
pub use rate_limit::StoreRateLimit;

#[cfg(test)]
#[path = "rate_reset_test.rs"]
mod rate_reset_test;

mod rate_reset;

#[cfg(test)]
#[path = "window_test.rs"]
mod window_test;

mod window;
pub use window::Window;

#[cfg(test)]
#[path = "sliding_windows_test.rs"]
mod sliding_windows_test;

mod sliding_windows;
pub use sliding_windows::SlidingWindows;

#[cfg(test)]
#[path = "window_feedback_test.rs"]
mod window_feedback_test;

mod window_feedback;
