// Copyright 2026 AsterSQL.

use std::fmt;

/// An operator priority. Lower values have higher priority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct PriorityLevel(i32);

impl PriorityLevel {
    pub const LOW: Self = Self(0);
    pub const MEDIUM: Self = Self(1);
    pub const HIGH: Self = Self(2);
    pub const URGENT: Self = Self(3);
    pub const LEN: Self = Self(4);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }

    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for PriorityLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match *self {
            Self::LOW => "low",
            Self::MEDIUM => "medium",
            Self::HIGH => "high",
            Self::URGENT => "urgent",
            _ => "unknown",
        })
    }
}

/// The resource measured by a scheduler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct ResourceKind(i32);

impl ResourceKind {
    pub const LEADER: Self = Self(0);
    pub const REGION: Self = Self(1);
    pub const WITNESS: Self = Self(2);
    pub const LEN: Self = Self(3);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }

    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match *self {
            Self::LEADER => "leader",
            Self::REGION => "region",
            Self::WITNESS => "witness",
            _ => "unknown",
        })
    }
}

/// The policy used to measure a scheduled resource.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct SchedulePolicy(i32);

impl SchedulePolicy {
    pub const BY_COUNT: Self = Self(0);
    pub const BY_SIZE: Self = Self(1);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }

    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for SchedulePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match *self {
            Self::BY_COUNT => "count",
            Self::BY_SIZE => "size",
            _ => "unknown",
        })
    }
}

/// Converts a Go-compatible schedule policy string into its policy value.
///
/// # Panics
///
/// Panics when `input` is neither `"count"` nor `"size"`.
pub fn string_to_schedule_policy(input: &str) -> SchedulePolicy {
    match input {
        "count" => SchedulePolicy::BY_COUNT,
        "size" => SchedulePolicy::BY_SIZE,
        _ => panic!("invalid schedule policy: {input}"),
    }
}

/// The encoding used for keys in a keyspace.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct KeyType(i32);

impl KeyType {
    pub const TABLE: Self = Self(0);
    pub const RAW: Self = Self(1);
    pub const TXN: Self = Self(2);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }

    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for KeyType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match *self {
            Self::TABLE => "table",
            Self::RAW => "raw",
            Self::TXN => "txn",
            _ => "unknown",
        })
    }
}

/// Converts a Go-compatible key type string into its key type value.
///
/// Empty or whitespace-only input selects [`KeyType::TABLE`], the Go default.
///
/// # Panics
///
/// Panics when the trimmed input is not `"table"`, `"raw"`, `"txn"`, or empty.
pub fn string_to_key_type(input: &str) -> KeyType {
    match input.trim() {
        "table" | "" => KeyType::TABLE,
        "raw" => KeyType::RAW,
        "txn" => KeyType::TXN,
        _ => panic!("invalid key type: {input}"),
    }
}

/// The direction of a scheduling operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Direction(i32);

impl Direction {
    pub const IN: Self = Self(0);
    pub const OUT: Self = Self(1);

    pub const fn from_i32(value: i32) -> Self {
        Self(value)
    }

    pub const fn as_i32(self) -> i32 {
        self.0
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match *self {
            Self::IN => "in",
            Self::OUT => "out",
            _ => "unknown",
        })
    }
}

/// A resource and the policy used to schedule it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScheduleKind {
    pub resource: ResourceKind,
    pub policy: SchedulePolicy,
}

impl ScheduleKind {
    pub const fn new(resource: ResourceKind, policy: SchedulePolicy) -> Self {
        Self { resource, policy }
    }
}

#[cfg(test)]
mod kind_test;
