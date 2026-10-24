// Copyright 2026 AsterSQL.
//! Pure balance diagnostics, independent of schedulers and transports.
use crate::{BalanceSchedulerPlan, Status};
use astersql_pd_errs::{TypedError, catalog::ERR_DIAGNOSTIC_LOAD_PLAN};
use std::{any::Any, collections::BTreeMap};

/// Successful summary: store statuses and the aggregate normal flag.
pub type SummaryResult = Result<(BTreeMap<u64, Status>, bool), TypedError>;
/// Go Summary equivalent. Dynamic inputs preserve the wrong-plan error boundary.
pub type Summary = fn(&[&dyn Any]) -> SummaryResult;

/// Summarizes plans at each store's deepest step. Priority precedes frequency,
/// then smaller codes win. Equal codes with different reasons have no Go tie order.
pub fn balance_plan_summary(plans: &[&dyn Any]) -> SummaryResult {
    let mut stores: BTreeMap<u64, (i64, Vec<(Status, usize)>)> = BTreeMap::new();
    let mut normal = true;
    for input in plans {
        let p = input
            .downcast_ref::<BalanceSchedulerPlan>()
            .ok_or_else(|| ERR_DIAGNOSTIC_LOAD_PLAN.instantiate(std::iter::empty::<String>()))?;
        let step = p.step.min(2);
        let store = if step == 1 {
            p.source.as_ref().expect("source is absent").id()
        } else {
            p.get_resource(step)
        };
        let (max_step, counts) = stores.entry(store).or_insert((-1, Vec::new()));
        if step > *max_step {
            counts.clear();
            *max_step = step;
        } else if step < *max_step {
            continue;
        }
        assert!(step >= 0, "assignment to entry in nil map");
        normal &= p.status.is_normal();
        if let Some((_, count)) = counts.iter_mut().find(|(s, _)| *s == p.status) {
            *count += 1;
        } else {
            counts.push((p.status.clone(), 1));
        }
    }
    let statuses = stores
        .into_iter()
        .filter_map(|(id, (_, counts))| {
            if counts.is_empty() {
                return None;
            }
            let mut best = Status::default();
            let mut max = 0;
            for (status, count) in counts {
                if better(max, &best, count, &status) {
                    max = count;
                    best = status;
                }
            }
            Some((id, best))
        })
        .collect();
    Ok((statuses, normal))
}
fn better(old_count: usize, old: &Status, new_count: usize, new: &Status) -> bool {
    if new.priority() != old.priority() {
        new.priority() > old.priority()
    } else if new_count != old_count {
        new_count > old_count
    } else {
        new.status_code < old.status_code
    }
}
