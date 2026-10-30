// Copyright 2026 AsterSQL.
//! Term-owned split admission; no protocol types enter the canonical cache.
use super::*;
use astersql_pd_server::grpc_ask_split::{SplitError, SplitIds};
impl ClusterHost {
    pub(crate) async fn batch_split_version(&self) -> Result<Option<String>, String> {
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or("node stopped")?;
        let version = &r.persisted_config.config.cluster_version;
        let threshold = astersql_pd_server_config::ClusterVersion {
            major: 2,
            minor: 1,
            patch: 0,
            pre_release: "rc.1".into(),
            metadata: String::new(),
        };
        Ok((compare_version(version, &threshold).is_lt()).then(|| version.to_string()))
    }
    /// Go updates the cluster version when registered store versions change.
    pub(super) async fn update_split_version(&self, r: &mut Resources) {
        let versions = {
            let state = r.state.lock().unwrap();
            state
                .cluster
                .stores
                .get_stores()
                .into_iter()
                .filter(|s| !s.is_removed())
                .filter_map(|s| s.meta().cloned())
                .map(|s| {
                    serde_json::from_value::<astersql_pd_server_config::ClusterVersion>(
                        serde_json::Value::String(s.version.trim_start_matches('v').into()),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let next = match versions {
            Ok(versions) => versions.into_iter().min_by(compare_version),
            Err(error) => {
                *self.store_config_error.lock().unwrap() = Some(error.to_string());
                return;
            }
        };
        if let Some(next) = next
            && !compare_version(&next, &r.persisted_config.config.cluster_version).is_eq()
        {
            r.persisted_config.config.cluster_version = next;
            if let Err(error) = r.persisted_config.persist(&r.storage, &r.paths).await {
                *self.store_config_error.lock().unwrap() = Some(error.to_string());
            }
        }
    }
    pub(crate) async fn split_ids(
        &self,
        region: astersql_pd_core::RegionMeta,
        count: u32,
        batch: bool,
        _reason: i32,
    ) -> Result<SplitIds, SplitError> {
        let other = SplitError::Other;
        let guard = self.resources.lock().await;
        let r = guard.as_ref().ok_or_else(|| other("node stopped".into()))?;
        r.leader
            .validate()
            .await
            .map_err(|e| other(e.to_string()))?;
        {
            let state = r.state.lock().unwrap();
            if state.config.schedule.halt_scheduling {
                return Err(other(
                    "[PD:cluster:ErrSchedulingIsHalted]scheduling is halted".into(),
                ));
            }
            if !state.config.schedule.enable_ti_kv_split_region {
                return Err(other(
                    "[PD:scheduler:ErrSchedulerTiKVSplitDisabled]tikv split region disabled".into(),
                ));
            }
            state
                .cluster
                .regions
                .valid_region(&region)
                .map_err(SplitError::Validation)?;
            let mode = &r.persisted_config.config.replication_mode;
            if mode.replication_mode == "dr-auto-sync" && mode.dr_auto_sync.pause_region_split {
                return Err(other("region split is paused by replication mode".into()));
            }
        }
        let mut ids = Vec::new();
        for _ in 0..count {
            let id = r.allocator.alloc().await.map_err(|e| {
                other(if batch {
                    "[PD:scheduler:ErrSchedulerNotFound]scheduler not found".into()
                } else {
                    e.to_string()
                })
            })?;
            let mut peers = Vec::with_capacity(region.peers.len());
            for _ in &region.peers {
                peers.push(
                    r.allocator
                        .alloc()
                        .await
                        .map_err(|e| other(e.to_string()))?,
                );
            }
            ids.push((id, peers));
        }
        Ok(ids)
    }
}

pub(super) fn compare_version(
    a: &astersql_pd_server_config::ClusterVersion,
    b: &astersql_pd_server_config::ClusterVersion,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let numeric = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if numeric != Ordering::Equal {
        return numeric;
    }
    match (a.pre_release.is_empty(), b.pre_release.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let mut left = a.pre_release.split('.');
    let mut right = b.pre_release.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let xn = x.bytes().all(|b| b.is_ascii_digit());
                let yn = y.bytes().all(|b| b.is_ascii_digit());
                let order = match (xn, yn) {
                    (true, true) => {
                        let x = x.trim_start_matches('0');
                        let y = y.trim_start_matches('0');
                        x.len().cmp(&y.len()).then_with(|| x.cmp(y))
                    }
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    _ => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "split_version_test.rs"]
mod split_version_test;
