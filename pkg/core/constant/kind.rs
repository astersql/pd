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
