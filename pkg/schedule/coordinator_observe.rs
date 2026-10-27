// Copyright 2026 AsterSQL.
//! Hot observations borrow host-owned statistics; the facade owns no hot cache.
use super::*;
use astersql_pd_statistics::{
    HotPeerStat, HotPeersStat, StoreHotPeersInfos, StoreKindLoads, get_hot_status,
};
use astersql_pd_statistics_utils::RWType;
use std::collections::HashMap;

/// Current cache snapshots, in canonical statistics units, supplied by the host.
pub struct HotObservationInputs<'a> {
    pub loads: &'a HashMap<u64, StoreKindLoads>,
    pub read: &'a HashMap<u64, Vec<HotPeerStat>>,
    pub write: &'a HashMap<u64, Vec<HotPeerStat>>,
}
impl HotObservationInputs<'_> {
    fn peers(&self, rw: RWType) -> &HashMap<u64, Vec<HotPeerStat>> {
        if rw == RWType::WRITE {
            self.write
        } else {
            self.read
        }
    }
}
/// Store filtering preserves Go's nullable map entries without changing the
/// canonical statistics population type used by aggregation and scheduling.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct HotRegionsQuery {
    pub as_peer: BTreeMap<u64, Option<HotPeersStat>>,
    pub as_leader: BTreeMap<u64, Option<HotPeersStat>>,
}
impl From<StoreHotPeersInfos> for HotRegionsQuery {
    fn from(infos: StoreHotPeersInfos) -> Self {
        Self {
            as_peer: infos
                .as_peer
                .into_iter()
                .map(|(id, stat)| (id, Some(stat)))
                .collect(),
            as_leader: infos
                .as_leader
                .into_iter()
                .map(|(id, stat)| (id, Some(stat)))
                .collect(),
        }
    }
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Coordinator<C> {
    /// Periodic publication borrows current statistics and rechecks store liveness.
    pub fn collect_hot_spot_metrics(
        &self,
        metrics: &HotSpotMetrics,
        input: &HotObservationInputs<'_>,
    ) {
        let stores: Vec<_> = self
            .cluster
            .stores
            .get_stores()
            .iter()
            .map(|s| s.as_ref().clone())
            .collect();
        metrics.collect_hot_spot_metrics(&self.cluster, &stores, input);
    }
    pub fn get_hot_regions_by_type(
        &self,
        rw: RWType,
        input: &HotObservationInputs<'_>,
    ) -> StoreHotPeersInfos {
        let stores: Vec<_> = self
            .cluster
            .stores
            .get_stores()
            .iter()
            .map(|s| s.as_ref().clone())
            .collect();
        let mut infos = get_hot_status(
            &stores,
            input.loads,
            input.peers(rw),
            rw,
            self.config.is_trace_region_flow(),
        );
        for population in [&mut infos.as_peer, &mut infos.as_leader] {
            for stat in population.values_mut() {
                for peer in stat.stats.iter_mut().flatten() {
                    let region = self.cluster.regions.get_region(peer.region_id);
                    if let Some(region) = &region {
                        // Preserve Go GetPeer(StoreID), which looks up by peer ID.
                        peer.is_learner = region
                            .get_peer(peer.store_id)
                            .is_some_and(|p| p.is_learner());
                    }
                    let timestamp = if rw == RWType::WRITE {
                        region.map(|r| (r.report_interval().1 as i64, 0))
                    } else if rw == RWType::READ {
                        self.cluster.stores.get_store(peer.store_id).map(|s| {
                            let nanos = s.last_heartbeat_unix_nanos();
                            (
                                nanos.div_euclid(1_000_000_000),
                                nanos.rem_euclid(1_000_000_000) as u32,
                            )
                        })
                    } else {
                        None
                    };
                    if let Some((seconds, nanos)) = timestamp {
                        peer.last_update_time = unix_timestamp(seconds, nanos);
                    }
                }
            }
        }
        infos
    }
    /// Explicit unknown IDs retain null entries, matching Go pointer-map reads.
    pub fn get_hot_regions(
        &self,
        rw: RWType,
        ids: &[u64],
        input: &HotObservationInputs<'_>,
    ) -> HotRegionsQuery {
        let mut result = self.get_hot_regions_by_type(rw, input);
        if ids.is_empty() {
            return result.into();
        }
        let mut filtered = HotRegionsQuery::default();
        for id in ids {
            filtered
                .as_peer
                .entry(*id)
                .or_insert_with(|| result.as_peer.remove(id));
            filtered
                .as_leader
                .entry(*id)
                .or_insert_with(|| result.as_leader.remove(id));
        }
        filtered
    }
}

use astersql_pd_schedule_schedulers::SchedulerMetrics;
use astersql_pd_statistics::collect_hot_peer_infos;
use prometheus::{Gauge, GaugeVec, Opts, Registry};

struct HotSpotSeries {
    vector: GaugeVec,
    handles: BTreeMap<[String; 3], Gauge>,
}
/// Host-owned registry and cached gauge handles; clones share deletion and reset.
/// Collection is periodic, outside the query path. No region labels or global state.
#[derive(Clone)]
pub struct HotSpotMetrics {
    series: Arc<Mutex<HotSpotSeries>>,
    scheduler: SchedulerMetrics,
}
impl HotSpotMetrics {
    pub fn register(registry: &Registry, scheduler: SchedulerMetrics) -> prometheus::Result<Self> {
        let vector = GaugeVec::new(
            Opts::new("pd_hotspot_status", "Status of the hotspot."),
            &["address", "store", "type"],
        )?;
        registry.register(Box::new(vector.clone()))?;
        Ok(Self {
            series: Arc::new(Mutex::new(HotSpotSeries {
                vector,
                handles: BTreeMap::new(),
            })),
            scheduler,
        })
    }
    /// Accept an immutable store snapshot, then consult the live canonical cluster
    /// after publishing so a buried/deleted store cannot be revived by that snapshot.
    pub fn collect_hot_spot_metrics(
        &self,
        cluster: &BasicCluster,
        stores: &[astersql_pd_core::StoreInfo],
        input: &HotObservationInputs<'_>,
    ) {
        let mut series = self.series.lock().unwrap();
        for rw in [RWType::WRITE, RWType::READ] {
            let status = collect_hot_peer_infos(stores, input.peers(rw));
            for store in stores {
                let id = store.id();
                let label = id.to_string();
                let mut hot = false;
                for (role, population) in [("leader", &status.as_leader), ("peer", &status.as_peer)]
                {
                    let stat = population.get(&id).filter(|_| !store.is_removed());
                    hot |= stat.is_some();
                    let values = stat.map(|s| {
                        [
                            s.total_bytes_rate,
                            s.total_keys_rate,
                            s.total_query_rate,
                            s.total_cpu_rate,
                            s.count as f64,
                        ]
                    });
                    for (i, dim) in ["bytes", "keys", "query", "cpu", "region"]
                        .into_iter()
                        .enumerate()
                    {
                        let typ = if dim == "region" {
                            format!("hot_{rw}_region_as_{role}")
                        } else {
                            format!("total_{rw}_{dim}_as_{role}")
                        };
                        let key = [store.address().to_owned(), label.clone(), typ];
                        if let Some(values) = values {
                            if !series.handles.contains_key(&key) {
                                let gauge = series
                                    .vector
                                    .with_label_values(&[&key[0], &key[1], &key[2]]);
                                series.handles.insert(key.clone(), gauge);
                            }
                            series.handles[&key].set(values[i]);
                        } else {
                            series
                                .vector
                                .remove_label_values(&[&key[0], &key[1], &key[2]])
                                .ok();
                            series.handles.remove(&key);
                        }
                    }
                }
                // Go deliberately clears BOTH RW pending populations when either
                // RW snapshot has no hot population. Do not conflate missing/empty.
                if !hot {
                    self.scheduler.delete_hot_pending_metrics(Some(&label));
                }
                if !store.is_removed()
                    && cluster.stores.get_store(id).is_none_or(|s| s.is_removed())
                {
                    delete_series(&mut series, &label);
                }
            }
        }
    }
    pub fn delete_store_metrics(&self, store: &str) {
        delete_series(&mut self.series.lock().unwrap(), store);
    }
    pub fn reset_hot_spot_metrics(&self) {
        let mut series = self.series.lock().unwrap();
        series.vector.reset();
        series.handles.clear();
        self.scheduler.delete_hot_pending_metrics(None);
    }
}
fn delete_series(series: &mut HotSpotSeries, store: &str) {
    series.handles.retain(|key, _| {
        if key[1] != store {
            return true;
        }
        series
            .vector
            .remove_label_values(&[&key[0], &key[1], &key[2]])
            .ok();
        false
    });
}

/// Proleptic Gregorian UTC, matching Go RFC3339Nano's trimmed fractions.
/// i128 arithmetic preserves every signed Unix second (including wrapped u64
/// region interval endpoints), without a third-party calendar range cutoff.
fn unix_timestamp(seconds: i64, nanos: u32) -> String {
    let days = (seconds as i128).div_euclid(86400);
    let clock = seconds.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let day_of_era = z - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i128::from(month <= 2);
    let year = if year < 0 {
        format!("-{:04}", -year)
    } else {
        format!("{year:04}")
    };
    let fraction = if nanos == 0 {
        String::new()
    } else {
        format!(".{nanos:09}").trim_end_matches('0').to_owned()
    };
    format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}{fraction}Z",
        clock / 3600,
        clock / 60 % 60,
        clock % 60
    )
}
