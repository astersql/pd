// Copyright 2026 AsterSQL.

#![forbid(unsafe_code)]

use std::{
    collections::HashSet,
    fmt,
    hash::Hash,
    sync::atomic::{AtomicU32, Ordering},
};

use astersql_pd_syncutil::blocking::RwLock;

/// Balancing policy, including unknown Go integer policy values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Policy(pub isize);

impl Policy {
    /// Select nodes in round-robin order.
    pub const ROUND_ROBIN: Self = Self(0);
    /// Least-used policy (currently falls back to round robin).
    pub const LEAST: Self = Self(1);
}

impl fmt::Display for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::ROUND_ROBIN => "round-robin",
            Self::LEAST => "least",
            _ => "unknown",
        })
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u32 {}
    impl Sealed for String {}
}

/// Supported Go balancer node types: uint32 and string.
pub trait Node: sealed::Sealed + Clone + Default + Eq + Hash + Send + Sync {}
impl Node for u32 {}
impl Node for String {}

#[derive(Default)]
struct Membership<T> {
    nodes: Vec<T>,
    exists: HashSet<T>,
}

/// Thread-safe insertion-ordered node collection for round-robin balancing.
///
/// Node snapshots are independent copies. Locks preserve Rust poisoning semantics.
/// Selection uses an atomic cursor; deletion is introduced by a subsequent task.
pub struct RoundRobin<T: Node> {
    membership: RwLock<Membership<T>>,
    next: AtomicU32,
}

impl<T: Node> Default for RoundRobin<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Node> RoundRobin<T> {
    /// Create an empty node collection.
    pub fn new() -> Self {
        Self {
            membership: RwLock::new(Membership {
                nodes: Vec::new(),
                exists: HashSet::new(),
            }),
            next: AtomicU32::new(0),
        }
    }

    /// Return the next node in insertion order, or the node type's default if empty.
    ///
    /// Empty selections do not advance the cursor. Membership stays read-locked
    /// through selection, while the atomic cursor gives concurrent readers unique
    /// positions. The lock, rather than the cursor, synchronizes node contents.
    pub fn next(&self) -> T {
        let membership = self.membership.read().expect("balancer lock poisoned");
        if membership.nodes.is_empty() {
            return T::default();
        }
        let next = self.next.fetch_add(1, Ordering::Relaxed);
        membership.nodes[next as usize % membership.nodes.len()].clone()
    }

    /// Append a node only on its first insertion.
    pub fn put(&self, node: T) {
        let mut membership = self.membership.write().expect("balancer lock poisoned");
        if membership.exists.insert(node.clone()) {
            membership.nodes.push(node);
        }
    }

    /// Return an independent snapshot in insertion order.
    pub fn get_all(&self) -> Vec<T> {
        self.membership
            .read()
            .expect("balancer lock poisoned")
            .nodes
            .clone()
    }

    /// Return the number of unique nodes.
    pub fn len(&self) -> usize {
        self.membership
            .read()
            .expect("balancer lock poisoned")
            .nodes
            .len()
    }

    /// Return whether the collection has no nodes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Create a balancer, falling back to round robin for every policy as Go does.
pub fn gen_by_policy<T: Node>(_policy: Policy) -> RoundRobin<T> {
    RoundRobin::new()
}

#[cfg(test)]
mod balancer_test;
