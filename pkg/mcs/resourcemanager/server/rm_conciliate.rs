// Copyright 2026 AsterSQL.
//! Priority queues allocate the service budget using live per-client RU demand.
use crate::{
    Manager,
    rm_slot::{go_max, go_min},
};
use std::collections::BTreeMap;
struct GroupDemand {
    name: String,
    fill: f64,
    burst: i64,
    tracked: bool,
    rate: f64,
    basic: f64,
    extra: f64,
}
impl Manager {
    fn override_rates(&mut self, id: u32, name: &str, fill: f64, burst: i64) {
        let bucket = self.buckets.entry((id, name.into())).or_default();
        let old = bucket.override_fill_rate.unwrap_or(-1.0);
        if old < 0.0 || fill < 0.0 || (fill - old).abs() > old * 0.05 {
            bucket.override_fill_rate = Some(fill);
        }
        bucket.override_burst_limit = Some(burst);
    }
    /// Run on the embedding host's one-second timer; configured settings stay intact.
    pub fn conciliate(&mut self) {
        let limits: Vec<_> = self
            .limits
            .iter()
            .map(|(id, limit)| (*id, limit.rate))
            .collect();
        for (id, limit) in limits {
            if limit == 0.0 {
                continue;
            }
            let Some(groups) = self.groups.get(&id) else {
                continue;
            };
            let mut queues: BTreeMap<u32, Vec<GroupDemand>> = BTreeMap::new();
            for (name, group) in groups {
                let settings = group.ru.as_ref().and_then(|b| b.settings.as_ref());
                let fill = settings.map_or(0.0, |s| s.fill_rate as f64);
                let burst = settings.map_or(0, |s| s.burst_limit);
                let bucket = self.buckets.get(&(id, name.clone()));
                let tracked = bucket.is_some_and(|b| b.has_tracker);
                let rate = bucket.map_or(0.0, |b| b.demands.values().map(|d| d.rate).sum());
                let basic = if tracked { go_min(rate, fill) } else { 0.0 };
                let extra = if tracked && !(burst >= 0 && burst as f64 <= fill) {
                    go_max(0.0, rate - fill)
                } else {
                    0.0
                };
                queues.entry(group.priority).or_default().push(GroupDemand {
                    name: name.clone(),
                    fill,
                    burst,
                    tracked,
                    rate,
                    basic,
                    extra,
                });
            }
            let mut remaining = limit;
            for (_, mut queue) in queues.into_iter().rev() {
                if remaining == 0.0 {
                    for group in queue {
                        if group.tracked && group.rate != 0.0 {
                            self.override_rates(id, &group.name, 0.0, 0);
                        }
                    }
                    continue;
                }
                let total_basic: f64 = queue.iter().map(|g| g.basic).sum();
                let total_extra: f64 = queue.iter().map(|g| g.extra).sum();
                let total = total_basic + total_extra;
                if total == 0.0 {
                    continue;
                }
                if total >= remaining {
                    if total_basic >= remaining {
                        let mut total_fill: f64 =
                            queue.iter().filter(|g| g.tracked).map(|g| g.fill).sum();
                        queue.sort_by(|a, b| {
                            (a.basic / a.fill)
                                .partial_cmp(&(b.basic / b.fill))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        for group in queue {
                            let fill = go_min(group.basic, remaining * group.fill / total_fill);
                            self.override_rates(id, &group.name, fill, fill as i64);
                            remaining -= fill;
                            total_fill -= group.fill;
                        }
                    } else {
                        remaining -= total_basic;
                        let burst_capacity = remaining;
                        for group in queue {
                            let mut burst = group.fill + burst_capacity * group.extra / total_extra;
                            if group.burst > 0 {
                                burst = go_min(burst, group.burst as f64);
                            }
                            self.override_rates(id, &group.name, -1.0, burst as i64);
                            remaining -= go_max(0.0, burst - group.fill);
                        }
                    }
                } else {
                    for group in queue {
                        self.override_rates(
                            id,
                            &group.name,
                            -1.0,
                            if group.burst >= 0 {
                                -1
                            } else {
                                remaining as i64
                            },
                        );
                    }
                    remaining -= total;
                }
            }
        }
    }
    /// Remove stale EMA trackers without rewriting balances or configured limits.
    pub fn cleanup_demand_trackers(&mut self, now: i64) {
        for bucket in self.buckets.values_mut() {
            bucket.demands.retain(|_, d| {
                d.last_sample
                    .is_some_and(|last| now.saturating_sub(last) < 600_000_000_000)
            });
        }
    }
}
