// Copyright 2026 AsterSQL.

use std::fmt;

/// A store limit type, retaining unknown signed values like the Go integer type.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Type(i64);

impl Type {
    /// Limits the rate of adding peers.
    pub const ADD_PEER: Self = Self(0);
    /// Limits the rate of removing peers.
    pub const REMOVE_PEER: Self = Self(1);
    /// Limits sending snapshots.
    pub const SEND_SNAPSHOT: Self = Self(2);
    /// Limits leaders transferred into a store.
    pub const TRANSFER_LEADER_IN: Self = Self(3);

    /// Preserves a Go-compatible integer value, including unknown types.
    pub const fn from_i64(value: i64) -> Self {
        Self(value)
    }

    /// Returns the original integer value.
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

/// The number of known store limit types.
pub const STORE_LIMIT_TYPE_LEN: usize = 4;
/// The token bucket store limit version.
pub const VERSION_V1: &str = "v1";
/// The sliding window store limit version.
pub const VERSION_V2: &str = "v2";
/// Regions smaller than this threshold are considered small by callers.
pub const SMALL_REGION_THRESHOLD: i64 = 20;
/// A finite rate used by Go callers to represent an unlimited store limit.
pub const UNLIMITED: f64 = 100_000_000.0;

pub(crate) const INFLUENCE: i64 = 1000;
const SMALL_INFLUENCE: i64 = 200;

/// Normal region operator costs, indexed by the known store limit type.
pub const REGION_INFLUENCE: [i64; STORE_LIMIT_TYPE_LEN] = [INFLUENCE; STORE_LIMIT_TYPE_LEN];
/// Small region costs, defined only for adding and removing peers.
///
/// Snapshot and leader transfer types have no entry, matching Go's two-element slice.
pub const SMALL_REGION_INFLUENCE: [i64; 2] = [SMALL_INFLUENCE; 2];

/// The canonical names and values of all known store limit types.
pub const TYPE_NAME_VALUE: [(&str, Type); STORE_LIMIT_TYPE_LEN] = [
    ("add-peer", Type::ADD_PEER),
    ("remove-peer", Type::REMOVE_PEER),
    ("send-snapshot", Type::SEND_SNAPSHOT),
    ("transfer-leader-in", Type::TRANSFER_LEADER_IN),
];

/// Looks up an exact, case-sensitive name; unknown names have no value.
pub fn type_from_name(name: &str) -> Option<Type> {
    TYPE_NAME_VALUE
        .iter()
        .find_map(|(candidate, typ)| (*candidate == name).then_some(*typ))
}

impl fmt::Display for Type {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = TYPE_NAME_VALUE
            .iter()
            .find_map(|(name, typ)| (typ == self).then_some(*name))
            .unwrap_or("");
        formatter.write_str(name)
    }
}
