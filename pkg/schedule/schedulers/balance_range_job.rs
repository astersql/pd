// Copyright 2026 AsterSQL.
//! Persisted balance-range job queue. The host owns scheduling and the clock.
use crate::{SchedulerConfig, SchedulerError, get_key_ranges};
use astersql_pd_core::Rule;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::{KeyRange, decode_http_key_ranges};
use astersql_pd_storage_endpoint::StorageEndpoint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::{str::FromStr, sync::Arc};

pub const RANGE_JOB_TIMEOUT: i64 = 1_800_000_000_000;
pub const RANGE_JOB_RETENTION: i128 = 604_800_000_000_000;

/// Unknown JSON tokens leave a fresh Go status at pending. Explicit raw updates
/// preserve an existing status, including for unknown strings and nonstrings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Finished,
    Cancelled,
    Unknown(i32),
}
impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Cancelled => "cancelled",
            Self::Unknown(_) => "unknown",
        }
    }
    pub fn unmarshal_json(&mut self, raw: &[u8]) {
        *self = match raw {
            b"\"pending\"" => Self::Pending,
            b"\"running\"" => Self::Running,
            b"\"finished\"" => Self::Finished,
            b"\"cancelled\"" => Self::Cancelled,
            _ => *self,
        };
    }
}
impl Serialize for JobStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for JobStatus {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Box::<serde_json::value::RawValue>::deserialize(d)?;
        let mut status = Self::default();
        status.unmarshal_json(v.get().as_bytes());
        Ok(status)
    }
}
/// Signed Unix nanoseconds, encoded as RFC3339Nano. Zero-value Go time is year 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobTime(pub i128);
impl Default for JobTime {
    fn default() -> Self {
        Self(-62_135_596_800_000_000_000)
    }
}
impl Serialize for JobTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let seconds =
            i64::try_from(self.0.div_euclid(1_000_000_000)).map_err(serde::ser::Error::custom)?;
        let time =
            chrono::DateTime::from_timestamp(seconds, self.0.rem_euclid(1_000_000_000) as u32)
                .ok_or_else(|| serde::ser::Error::custom("invalid job time"))?;
        let text = time.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let text = text
            .trim_end_matches('Z')
            .trim_end_matches('0')
            .trim_end_matches('.');
        s.serialize_str(&format!("{text}Z"))
    }
}
impl<'de> Deserialize<'de> for JobTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let Some(text) = Option::<String>::deserialize(d)? else {
            return Ok(Self::default());
        };
        let t = chrono::DateTime::parse_from_rfc3339(&text).map_err(serde::de::Error::custom)?;
        Ok(Self(
            i128::from(t.timestamp()) * 1_000_000_000 + i128::from(t.timestamp_subsec_nanos()),
        ))
    }
}
mod rule_json {
    use super::*;
    pub fn serialize<S: Serializer>(rule: &Rule, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&rule.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Rule, D::Error> {
        let value = Box::<serde_json::value::RawValue>::deserialize(d)?;
        Ok(Rule::unmarshal_json(value.get().as_bytes()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BalanceRangeJob {
    #[serde(rename = "job-id")]
    pub job_id: u64,
    #[serde(with = "rule_json")]
    pub rule: Rule,
    pub engine: String,
    /// Go time.Duration is a signed integer number of nanoseconds in this schema.
    pub timeout: i64,
    pub ranges: Vec<KeyRange>,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<JobTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<JobTime>,
    pub create: JobTime,
    pub status: JobStatus,
}
impl Default for BalanceRangeJob {
    fn default() -> Self {
        Self {
            job_id: 0,
            rule: Rule::LeaderScatter,
            engine: String::new(),
            timeout: 0,
            ranges: vec![],
            alias: String::new(),
            start: None,
            finish: None,
            create: JobTime::default(),
            status: JobStatus::Pending,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum RangeJobError {
    #[error("invalid job argument: {0}")]
    Argument(String),
    #[error("job already exists")]
    Duplicate,
    #[error("job {0} does not exist")]
    NotFound(u64),
    #[error("job {0} has been completed and cannot be cancelled")]
    Complete(u64),
    #[error("the job is not {0}")]
    State(&'static str),
    #[error("job index out of bounds")]
    Index,
    #[error(transparent)]
    Persistence(#[from] SchedulerError),
}
fn argument(value: impl ToString) -> RangeJobError {
    RangeJobError::Argument(value.to_string())
}
fn duration(text: &str) -> Result<i64, RangeJobError> {
    let n = astersql_pd_typeutil::Duration::from_str(text)
        .map_err(argument)?
        .as_nanos();
    if n <= 0 {
        return Err(argument("timeout must be positive"));
    }
    Ok(n)
}
fn ranges(args: &[&str]) -> Result<Vec<KeyRange>, RangeJobError> {
    Ok(get_key_ranges(args)
        .map_err(argument)?
        .into_iter()
        .map(|r| KeyRange::new(r.start_key, r.end_key))
        .collect())
}
fn unescape(s: &str) -> Result<String, RangeJobError> {
    let ranges = get_key_ranges(&[s, ""]).map_err(argument)?;
    String::from_utf8(ranges[0].start_key.clone()).map_err(argument)
}
impl BalanceRangeJob {
    pub fn is_complete(&self) -> bool {
        matches!(self.status, JobStatus::Finished | JobStatus::Cancelled)
    }
    pub fn expired(&self, now: JobTime, retention: i128) -> bool {
        self.finish
            .is_some_and(|finish| now.0.saturating_sub(finish.0) > retention)
    }
    pub fn should_finish(&self, now: JobTime) -> Result<bool, RangeJobError> {
        let start = self.start.ok_or(RangeJobError::State("started"))?;
        Ok(now.0.saturating_sub(start.0) > i128::from(self.timeout))
    }
    /// Slice decoder permits an empty alias and ignores an unmatched final range.
    pub fn from_args(args: &[&str], now: JobTime) -> Result<Self, RangeJobError> {
        if args.len() < 5 {
            return Err(argument("args length must be greater than 4"));
        }
        let rule = Rule::new(&unescape(args[0])?);
        let engine = unescape(args[1])?;
        Self::validate(rule, &engine)?;
        Ok(Self {
            rule,
            engine,
            timeout: duration(&unescape(args[2])?)?,
            alias: unescape(args[3])?,
            ranges: ranges(&args[4..])?,
            create: now,
            ..Default::default()
        })
    }
    fn validate(rule: Rule, engine: &str) -> Result<(), RangeJobError> {
        if rule == Rule::Unknown {
            return Err(argument("invalid rule"));
        }
        if engine != "tikv" && engine != "tiflash" {
            return Err(argument("invalid engine"));
        }
        Ok(())
    }
    /// HTTP adapters pass decoded JSON here and map typed errors to responses.
    pub fn from_http(
        input: &serde_json::Map<String, Value>,
        now: JobTime,
    ) -> Result<Self, RangeJobError> {
        let field = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| argument(key))
        };
        let engine = field("engine")?.to_owned();
        let rule = Rule::new(field("rule")?);
        Self::validate(rule, &engine)?;
        let alias = field("alias")?.to_owned();
        let timeout = match input.get("timeout") {
            None => RANGE_JOB_TIMEOUT,
            Some(v) => match v.as_str() {
                Some("") => RANGE_JOB_TIMEOUT,
                Some(s) => duration(s)?,
                None => return Err(argument("timeout must be a string")),
            },
        };
        let keys = decode_http_key_ranges(input).map_err(argument)?;
        // Go applies QueryUnescape again in getKeyRanges after HTTP decoding.
        let refs: Vec<_> = keys
            .iter()
            .map(|k| {
                k.iter()
                    .map(|&b| {
                        if b >= 128 {
                            format!("%{b:02X}")
                        } else {
                            char::from(b).to_string()
                        }
                    })
                    .collect::<String>()
            })
            .collect();
        let refs: Vec<_> = refs.iter().map(String::as_str).collect();
        Ok(Self {
            rule,
            engine,
            alias,
            timeout,
            ranges: ranges(&refs)?,
            create: now,
            ..Default::default()
        })
    }
}
/// Exclusive access spans validation, I/O and publication. Failed or cancelled saves
/// retain the local snapshot; rebuilding loads the last successful durable queue.
pub struct BalanceRangeJobs {
    config: SchedulerConfig<Vec<BalanceRangeJob>>,
}
impl BalanceRangeJobs {
    pub fn new(storage: Arc<StorageEndpoint>, paths: ClusterPaths) -> Self {
        Self {
            config: SchedulerConfig::new("balance-range-scheduler".into(), storage, paths, vec![]),
        }
    }
    pub fn jobs(&self) -> &[BalanceRangeJob] {
        self.config.get()
    }
    pub fn peek(&self) -> Option<(usize, &BalanceRangeJob)> {
        self.jobs()
            .iter()
            .enumerate()
            .find(|(_, j)| !j.is_complete())
    }
    pub async fn reload(&mut self) -> Result<(), RangeJobError> {
        self.config.reload().await?;
        Ok(())
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, RangeJobError> {
        Ok(crate::encode_config(self.config.get())?)
    }
    /// Factory initialization does not save and starts IDs at zero, unlike add.
    pub fn from_args(
        storage: Arc<StorageEndpoint>,
        paths: ClusterPaths,
        args: &[&str],
        now: JobTime,
    ) -> Result<Self, RangeJobError> {
        let job = BalanceRangeJob::from_args(args, now)?;
        Ok(Self {
            config: SchedulerConfig::new(
                "balance-range-scheduler".into(),
                storage,
                paths,
                vec![job],
            ),
        })
    }
    pub async fn add(&mut self, mut job: BalanceRangeJob) -> Result<u64, RangeJobError> {
        if self
            .jobs()
            .iter()
            .any(|j| !j.is_complete() && j.alias == job.alias)
        {
            return Err(RangeJobError::Duplicate);
        }
        job.status = JobStatus::Pending;
        job.job_id = self.jobs().last().map_or(1, |j| j.job_id.wrapping_add(1));
        let id = job.job_id;
        let mut jobs = self.jobs().to_vec();
        jobs.push(job);
        self.config.update(jobs).await?;
        Ok(id)
    }
    pub async fn delete(&mut self, id: u64, now: JobTime) -> Result<(), RangeJobError> {
        let mut jobs = self.jobs().to_vec();
        let job = jobs
            .iter_mut()
            .find(|j| j.job_id == id)
            .ok_or(RangeJobError::NotFound(id))?;
        if job.is_complete() {
            return Err(RangeJobError::Complete(id));
        }
        job.status = JobStatus::Cancelled;
        job.start.get_or_insert(now);
        job.finish = Some(now);
        self.config.update(jobs).await?;
        Ok(())
    }
    pub async fn begin(&mut self, index: usize, now: JobTime) -> Result<(), RangeJobError> {
        self.transition(index, now, false).await
    }
    pub async fn finish(&mut self, index: usize, now: JobTime) -> Result<(), RangeJobError> {
        self.transition(index, now, true).await
    }
    async fn transition(
        &mut self,
        index: usize,
        now: JobTime,
        finish: bool,
    ) -> Result<(), RangeJobError> {
        let mut jobs = self.jobs().to_vec();
        let job = jobs.get_mut(index).ok_or(RangeJobError::Index)?;
        if job.status
            != if finish {
                JobStatus::Running
            } else {
                JobStatus::Pending
            }
        {
            return Err(RangeJobError::State(if finish {
                "running"
            } else {
                "pending"
            }));
        }
        if finish {
            job.status = JobStatus::Finished;
            job.finish = Some(now);
        } else {
            job.status = JobStatus::Running;
            job.start = Some(now);
        }
        self.config.update(jobs).await?;
        Ok(())
    }
    pub async fn gc(&mut self, now: JobTime) -> Result<usize, RangeJobError> {
        let count = self
            .jobs()
            .iter()
            .take_while(|j| j.is_complete() && j.expired(now, RANGE_JOB_RETENTION))
            .count();
        if count > 0 {
            self.config.update(self.jobs()[count..].to_vec()).await?;
        }
        Ok(count)
    }
}
