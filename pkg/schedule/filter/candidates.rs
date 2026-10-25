// Copyright 2026 AsterSQL.
//! Candidate operations over shared canonical store snapshots.
use crate::{Filter, select_source_stores, select_target_stores};
use astersql_pd_core::{StoreInfo, distinct_score};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_plan::Collector;
use std::{cmp::Ordering, fmt, sync::Arc};

fn compare_scores(a: f64, b: f64) -> Ordering {
    if a > b {
        Ordering::Greater
    } else if a < b {
        Ordering::Less
    } else {
        Ordering::Equal
    }
}

/// Compare region scores using one borrowed configuration generation and zero delta.
pub fn region_score_comparer<V>(
    conf: &dyn SharedConfigProvider<Version = V>,
) -> impl Fn(&StoreInfo, &StoreInfo) -> Ordering + '_ {
    move |a, b| {
        let score = |s: &StoreInfo| {
            s.region_score(
                conf.get_region_score_formula_version(),
                conf.get_high_space_ratio(),
                conf.get_low_space_ratio(),
                0,
            )
        };
        compare_scores(score(a), score(b))
    }
}

/// Compare hierarchical diversity against the same replica set, including repeats.
pub fn isolation_comparer<'a>(
    labels: &'a [&str],
    region_stores: &'a [&StoreInfo],
) -> impl Fn(&StoreInfo, &StoreInfo) -> Ordering + 'a {
    move |a, b| {
        compare_scores(
            distinct_score(labels, region_stores, a),
            distinct_score(labels, region_stores, b),
        )
    }
}

/// A host random source violated its exclusive upper bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomIndexError {
    /// Invalid sampled index.
    pub index: usize,
    /// Requested exclusive upper bound.
    pub upper_bound: usize,
}
impl fmt::Display for RandomIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "random index {} outside 0..{}",
            self.index, self.upper_bound
        )
    }
}
impl std::error::Error for RandomIndexError {}
fn sample(
    random: &mut impl FnMut(usize) -> usize,
    upper_bound: usize,
) -> Result<usize, RandomIndexError> {
    let index = random(upper_bound);
    if index < upper_bound {
        Ok(index)
    } else {
        Err(RandomIndexError { index, upper_bound })
    }
}

/// An owned candidate order retaining the identity of host-owned store snapshots.
#[derive(Debug, Clone, Default)]
pub struct StoreCandidates {
    stores: Vec<Arc<StoreInfo>>,
}
impl StoreCandidates {
    /// Take candidate order without copying store state or deduplicating IDs.
    pub fn new(stores: Vec<Arc<StoreInfo>>) -> Self {
        Self { stores }
    }
    /// Apply the canonical source selector and its first-rejection diagnostics.
    pub fn filter_source<V>(
        &mut self,
        conf: &dyn SharedConfigProvider<Version = V>,
        collector: Option<&mut Collector>,
        filters: &[&dyn Filter<V>],
    ) -> &mut Self {
        self.stores = select_source_stores(&self.stores, filters, conf, collector);
        self
    }
    /// Apply the canonical target selector and its first-rejection diagnostics.
    pub fn filter_target<V>(
        &mut self,
        conf: &dyn SharedConfigProvider<Version = V>,
        collector: Option<&mut Collector>,
        filters: &[&dyn Filter<V>],
    ) -> &mut Self {
        self.stores = select_target_stores(&self.stores, filters, conf, collector);
        self
    }
    /// Sort ascending. Like Go sort.Slice, equal-key order is unspecified.
    pub fn sort(&mut self, cmp: impl Fn(&StoreInfo, &StoreInfo) -> Ordering) -> &mut Self {
        self.stores.sort_unstable_by(|a, b| cmp(a, b));
        self
    }
    /// Retain every best-scoring store in Go's in-place scan order.
    pub fn keep_the_top_stores(
        &mut self,
        cmp: impl Fn(&StoreInfo, &StoreInfo) -> Ordering,
        asc: bool,
    ) -> &mut Self {
        if self.stores.len() <= 1 {
            return self;
        }
        let mut top = 0;
        for i in 1..self.stores.len() {
            let order = cmp(&self.stores[top], &self.stores[i]);
            if order == Ordering::Equal {
                top += 1;
            } else if (asc && order == Ordering::Greater) || (!asc && order == Ordering::Less) {
                top = 0;
            } else {
                continue;
            }
            self.stores.swap(i, top);
        }
        self.stores.truncate(top + 1);
        self
    }
    /// Borrow the first minimum/maximum without reordering. Ties keep the first.
    pub fn pick_the_top_store(
        &self,
        cmp: impl Fn(&StoreInfo, &StoreInfo) -> Ordering,
        asc: bool,
    ) -> Option<&Arc<StoreInfo>> {
        let mut top = self.stores.first()?;
        for store in &self.stores[1..] {
            let order = cmp(top, store);
            if (asc && order == Ordering::Greater) || (!asc && order == Ordering::Less) {
                top = store;
            }
        }
        Some(top)
    }
    /// Borrow the first candidate; empty candidates return None.
    pub fn pick_first(&self) -> Option<&Arc<StoreInfo>> {
        self.stores.first()
    }
    /// Reorder using host-provided uniform bounded samples (descending Fisher–Yates).
    /// An invalid sample returns an error, retaining any preceding valid swaps.
    pub fn shuffle(
        &mut self,
        mut random: impl FnMut(usize) -> usize,
    ) -> Result<&mut Self, RandomIndexError> {
        for i in (1..self.stores.len()).rev() {
            let j = sample(&mut random, i + 1)?;
            self.stores.swap(i, j);
        }
        Ok(self)
    }
    /// Borrow a sampled candidate. Empty candidates do not consult the random source.
    pub fn random_pick(
        &self,
        mut random: impl FnMut(usize) -> usize,
    ) -> Result<Option<&Arc<StoreInfo>>, RandomIndexError> {
        if self.stores.is_empty() {
            return Ok(None);
        }
        let index = sample(&mut random, self.stores.len())?;
        Ok(Some(&self.stores[index]))
    }
    /// Borrow all candidates in their current order.
    pub fn pick_all(&self) -> &[Arc<StoreInfo>] {
        &self.stores
    }
    /// Number of candidates, including repeated IDs.
    pub fn len(&self) -> usize {
        self.stores.len()
    }
    /// Whether there are no candidates.
    pub fn is_empty(&self) -> bool {
        self.stores.is_empty()
    }
}
