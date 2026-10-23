// Copyright 2026 AsterSQL.
//! Metric push configuration values; no background push IO.
use astersql_pd_typeutil::Duration;
use serde::{Deserialize, Serialize};
/// Pure values from Go MetricConfig; no service or IO initialization.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetricConfig {
    #[serde(rename = "job")]
    #[serde(deserialize_with = "null_default")]
    pub push_job: String,
    #[serde(rename = "address")]
    #[serde(deserialize_with = "null_default")]
    pub push_address: String,
    #[serde(rename = "interval")]
    pub push_interval: Duration,
}

impl MetricConfig {
    /// Uses the Config-level 15s interval and server name only for missing/invalid values.
    pub fn adjust(&mut self, name: &str) {
        if self.push_interval.as_nanos() <= 0 {
            self.push_interval = Duration::from_nanos(15000000000);
        }
        if self.push_job.is_empty() {
            self.push_job = name.into();
        }
    }
}

fn null_default<'de, T: serde::Deserialize<'de> + Default, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[cfg(test)]
mod config_test;
