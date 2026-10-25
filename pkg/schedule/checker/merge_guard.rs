// Copyright 2026 AsterSQL.
//! Merge permission over canonical key, placement, label and store snapshots.
use astersql_pd_codec::table_identity;
use astersql_pd_core::{RegionInfo, StoresInfo};
use astersql_pd_core_constant::KeyType;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_filter::{
    RegionReplicationContext, is_region_healthy, is_region_replicated,
};
use astersql_pd_schedule_labeler::{RegionLabeler, Timestamp};
use astersql_pd_schedule_placement::RuleManager;

/// Check adjacency, internal split boundaries, labels and logical table identity.
/// Missing placement capability rejects when rules are enabled, matching Go's
/// interface probe. Malformed boundary keys have the zero identity as in Go.
/// Raw/transactional key policies bypass identity only, never range constraints.
pub fn allow_merge<C: SharedConfigProvider>(
    conf: &C,
    rules: Option<&RuleManager>,
    labels: Option<&RegionLabeler>,
    now: Timestamp,
    region: &RegionInfo,
    adjacent: &RegionInfo,
) -> bool {
    let a = &region.meta().key_range;
    let b = &adjacent.meta().key_range;
    let (start, end) = if !a.end_key.is_empty() && a.end_key == b.start_key {
        (&a.start_key, &b.end_key)
    } else if !b.end_key.is_empty() && b.end_key == a.start_key {
        (&b.start_key, &a.end_key)
    } else {
        return false;
    };
    if conf.is_placement_rules_enabled()
        && rules.is_none_or(|rules| !rules.get_split_keys(start, end).is_empty())
    {
        return false;
    }
    if let Some(labels) = labels {
        if !labels.index().get_split_keys(start, end).is_empty()
            || labels.get_region_label(region, "merge_option", now) == "deny"
            || labels.get_region_label(adjacent, "merge_option", now) == "deny"
        {
            return false;
        }
    }
    match conf.get_key_type() {
        KeyType::RAW | KeyType::TXN => true,
        KeyType::TABLE if conf.is_cross_table_merge_enabled() => true,
        _ => {
            table_identity(&a.start_key).unwrap_or_default()
                == table_identity(&b.start_key).unwrap_or_default()
        }
    }
}

/// Do not bring a new peer into a missing/removing target store. A source peer
/// already on that store permits merging; unrelated store health is immaterial.
pub fn check_peer_store(stores: &StoresInfo, region: &RegionInfo, adjacent: &RegionInfo) -> bool {
    adjacent.meta().peers.iter().all(|peer| {
        stores
            .get_store(peer.store_id)
            .is_some_and(|s| !s.is_removing())
            || region.get_store_peer(peer.store_id).is_some()
    })
}

/// Structural target safety from Go checkTarget. The host additionally checks
/// its hot-region and recent-split state before calling this guard. Source health,
/// merge size thresholds, target selection and operator construction belong to
/// the merge checker lifecycle rather than AllowMerge's shared policy.
pub fn merge_target_allowed<C: SharedConfigProvider>(
    context: &RegionReplicationContext<'_, C>,
    labels: Option<&RegionLabeler>,
    stores: &StoresInfo,
    region: &RegionInfo,
    adjacent: Option<&RegionInfo>,
) -> bool {
    let Some(adjacent) = adjacent else {
        return false;
    };
    allow_merge(
        context.conf,
        Some(&context.manager.borrow()),
        labels,
        context.now,
        region,
        adjacent,
    ) && check_peer_store(stores, region, adjacent)
        && is_region_healthy(adjacent)
        && is_region_replicated(context, adjacent)
}
