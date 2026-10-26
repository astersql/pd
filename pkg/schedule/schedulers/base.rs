// Copyright 2026 AsterSQL.
//! Protocol-free scheduler base and JSON configuration boundary.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_core::SchedulerCluster;
use astersql_pd_schedule_operator::Operator;
use astersql_pd_schedule_plan::BalanceSchedulerPlan;
use astersql_pd_schedule_types::{CheckerSchedulerType, current_kernel_default_schedulers};
use astersql_pd_storage_endpoint::{EndpointError, StorageEndpoint};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

/// Intervals use signed nanoseconds, matching Go time.Duration (including negatives).
pub const MAX_SCHEDULE_INTERVAL: i64 = 5_000_000_000;
pub const MIN_SCHEDULE_INTERVAL: i64 = 10_000_000;
pub const MIN_SLOW_SCHEDULE_INTERVAL: i64 = 3_000_000_000;
pub const SCHEDULE_INTERVAL_FACTOR: f64 = 1.3;

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("json marshal: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("json unmarshal: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("scheduler/config length mismatch")]
    PayloadLength,
    #[error("scheduler create function not registered: {0}")]
    NotRegistered(CheckerSchedulerType),
    #[error("the config decoder do not register for {0}")]
    DecoderNotRegistered(CheckerSchedulerType),
    #[error("unsupported scheduler action: {0}")]
    Unsupported(&'static str),
    #[error(transparent)]
    Storage(#[from] EndpointError),
}

#[derive(Clone, Copy, Debug)]
pub enum IntervalGrowth {
    Exponential,
    Linear,
    Zero,
}

/// Grow then cap; zero growth intentionally does not apply the cap.
pub fn interval_grow(interval: i64, max: i64, growth: IntervalGrowth) -> i64 {
    match growth {
        IntervalGrowth::Exponential => {
            ((interval as f64 * SCHEDULE_INTERVAL_FACTOR) as i64).min(max)
        }
        IntervalGrowth::Linear => interval.wrapping_add(MIN_SLOW_SCHEDULE_INTERVAL).min(max),
        IntervalGrowth::Zero => interval,
    }
}

pub fn encode_config<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, SchedulerError> {
    let json = serde_json::to_string(value).map_err(SchedulerError::Encode)?;
    // encoding/json escapes HTML-sensitive characters and JS line separators.
    Ok(json
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
        .into_bytes())
}
pub fn decode_config<T: DeserializeOwned>(data: &[u8]) -> Result<T, SchedulerError> {
    serde_json::from_slice(data).map_err(SchedulerError::Decode)
}

/// Invalid JSON entries are omitted; duplicate names use the last valid entry.
/// Unlike Go's out-of-bounds panic, malformed parallel slices return an error.
pub fn to_payload(
    names: &[String],
    configs: &[String],
) -> Result<BTreeMap<String, Value>, SchedulerError> {
    if configs.len() < names.len() {
        return Err(SchedulerError::PayloadLength);
    }
    let mut payload = BTreeMap::new();
    for (name, config) in names.iter().zip(configs) {
        if let Ok(mut value) = decode_config(config.as_bytes()) {
            payload_numbers(&mut value);
            payload.insert(name.clone(), value);
        }
    }
    Ok(payload)
}

// Go unmarshals into `any` using float64, including integers beyond 2^53.
fn payload_numbers(value: &mut Value) {
    match value {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                *n = serde_json::Number::from_f64(f).expect("finite JSON number");
            }
        }
        Value::Array(values) => values.iter_mut().for_each(payload_numbers),
        Value::Object(values) => values.values_mut().for_each(payload_numbers),
        _ => {}
    }
}

/// Owned JSON decoder suitable for delayed configuration construction.
pub struct ConfigJsonDecoder(Vec<u8>);
impl ConfigJsonDecoder {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, SchedulerError> {
        decode_config(&self.0)
    }
}

/// Persisted disabled state; the complete config is retained when toggling disabled.
/// Loads refresh state; load failures retain the last local state, as in Go.
pub struct DefaultSchedulerConfig {
    name: String,
    storage: Arc<StorageEndpoint>,
    paths: ClusterPaths,
    data: Value,
    disabled: bool,
}
impl DefaultSchedulerConfig {
    pub fn new(
        name: String,
        storage: Arc<StorageEndpoint>,
        paths: ClusterPaths,
        data: Value,
    ) -> Self {
        let disabled = data
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Self {
            name,
            storage,
            paths,
            data,
            disabled,
        }
    }
    pub async fn is_disabled(&mut self) -> bool {
        if let Ok(bytes) = self
            .storage
            .load_scheduler_config(&self.paths, &self.name)
            .await
        {
            // Go leaves absent/null fields untouched and ignores unknown fields.
            #[derive(Deserialize)]
            struct Disabled {
                disabled: Option<bool>,
            }
            if let Ok(config) = decode_config::<Disabled>(&bytes) {
                if let Some(disabled) = config.disabled {
                    self.disabled = disabled;
                }
            }
        }
        self.disabled
    }
    pub async fn set_disabled(&mut self, disabled: bool) -> Result<(), SchedulerError> {
        self.disabled = disabled; // Deliberately retained even when persistence fails.
        if !self.data.is_object() {
            self.data = Value::Object(Default::default());
        }
        self.data["disabled"] = Value::Bool(disabled);
        let data = encode_config(&self.data)?;
        self.storage
            .save_scheduler_config(&self.paths, &self.name, &data)
            .await?;
        Ok(())
    }
}

/// Shared metadata and lifecycle defaults. This is not a concrete scheduling policy.
pub struct BaseScheduler {
    name: String,
    typ: CheckerSchedulerType,
    config: Option<DefaultSchedulerConfig>,
}
impl BaseScheduler {
    pub fn new(typ: CheckerSchedulerType, config: Option<DefaultSchedulerConfig>) -> Self {
        Self {
            name: String::new(),
            typ,
            config,
        }
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
    pub fn name(&self) -> &str {
        if self.name.is_empty() {
            self.typ.as_str()
        } else {
            &self.name
        }
    }
    pub fn scheduler_type(&self) -> &CheckerSchedulerType {
        &self.typ
    }
    pub fn min_interval(&self) -> i64 {
        MIN_SCHEDULE_INTERVAL
    }
    pub fn next_interval(&self, interval: i64) -> i64 {
        interval_grow(interval, MAX_SCHEDULE_INTERVAL, IntervalGrowth::Exponential)
    }
    pub fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        encode_config(&Value::Null)
    }
    pub fn reload_config(&mut self) -> Result<(), SchedulerError> {
        Ok(())
    }
    pub fn prepare_config<C: SchedulerCluster>(
        &mut self,
        _cluster: &C,
    ) -> Result<(), SchedulerError> {
        Ok(())
    }
    pub fn clean_config<C: SchedulerCluster>(&mut self, _cluster: &C) {}
    pub fn is_default(&self) -> bool {
        self.is_default_in(&current_kernel_default_schedulers())
    }
    /// Host-supplied defaults permit explicit Classic and NextGen configuration.
    pub fn is_default_in(&self, defaults: &[CheckerSchedulerType]) -> bool {
        defaults.contains(&self.typ)
    }
    pub async fn is_disabled(&mut self) -> bool {
        match self.config.as_mut() {
            Some(config) => config.is_disabled().await,
            None => false,
        }
    }
    pub async fn set_disabled(&mut self, disabled: bool) -> Result<(), SchedulerError> {
        match self.config.as_mut() {
            Some(config) => config.set_disabled(disabled).await,
            None => Ok(()),
        }
    }
    /// The base alone cannot schedule; callers must use a concrete policy.
    pub fn update_config(&mut self, _data: &[u8]) -> Result<(), SchedulerError> {
        Err(SchedulerError::Unsupported("update config"))
    }
}

/// Policy contract using canonical cluster, operator and diagnostic objects.
/// HTTP handling belongs to adapters. No policy is implicitly implemented by BaseScheduler.
pub trait Scheduler<C: SchedulerCluster> {
    fn base(&self) -> &BaseScheduler;
    fn base_mut(&mut self) -> &mut BaseScheduler;
    fn name(&self) -> &str {
        self.base().name()
    }
    fn scheduler_type(&self) -> &CheckerSchedulerType {
        self.base().scheduler_type()
    }
    fn encode_config(&self) -> Result<Vec<u8>, SchedulerError> {
        self.base().encode_config()
    }
    fn reload_config(&mut self) -> Result<(), SchedulerError> {
        self.base_mut().reload_config()
    }
    fn min_interval(&self) -> i64 {
        self.base().min_interval()
    }
    fn next_interval(&self, interval: i64) -> i64 {
        self.base().next_interval(interval)
    }
    fn prepare_config(&mut self, cluster: &C) -> Result<(), SchedulerError> {
        self.base_mut().prepare_config(cluster)
    }
    fn clean_config(&mut self, cluster: &C) {
        self.base_mut().clean_config(cluster);
    }
    fn is_default(&self) -> bool {
        self.base().is_default()
    }
    fn is_disabled(&mut self) -> impl std::future::Future<Output = bool> + Send
    where
        Self: Send,
    {
        async { self.base_mut().is_disabled().await }
    }
    fn set_disabled(
        &mut self,
        disabled: bool,
    ) -> impl std::future::Future<Output = Result<(), SchedulerError>> + Send
    where
        Self: Send,
    {
        async move { self.base_mut().set_disabled(disabled).await }
    }
    fn schedule(
        &mut self,
        cluster: &C,
        dry_run: bool,
    ) -> Result<(Vec<Arc<Operator>>, Vec<BalanceSchedulerPlan>), SchedulerError>;
    fn is_schedule_allowed(&self, cluster: &C) -> bool;
}

/// Empty until concrete policies are ported. Known type strings are not factories.
#[derive(Default)]
pub struct SchedulerRegistry;
impl SchedulerRegistry {
    pub fn lookup(&self, typ: &CheckerSchedulerType) -> Result<(), SchedulerError> {
        Err(SchedulerError::NotRegistered(typ.clone()))
    }
    pub fn decode_slice<T>(
        &self,
        typ: &CheckerSchedulerType,
        _args: &[String],
    ) -> Result<T, SchedulerError> {
        Err(SchedulerError::DecoderNotRegistered(typ.clone()))
    }
    pub fn find_type_by_name(&self, name: &str) -> Option<CheckerSchedulerType> {
        name.contains("scatter-range")
            .then_some(astersql_pd_schedule_types::SCATTER_RANGE_SCHEDULER)
    }
}
