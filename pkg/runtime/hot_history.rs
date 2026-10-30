// Copyright 2026 AsterSQL.
//! History sampling runs inside the existing term worker and fenced MetaStore.
use super::Resources;
use astersql_pd_storage::{HistoryHotRegion, HotHistory, MetaStore};
use chrono::{Datelike, Days, Local, TimeZone};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
pub(super) struct Maintenance {
    store: HotHistory,
    last_flush: tokio::time::Instant,
    interval: Duration,
    next_delete: i64,
    first_delete: bool,
    pending: BTreeMap<(String, i64, u64), HistoryHotRegion>,
}
fn first_delete<T: TimeZone>(now: chrono::DateTime<T>) -> i64 {
    let next = now
        .timezone()
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 4, 0, 0)
        .earliest()
        .map(|t| t.timestamp_millis())
        .unwrap_or(now.timestamp_millis() + 86_400_000);
    if next < now.timestamp_millis() {
        next + 86_400_000
    } else {
        next
    }
}
impl Maintenance {
    pub(super) fn new(
        store: Arc<dyn MetaStore>,
        cluster: u64,
        member: u64,
        interval: Duration,
    ) -> Self {
        Self {
            store: HotHistory::new(store, cluster, member),
            last_flush: tokio::time::Instant::now(),
            interval,
            next_delete: first_delete(Local::now()),
            first_delete: true,
            pending: BTreeMap::new(),
        }
    }
    fn flush_due(&mut self, next: Duration) -> bool {
        if self.last_flush.elapsed() < self.interval {
            return false;
        }
        self.last_flush = tokio::time::Instant::now();
        self.interval = next;
        true
    }
    fn delete_due(&mut self, now: i64) -> bool {
        if now < self.next_delete {
            return false;
        }
        if self.first_delete {
            self.next_delete = now + 86_400_000;
            self.first_delete = false;
        } else {
            self.next_delete += ((now - self.next_delete) / 86_400_000 + 1) * 86_400_000;
        }
        true
    }
    pub(super) fn wait(&self) -> Duration {
        let until_flush = self.interval.saturating_sub(self.last_flush.elapsed());
        let until_delete = Duration::from_millis(
            self.next_delete
                .saturating_sub(Local::now().timestamp_millis())
                .max(0) as u64,
        );
        until_flush.min(until_delete).max(Duration::from_millis(1))
    }
}
pub(super) async fn maintain(
    r: &mut Resources,
    coordinator: &astersql_pd_schedule::Coordinator<
        astersql_pd_schedule_config::ProviderSnapshot<()>,
    >,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let config = r.state.lock().unwrap().config.clone();
    let interval =
        Duration::from_nanos(config.schedule.hot_regions_write_interval.as_nanos().max(1) as u64);
    let days = config.schedule.hot_regions_reserved_days;
    let time = Local::now();
    if r.history.delete_due(time.timestamp_millis()) {
        if days > 0 {
            let cutoff = time
                .checked_sub_days(Days::new(days))
                .ok_or("history retention date overflow")?
                .timestamp_millis();
            r.history
                .store
                .purge_before(cutoff, cancel)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    if !r.history.flush_due(interval) {
        return Ok(());
    }
    if days == 0 {
        return Ok(());
    }
    let method = r
        .persisted_config
        .config
        .security
        .encryption
        .data_encryption_method
        .as_str();
    if !matches!(method, "" | "plaintext") {
        return Err("history region encryption key manager is not configured".into());
    }
    use astersql_pd_statistics::StoreStatInformer;
    use astersql_pd_statistics_utils::RWType;
    let handler = astersql_pd_schedule_handler::Handler::new(Some(coordinator));
    for rw in [RWType::READ, RWType::WRITE] {
        let mut degree = config.schedule.hot_region_cache_hits_threshold as i64;
        if rw == RWType::READ {
            degree = degree.wrapping_mul(6);
        }
        let peers = r
            .hot
            .hot_cache
            .get_hot_peer_stats(rw, degree)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
        let empty = std::collections::HashMap::new();
        let loads = r.hot.get_stores_loads();
        let info = handler
            .get_hot_regions(
                rw,
                &[],
                &astersql_pd_schedule::HotObservationInputs {
                    loads: &loads,
                    read: if rw == RWType::READ { &peers } else { &empty },
                    write: if rw == RWType::WRITE { &peers } else { &empty },
                },
            )
            .map_err(|e| e.to_string())?;
        for population in info.as_peer.values().flatten() {
            for stat in population.stats.as_deref().unwrap_or_default() {
                let region = r
                    .state
                    .lock()
                    .unwrap()
                    .cluster
                    .regions
                    .get_region(stat.region_id);
                let Some(region) = region else {
                    continue;
                };
                if region.meta().encryption_meta.is_some() {
                    return Err("history region encryption key manager is not configured".into());
                }
                let Some(peer) = region.get_store_peer(stat.store_id) else {
                    continue;
                };
                let row = HistoryHotRegion {
                    update_time: region.report_interval().1.wrapping_mul(1000) as i64,
                    region_id: stat.region_id,
                    peer_id: peer.id,
                    store_id: stat.store_id,
                    is_leader: stat.is_leader,
                    // Preserve Go packHotRegions' peer-ID lookup for this field.
                    is_learner: region
                        .get_peer(stat.store_id)
                        .is_some_and(|p| p.is_learner()),
                    hot_region_type: if rw == RWType::READ { "read" } else { "write" }.into(),
                    hot_degree: stat.hot_degree,
                    flow_bytes: stat.byte_rate,
                    flow_cpu: stat.cpu_rate,
                    key_rate: stat.key_rate,
                    query_rate: stat.query_rate,
                    start_key: astersql_pd_core::hex_region_key_str(
                        &region.meta().key_range.start_key,
                    ),
                    end_key: astersql_pd_core::hex_region_key_str(&region.meta().key_range.end_key),
                    encryption_meta: None,
                };
                r.history.pending.insert(
                    (row.hot_region_type.clone(), row.update_time, row.region_id),
                    row,
                );
            }
        }
    }
    let rows = r.history.pending.values().cloned().collect::<Vec<_>>();
    r.history
        .store
        .append(&rows, cancel)
        .await
        .map_err(|e| e.to_string())?;
    r.history.pending.clear();
    Ok(())
}

#[cfg(test)]
#[path = "hot_history_test.rs"]
mod tests;
