// Copyright 2026 AsterSQL.
//! Management projections over canonical cluster and host-owned statistics.
use crate::{Handler, HandlerError};
use astersql_pd_core::{StoreInfo, hex_region_key_str};
use astersql_pd_schedule::{HotObservationInputs, HotRegionsQuery};
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_statistics::{StoreKindLoads, StoreStatInformer};
use astersql_pd_statistics_buckets::{BucketSubmitError, BucketWaitError, BucketWorkerClient};
use astersql_pd_statistics_utils::{RWType, StoreLoadKind};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotQueryError {
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error("store {0} not found")]
    StoreNotFound(u64),
    #[error(transparent)]
    BucketSubmit(#[from] BucketSubmitError),
    #[error(transparent)]
    BucketWait(#[from] BucketWaitError),
}
/// Store rates with Go's public field names and omission of empty maps.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct HotStoreStats {
    #[serde(
        rename = "bytes-write-rate",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub bytes_write_stats: BTreeMap<u64, f64>,
    #[serde(rename = "bytes-read-rate", skip_serializing_if = "BTreeMap::is_empty")]
    pub bytes_read_stats: BTreeMap<u64, f64>,
    #[serde(rename = "keys-write-rate", skip_serializing_if = "BTreeMap::is_empty")]
    pub keys_write_stats: BTreeMap<u64, f64>,
    #[serde(rename = "keys-read-rate", skip_serializing_if = "BTreeMap::is_empty")]
    pub keys_read_stats: BTreeMap<u64, f64>,
    #[serde(
        rename = "query-write-rate",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub query_write_stats: BTreeMap<u64, f64>,
    #[serde(rename = "query-read-rate", skip_serializing_if = "BTreeMap::is_empty")]
    pub query_read_stats: BTreeMap<u64, f64>,
    #[serde(rename = "cpu-read-rate", skip_serializing_if = "BTreeMap::is_empty")]
    pub cpu_read_stats: BTreeMap<u64, f64>,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct HotBucketsItem {
    pub start_key: String,
    pub end_key: String,
    pub hot_degree: i32,
    pub read_bytes: u64,
    pub read_keys: u64,
    pub write_bytes: u64,
    pub write_keys: u64,
}
pub type HotBucketsResponse = BTreeMap<u64, Vec<HotBucketsItem>>;
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Handler<'_, C> {
    pub fn get_store(&self, id: u64) -> Result<Arc<StoreInfo>, HotQueryError> {
        self.coordinator()?
            .get_cluster()
            .stores
            .get_store(id)
            .ok_or(HotQueryError::StoreNotFound(id))
    }
    pub fn get_stores(&self) -> Result<Vec<Arc<StoreInfo>>, HotQueryError> {
        // Re-read each listed ID, preserving missing-store failure under removal.
        let stores = self.coordinator()?.get_cluster().stores.get_stores();
        stores.iter().map(|s| self.get_store(s.id())).collect()
    }
    pub fn get_stores_loads(
        &self,
        stats: &impl StoreStatInformer,
    ) -> Result<HashMap<u64, StoreKindLoads>, HotQueryError> {
        self.coordinator()?;
        Ok(stats.get_stores_loads())
    }
    pub fn get_hot_stores(
        &self,
        stats: &impl StoreStatInformer,
    ) -> Result<HotStoreStats, HotQueryError> {
        let stores = self.get_stores()?;
        let loads = self.get_stores_loads(stats)?;
        let mut result = HotStoreStats::default();
        for store in stores {
            let id = store.id();
            let Some(load) = loads.get(&id) else { continue };
            if store.is_tiflash_compute() {
                continue;
            }
            let (bytes, keys) = if store.is_tiflash_write() {
                (
                    StoreLoadKind::REGIONS_WRITE_BYTES,
                    StoreLoadKind::REGIONS_WRITE_KEYS,
                )
            } else {
                (StoreLoadKind::WRITE_BYTES, StoreLoadKind::WRITE_KEYS)
            };
            for (map, kind) in [
                (&mut result.bytes_write_stats, bytes),
                (&mut result.keys_write_stats, keys),
                (&mut result.bytes_read_stats, StoreLoadKind::READ_BYTES),
                (&mut result.keys_read_stats, StoreLoadKind::READ_KEYS),
                (&mut result.query_write_stats, StoreLoadKind::WRITE_QUERY),
                (&mut result.query_read_stats, StoreLoadKind::READ_QUERY),
                (&mut result.cpu_read_stats, StoreLoadKind::READ_CPU),
            ] {
                map.insert(id, load[kind.as_i32() as usize]);
            }
        }
        Ok(result)
    }
    pub fn get_hot_regions(
        &self,
        rw: RWType,
        ids: &[u64],
        input: &HotObservationInputs<'_>,
    ) -> Result<HotRegionsQuery, HotQueryError> {
        Ok(self.coordinator()?.get_hot_regions(rw, ids, input))
    }
    /// Query the real FIFO worker with the current shared configuration threshold.
    /// Cancellation and queue/worker failure return errors, never partial success.
    pub async fn get_hot_buckets(
        &self,
        worker: &BucketWorkerClient,
        ids: &[u64],
        cancel: &CancellationToken,
    ) -> Result<HotBucketsResponse, HotQueryError> {
        let degree = self
            .coordinator()?
            .get_config()
            .get_hot_region_cache_hits_threshold();
        // The canonical bucket heat is i32; retain the full configuration range
        // without wrapping a large positive threshold into a cold threshold.
        let worker_degree = degree.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let stats = worker
            .collect(worker_degree, ids.to_vec())?
            .wait(cancel)
            .await?;
        if degree > i64::from(i32::MAX) {
            return Ok(BTreeMap::new());
        }
        Ok(stats
            .into_iter()
            .map(|(id, buckets)| {
                (
                    id,
                    buckets
                        .into_iter()
                        .map(|b| HotBucketsItem {
                            start_key: hex_region_key_str(&b.start_key),
                            end_key: hex_region_key_str(&b.end_key),
                            hot_degree: b.hot_degree,
                            read_bytes: b.loads[0],
                            read_keys: b.loads[1],
                            write_bytes: b.loads[3],
                            write_keys: b.loads[4],
                        })
                        .collect(),
                )
            })
            .collect())
    }
}
