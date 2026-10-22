// Copyright 2026 AsterSQL.
//! Store HTTP responses with owned metadata and Go-compatible omission rules.
use astersql_pd_core::{NodeState, StoreInfo as CoreStore, StoreMeta, StoreState};
use astersql_pd_core_constant::string_to_schedule_policy;
use astersql_pd_typeutil::{ByteSize, Duration};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::collections::HashMap;
fn zero<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}
fn finite<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if !v.is_finite() {
        return Err(serde::ser::Error::custom("unsupported non-finite float"));
    }
    s.serialize_f64(*v)
}
/// Only the scheduling values read by Go BuildStoreInfo; no scheduler dependency.
#[derive(Clone, Debug)]
pub struct StoreResponseConfig {
    pub leader_schedule_policy: String,
    pub region_score_formula_version: String,
    pub high_space_ratio: f64,
    pub low_space_ratio: f64,
}
impl Default for StoreResponseConfig {
    fn default() -> Self {
        Self {
            leader_schedule_policy: "count".into(),
            region_score_formula_version: "v2".into(),
            high_space_ratio: 0.7,
            low_space_ratio: 0.8,
        }
    }
}
/// Flattened metapb.Store-compatible metadata with the response state name.
#[derive(Clone, Debug)]
pub struct MetaStore {
    pub meta: Option<StoreMeta>,
    pub state_name: String,
}
impl Serialize for MetaStore {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("state_name", &self.state_name)?;
        if let Some(m) = &self.meta {
            macro_rules! field {
                ($name:ident) => {
                    if !zero(&m.$name) {
                        map.serialize_entry(stringify!($name), &m.$name)?;
                    }
                };
            }
            field!(id);
            field!(address);
            field!(version);
            field!(status_address);
            field!(peer_address);
            field!(git_hash);
            field!(start_timestamp);
            field!(deploy_path);
            field!(last_heartbeat);
            field!(physically_destroyed);
            let state = match m.state {
                StoreState::Up => 0,
                StoreState::Offline => 1,
                StoreState::Tombstone => 2,
                StoreState::Unknown(n) => n,
            };
            if state != 0 {
                map.serialize_entry("state", &state)?;
            }
            let node = match m.node_state {
                NodeState::Preparing => 0,
                NodeState::Serving => 1,
                NodeState::Removing => 2,
                NodeState::Removed => 3,
                NodeState::Unknown(n) => n,
            };
            if node != 0 {
                map.serialize_entry("node_state", &node)?;
            }
            if !m.labels.is_empty() {
                let labels: Vec<_> = m
                    .labels
                    .iter()
                    .map(|l| {
                        let mut m = serde_json::Map::new();
                        if !l.key.is_empty() {
                            m.insert("key".into(), l.key.clone().into());
                        }
                        if !l.value.is_empty() {
                            m.insert("value".into(), l.value.clone().into());
                        }
                        m
                    })
                    .collect();
                map.serialize_entry("labels", &labels)?;
            }
            if let Some(r) = &m.txn_protocol_version_range {
                let mut fields = serde_json::Map::new();
                if r.min != 0 {
                    fields.insert("min".into(), r.min.into());
                }
                if r.max != 0 {
                    fields.insert("max".into(), r.max.into());
                }
                map.serialize_entry("txn_protocol_version_range", &fields)?;
            }
        }
        map.end()
    }
}
/// RFC3339 nanosecond timestamp, normalized to UTC (Go local zone is normalized).
#[derive(Clone, Copy, Debug)]
pub struct StoreTimestamp(pub i128);
impl Serialize for StoreTimestamp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let seconds = self.0.div_euclid(1_000_000_000);
        let nanos = self.0.rem_euclid(1_000_000_000);
        let days = seconds.div_euclid(86400);
        let daytime = seconds.rem_euclid(86400);
        // Proleptic Gregorian civil date from days since Unix epoch.
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let mut year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        year += i128::from(month <= 2);
        if !(0..=9999).contains(&year) {
            return Err(serde::ser::Error::custom(
                "timestamp year outside RFC3339 range",
            ));
        }
        let fraction = if nanos == 0 {
            String::new()
        } else {
            format!(".{nanos:09}").trim_end_matches('0').to_owned()
        };
        s.serialize_str(&format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{fraction}Z",
            daytime / 3600,
            daytime / 60 % 60,
            daytime % 60
        ))
    }
}
/// Slow trend retains even a present all-zero report.
#[derive(Clone, Debug, Serialize)]
pub struct SlowTrend {
    #[serde(serialize_with = "finite")]
    pub cause_value: f64,
    #[serde(serialize_with = "finite")]
    pub cause_rate: f64,
    #[serde(serialize_with = "finite")]
    pub result_value: f64,
    #[serde(serialize_with = "finite")]
    pub result_rate: f64,
}
/// Complete store status; required zeros and optional presence follow Go tags.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StoreStatus {
    pub capacity: ByteSize,
    pub available: ByteSize,
    pub used_size: ByteSize,
    pub leader_count: i64,
    #[serde(serialize_with = "finite")]
    pub leader_weight: f64,
    #[serde(serialize_with = "finite")]
    pub leader_score: f64,
    pub leader_size: i64,
    pub region_count: i64,
    #[serde(serialize_with = "finite")]
    pub region_weight: f64,
    #[serde(serialize_with = "finite")]
    pub region_score: f64,
    pub region_size: i64,
    #[serde(skip_serializing_if = "zero")]
    pub learner_count: i64,
    #[serde(skip_serializing_if = "zero")]
    pub witness_count: i64,
    #[serde(skip_serializing_if = "zero")]
    pub pending_peer_count: i64,
    #[serde(skip_serializing_if = "zero")]
    pub slow_score: u64,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub network_slow_score: HashMap<u64, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow_trend: Option<SlowTrend>,
    #[serde(skip_serializing_if = "zero")]
    pub sending_snap_count: u32,
    #[serde(skip_serializing_if = "zero")]
    pub receiving_snap_count: u32,
    #[serde(skip_serializing_if = "zero")]
    pub is_busy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ts: Option<StoreTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_ts: Option<StoreTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime: Option<Duration>,
    #[serde(skip_serializing_if = "zero")]
    pub pause_leader_transfer_in: bool,
    #[serde(skip_serializing_if = "zero")]
    pub pause_leader_transfer_out: bool,
}
/// A nullable store and status match independently nullable Go pointers.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StoreInfo {
    pub store: Option<MetaStore>,
    pub status: Option<StoreStatus>,
}
/// Preserve nil versus allocated empty slices, and nil store entries.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StoresInfo {
    pub count: i64,
    pub stores: Option<Vec<Option<StoreInfo>>>,
}
/// Build from the canonical core store. Core owns an always-present raw report.
pub fn build_store_info(opt: &StoreResponseConfig, store: &CoreStore) -> StoreInfo {
    let raw = store.store_stats();
    let state_name = match store.state() {
        StoreState::Up => "Up".into(),
        StoreState::Offline => "Offline".into(),
        StoreState::Tombstone => "Tombstone".into(),
        StoreState::Unknown(n) => n.to_string(),
    };
    let policy = string_to_schedule_policy(&opt.leader_schedule_policy);
    let mut status = StoreStatus {
        capacity: ByteSize::from_bytes(store.capacity()),
        available: ByteSize::from_bytes(store.available()),
        used_size: ByteSize::from_bytes(store.used_size()),
        leader_count: store.leader_count(),
        leader_weight: store.leader_weight(),
        leader_score: store.leader_score(policy, 0),
        leader_size: store.leader_size(),
        region_count: store.region_count(),
        region_weight: store.region_weight(),
        region_score: store.region_score(
            &opt.region_score_formula_version,
            opt.high_space_ratio,
            opt.low_space_ratio,
            0,
        ),
        region_size: store.region_size(),
        learner_count: store.learner_count(),
        witness_count: store.witness_count(),
        pending_peer_count: store.pending_peer_count(),
        slow_score: store.slow_score(),
        network_slow_score: store.network_slow_scores(),
        sending_snap_count: store.sending_snap_count(),
        receiving_snap_count: store.receiving_snap_count(),
        is_busy: store.is_busy(),
        pause_leader_transfer_in: !store.allow_leader_transfer_in(),
        pause_leader_transfer_out: !store.allow_leader_transfer_out(),
        start_ts: Some(StoreTimestamp(
            store.start_time_unix_seconds() as i128 * 1_000_000_000,
        )),
        last_heartbeat_ts: Some(StoreTimestamp(store.last_heartbeat_unix_nanos() as i128)),
        uptime: None,
        slow_trend: raw.slow_trend.map(|t| SlowTrend {
            cause_value: t.cause_value,
            cause_rate: t.cause_rate,
            result_value: t.result_value,
            result_rate: t.result_rate,
        }),
    };
    let uptime = store.uptime_nanos();
    if uptime > 0 {
        status.uptime = Some(Duration::from_nanos(uptime));
    }
    let meta = MetaStore {
        meta: store.meta().cloned(),
        state_name,
    };
    StoreInfo {
        store: Some(meta),
        status: Some(status),
    }
}
