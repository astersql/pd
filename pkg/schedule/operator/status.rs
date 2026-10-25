// Copyright 2026 AsterSQL.

use std::{
    collections::BTreeMap,
    fmt,
    ops::{BitOr, BitOrAssign},
    str::FromStr,
    sync::{Arc, Mutex, MutexGuard, RwLock},
    time::{Duration, Instant},
};

/// A bit field identifying the producer and steps of an operator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpKind(pub u32);

const KIND_NAMES: [&str; 12] = [
    "admin",
    "affinity",
    "merge",
    "range",
    "replica",
    "split",
    "hot-region",
    "region",
    "leader",
    "witness-leader",
    "witness",
    "split-scatter",
];

impl OpKind {
    pub const ADMIN: Self = Self(1);
    pub const AFFINITY: Self = Self(1 << 1);
    pub const MERGE: Self = Self(1 << 2);
    pub const RANGE: Self = Self(1 << 3);
    pub const REPLICA: Self = Self(1 << 4);
    pub const SPLIT: Self = Self(1 << 5);
    pub const HOT_REGION: Self = Self(1 << 6);
    pub const REGION: Self = Self(1 << 7);
    pub const LEADER: Self = Self(1 << 8);
    pub const WITNESS_LEADER: Self = Self(1 << 9);
    pub const WITNESS: Self = Self(1 << 10);
    pub const SPLIT_SCATTER: Self = Self(1 << 11);
}

/// A rejected flag name, preserving the exact Go error text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownKind(pub String);
impl fmt::Display for UnknownKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown flag name: {}", self.0)
    }
}
impl std::error::Error for UnknownKind {}
impl FromStr for OpKind {
    type Err = UnknownKind;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut bits = 0;
        for flag in input.split(',') {
            // Go has a display name but no nameToFlag entry for witness.
            let Some(index) = KIND_NAMES
                .iter()
                .position(|name| *name == flag && flag != "witness")
            else {
                return Err(UnknownKind(flag.to_owned()));
            };
            bits |= 1 << index;
        }
        Ok(Self(bits))
    }
}
impl fmt::Display for OpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for (index, name) in KIND_NAMES.iter().enumerate() {
            if self.0 & (1 << index) != 0 {
                write!(f, "{separator}{name}")?;
                separator = ",";
            }
        }
        if separator.is_empty() {
            f.write_str("unknown")
        } else {
            Ok(())
        }
    }
}
impl BitOr for OpKind {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitOrAssign for OpKind {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// An operator status. Unknown raw values are safely rejected by transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpStatus(pub u32);
pub const CREATED: OpStatus = OpStatus(0);
pub const STARTED: OpStatus = OpStatus(1);
pub const SUCCESS: OpStatus = OpStatus(2);
pub const CANCELED: OpStatus = OpStatus(3);
pub const REPLACED: OpStatus = OpStatus(4);
pub const EXPIRED: OpStatus = OpStatus(5);
pub const TIMEOUT: OpStatus = OpStatus(6);
impl OpStatus {
    /// Returns true only for the five defined terminal states.
    pub fn is_end(self) -> bool {
        (2..7).contains(&self.0)
    }
    fn permits(self, destination: Self) -> bool {
        matches!(
            (self, destination),
            (CREATED, STARTED | CANCELED | EXPIRED)
                | (STARTED, SUCCESS | CANCELED | REPLACED | TIMEOUT)
        )
    }
}
impl fmt::Display for OpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            [
                "Created", "Started", "Success", "Canceled", "Replaced", "Expired", "Timeout",
            ]
            .get(self.0 as usize)
            .copied()
            .unwrap_or("Unknown"),
        )
    }
}

/// Monotonic time supplied by the host or a deterministic test boundary.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}
struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
struct TrackerState {
    current: OpStatus,
    times: [Option<Instant>; 3],
}
impl TrackerState {
    fn time_of(&self, status: OpStatus) -> Option<Instant> {
        match status.0 {
            0 | 1 => self.times[status.0 as usize],
            _ if status == self.current => self.times[2],
            _ => None,
        }
    }
    fn transition(&mut self, destination: OpStatus, now: Instant) -> bool {
        if !self.current.permits(destination) {
            return false;
        }
        self.current = destination;
        self.times[destination.0.min(2) as usize] = Some(now);
        true
    }
}

/// A synchronized status graph retaining created, started and one terminal time.
pub struct OpStatusTracker {
    state: Mutex<TrackerState>,
    clock: Arc<dyn Clock>,
}
impl Default for OpStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}
impl OpStatusTracker {
    /// Creates a tracker in CREATED using the system monotonic clock.
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }
    /// Creates a tracker with a host-provided monotonic clock.
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        let now = clock.now();
        Self {
            state: Mutex::new(TrackerState {
                current: CREATED,
                times: [Some(now), None, None],
            }),
            clock,
        }
    }
    fn lock(&self) -> MutexGuard<'_, TrackerState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub fn status(&self) -> OpStatus {
        self.lock().current
    }
    pub fn is_end(&self) -> bool {
        self.status().is_end()
    }
    /// Returns the recorded monotonic time of the current state.
    pub fn reach_time(&self) -> Instant {
        let state = self.lock();
        state
            .time_of(state.current)
            .expect("current status always has a reach time")
    }
    /// Returns None for unreached or unknown states.
    pub fn reach_time_of(&self, status: OpStatus) -> Option<Instant> {
        self.lock().time_of(status)
    }
    /// Atomically changes state and its time, or leaves both unchanged.
    pub fn to(&self, destination: OpStatus) -> bool {
        self.lock().transition(destination, self.clock.now())
    }
    /// Expires only CREATED operators once elapsed creation time is >= duration.
    /// Repeated checks return true for EXPIRED without changing its reach time.
    pub fn check_expired(&self, duration: Duration) -> bool {
        self.check_duration(CREATED, EXPIRED, duration)
    }
    /// Times out only STARTED operators, measuring from start rather than creation.
    /// Equality and zero duration trigger timeout; other terminal states are preserved.
    pub fn check_timeout(&self, duration: Duration) -> bool {
        self.check_duration(STARTED, TIMEOUT, duration)
    }
    fn check_duration(&self, from: OpStatus, destination: OpStatus, duration: Duration) -> bool {
        let mut state = self.lock();
        if state.current == from {
            let now = self.clock.now();
            let reached = state
                .time_of(from)
                .expect("current status always has a reach time");
            if now.saturating_duration_since(reached) < duration {
                return false;
            }
            return state.transition(destination, now);
        }
        state.current == destination
    }
}
impl fmt::Display for OpStatusTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.status().fmt(f)
    }
}

pub const RELATED_MERGE_REGION: &str = "related merge region";
/// Thread-safe additional metadata for embedding in the canonical Operator.
#[derive(Default)]
pub struct AdditionalInfo {
    values: RwLock<BTreeMap<String, String>>,
}
impl AdditionalInfo {
    pub fn set_if_absent(&self, key: &str, value: &str) {
        self.values
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.to_owned())
            .or_insert_with(|| value.to_owned());
    }
    pub fn set(&self, key: impl Into<String>, value: impl Into<String>) {
        self.values
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.into(), value.into());
    }
    pub fn get(&self, key: &str) -> Option<String> {
        self.values
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
    }
    /// Returns empty for no metadata; otherwise sorted JSON with Go HTML escaping.
    pub fn log(&self) -> String {
        let values = self
            .values
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if values.is_empty() {
            return String::new();
        }
        serde_json::to_string(&*values)
            .expect("string map is serializable")
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029")
    }
    pub fn has_related_merge_region(&self) -> bool {
        self.get(RELATED_MERGE_REGION)
            .is_some_and(|value| !value.is_empty())
    }
    /// Returns zero for missing, empty, nondecimal or overflowing values.
    pub fn related_merge_region(&self) -> u64 {
        self.get(RELATED_MERGE_REGION)
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    }
}
