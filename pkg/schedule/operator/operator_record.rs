// Copyright 2026 AsterSQL.
//! Observability over the canonical operator; timestamp rendering belongs to the host.
use crate::*;
use astersql_pd_core_constant::ResourceKind;
use serde_json::{Value, json};
use std::time::Instant;

/// Stable cancellation reasons used by schedulers and controllers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    RegionNotFound,
    EpochNotMatch,
    AlreadyExist,
    AdminStop,
    NotInRunningState,
    Timeout,
    Expired,
    NotInCreateStatus,
    StaleStatus,
    ExceedStoreLimit,
    ExceedWaitLimit,
    RelatedMergeRegion,
    StartFailed,
    Unknown,
}
impl CancelReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegionNotFound => "region not found",
            Self::EpochNotMatch => "epoch not match",
            Self::AlreadyExist => "already exist",
            Self::AdminStop => "admin stop",
            Self::NotInRunningState => "not in running state",
            Self::Timeout => "timeout",
            Self::Expired => "expired",
            Self::NotInCreateStatus => "not in create status",
            Self::StaleStatus => "stale status",
            Self::ExceedStoreLimit => "exceed store limit",
            Self::ExceedWaitLimit => "exceed wait limit",
            Self::RelatedMergeRegion => "related merge region",
            Self::StartFailed => "start failed",
            Self::Unknown => "unknown",
        }
    }
}
/// A completed move, with one shared observation timestamp per history call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpHistory {
    pub finish_time: Instant,
    pub from: u64,
    pub to: u64,
    pub kind: ResourceKind,
}
/// A live reference to an operator with fixed finish time and signed duration.
/// None preserves Go's unreached zero start; the wall-clock adapter resolves it.
pub struct OpRecord<'a> {
    pub operator: &'a Operator,
    pub finish_time: Instant,
    pub duration_nanos: Option<i64>,
}
fn signed_duration(end: Instant, start: Instant) -> i64 {
    if end >= start {
        end.duration_since(start).as_nanos().min(i64::MAX as u128) as i64
    } else {
        i64::try_from(start.duration_since(end).as_nanos()).map_or(i64::MIN, |n| -n)
    }
}
fn duration(nanos: i64) -> String {
    let sign = if nanos < 0 { "-" } else { "" };
    let n = nanos.unsigned_abs();
    if n == 0 {
        return "0s".into();
    }
    if n < 1_000_000_000 {
        let (scale, unit) = if n < 1_000 {
            (1, "ns")
        } else if n < 1_000_000 {
            (1_000, "µs")
        } else {
            (1_000_000, "ms")
        };
        return format!("{sign}{}{unit}", decimal(n, scale));
    }
    let sec = n / 1_000_000_000;
    let h = sec / 3600;
    let m = sec % 3600 / 60;
    format!(
        "{sign}{}{}{}s",
        if h > 0 {
            format!("{h}h")
        } else {
            String::new()
        },
        if h > 0 || m > 0 {
            format!("{m}m")
        } else {
            String::new()
        },
        decimal(n % 60_000_000_000, 1_000_000_000)
    )
}
fn decimal(n: u64, scale: u64) -> String {
    let whole = n / scale;
    let rem = n % scale;
    if rem == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{:0width$}", rem, width = scale.ilog10() as usize)
            .trim_end_matches('0')
            .to_owned()
    }
}
impl Operator {
    pub fn scheduler_kind(&self) -> OpKind {
        let k = self.kind().0;
        if k & OpKind::ADMIN.0 != 0 {
            OpKind::ADMIN
        } else if k & OpKind::SPLIT_SCATTER.0 != 0 {
            OpKind::SPLIT_SCATTER
        } else {
            OpKind(k & k.wrapping_neg())
        }
    }
    pub fn history_at(&self, now: Instant) -> Vec<OpHistory> {
        let mut histories = Vec::new();
        let mut adds = Vec::new();
        let mut removes = Vec::new();
        for i in 0..self.len() {
            match self.step(i).expect("in bounds") {
                OpStep::TransferLeader(s) => histories.push(OpHistory {
                    finish_time: now,
                    from: s.from_store,
                    to: s.to_store,
                    kind: ResourceKind::LEADER,
                }),
                OpStep::AddPeer(s) => adds.push(s.to_store),
                OpStep::AddLearner(s) => adds.push(s.to_store),
                OpStep::RemovePeer(s) => removes.push(s.from_store),
                _ => {}
            }
        }
        histories.extend(adds.into_iter().zip(removes).map(|(to, from)| OpHistory {
            finish_time: now,
            from,
            to,
            kind: ResourceKind::REGION,
        }));
        histories
    }
    pub fn history(&self) -> Vec<OpHistory> {
        self.history_at(Instant::now())
    }
    pub fn record(&self, finish_time: Instant) -> OpRecord<'_> {
        let current = self.current_step();
        let start = if self.status() != SUCCESS && current > 0 {
            self.step_finish_time(current - 1)
        } else {
            self.reach_time_of(STARTED)
        };
        OpRecord {
            operator: self,
            finish_time,
            duration_nanos: start.map(|s| signed_duration(finish_time, s)),
        }
    }
    /// Matches OpObject's JSON contract, including side effects of success/timeout checks.
    pub fn to_json_object(&self) -> Value {
        self.check_success();
        self.check_timeout();
        let epoch = self.region_epoch().map(|e| {
            let mut obj = serde_json::Map::new();
            if e.conf_ver != 0 {
                obj.insert("conf_ver".into(), json!(e.conf_ver));
            }
            if e.version != 0 {
                obj.insert("version".into(), json!(e.version));
            }
            Value::Object(obj)
        });
        json!({"desc":self.desc(),"brief":self.brief(),"region_id":self.region_id(),"region_epoch":epoch,"kind":self.kind().0,"timeout":duration(self.timeout_nanos()),"status":self.status().0})
    }
    /// Host renders monotonic observations as wall times; None denotes Go's zero time.
    pub fn render(&self, time: &impl Fn(Option<Instant>) -> String) -> String {
        let steps = (0..self.len())
            .map(|i| format!("{i}:{{{}}}", self.step(i).unwrap()))
            .collect::<Vec<_>>()
            .join(", ");
        let epoch = self.region_epoch();
        let mut s = format!(
            "{} {{{}}} (kind:{}, region:{}({}, {}), createAt:{}, startAt:{}, currentStep:{}, size:{}, steps:[{}], timeout:[{}])",
            self.desc(),
            self.brief(),
            self.kind(),
            self.region_id(),
            epoch.map_or(0, |e| e.version),
            epoch.map_or(0, |e| e.conf_ver),
            time(self.reach_time_of(CREATED)),
            time(self.reach_time_of(STARTED)),
            self.current_step(),
            self.approximate_size(),
            steps,
            duration(self.timeout_nanos())
        );
        if self.check_success() {
            s.push_str(" finished");
        }
        if self.check_timeout() {
            s.push_str(" timeout");
        }
        s
    }
    pub fn json_string(&self, time: &impl Fn(Option<Instant>) -> String) -> String {
        format!("\"{}\"", self.render(time))
    }
}
impl OpRecord<'_> {
    /// Resolve an unreached start using the host's elapsed wall duration since Go year 1.
    pub fn with_zero_start_duration(mut self, nanos: i64) -> Self {
        if self.duration_nanos.is_none() {
            self.duration_nanos = Some(nanos);
        }
        self
    }
    pub fn render(
        &self,
        time: &impl Fn(Option<Instant>) -> String,
    ) -> Result<String, UnreachedStart> {
        let nanos = self.duration_nanos.ok_or(UnreachedStart)?;
        Ok(format!(
            "{} (finishAt:{}, duration:{})",
            self.operator.render(time),
            time(Some(self.finish_time)),
            duration(nanos)
        ))
    }
    pub fn json_string(
        &self,
        time: &impl Fn(Option<Instant>) -> String,
    ) -> Result<String, UnreachedStart> {
        Ok(format!("\"{}\"", self.render(time)?))
    }
}
impl std::fmt::Display for OpStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TransferLeader(s) => std::fmt::Display::fmt(s, f),
            Self::AddPeer(s) => std::fmt::Display::fmt(s, f),
            Self::AddLearner(s) => std::fmt::Display::fmt(s, f),
            Self::PromoteLearner(s) => std::fmt::Display::fmt(s, f),
            Self::RemovePeer(s) => std::fmt::Display::fmt(s, f),
            Self::BecomeWitness(s) => std::fmt::Display::fmt(s, f),
            Self::BecomeNonWitness(s) => std::fmt::Display::fmt(s, f),
            Self::BatchSwitchWitness(s) => std::fmt::Display::fmt(s, f),
            Self::ChangePeerV2Enter(s) => std::fmt::Display::fmt(s, f),
            Self::ChangePeerV2Leave(s) => std::fmt::Display::fmt(s, f),
            Self::MergeRegion(s) => std::fmt::Display::fmt(s, f),
            Self::SplitRegion(s) => std::fmt::Display::fmt(s, f),
        }
    }
}

/// A wall-clock adapter must resolve the duration from Go zero time before rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnreachedStart;
impl std::fmt::Display for UnreachedStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("operator start time has not been reached")
    }
}
impl std::error::Error for UnreachedStart {}
