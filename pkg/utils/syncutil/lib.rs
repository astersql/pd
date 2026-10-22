// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

mod lock_group;

pub use lock_group::LockGroup;

/// Blocking locks backed directly by [`std::sync`].
///
/// These locks preserve Rust's poisoning contract when a thread panics while
/// holding a guard. Share a lock through an [`std::sync::Arc`] rather than
/// attempting to copy the lock itself.
///
/// ```compile_fail
/// use astersql_pd_syncutil::blocking::Mutex;
///
/// let lock = Mutex::new(());
/// let moved = lock;
/// drop(lock);
/// drop(moved);
/// ```
pub mod blocking {
    pub use std::sync::{Mutex, RwLock};
}

/// Async-aware locks backed directly by [`tokio::sync`].
///
/// Tokio locks do not poison after a task panic and rely on the caller's
/// runtime. The crate does not create or own a runtime.
///
/// ```compile_fail
/// use astersql_pd_syncutil::asynchronous::Mutex;
///
/// let lock = Mutex::new(());
/// let moved = lock;
/// drop(lock);
/// drop(moved);
/// ```
pub mod asynchronous {
    pub use tokio::sync::{Mutex, RwLock};
}

#[cfg(test)]
mod lock_group_test;

#[cfg(test)]
mod mutex_test;
