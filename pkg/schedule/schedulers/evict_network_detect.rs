// Copyright 2026 AsterSQL.
//! Network candidate evaluation over real store snapshots, without eviction side effects.
use astersql_pd_core::{BasicCluster, StoreInfo};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Symmetric issue edges. Unknown targets remain valid score-map keys.
pub type ProblematicNetwork = HashMap<u64, HashSet<u64>>;

/// Remove known removed targets and every tracked slow target; unknown targets remain.
/// Recovery timestamps do not affect detection, so only recovery-map keys are required.
pub fn filter_network_slow_scores(
    cluster: &BasicCluster,
    scores: &HashMap<u64, u64>,
    recovering: &HashSet<u64>,
) -> HashMap<u64, u64> {
    scores
        .iter()
        .filter(|(id, _)| {
            !recovering.contains(id)
                && !cluster
                    .stores
                    .get_store(**id)
                    .is_some_and(|s| s.is_removed())
        })
        .map(|(&id, &score)| (id, score))
        .collect()
}

/// Build undirected issue edges from non-recovering reporters with at least two scores.
/// Callers supply live snapshots; detection below performs that population selection.
pub fn build_problematic_network_map(
    cluster: &BasicCluster,
    stores: &[Arc<StoreInfo>],
    recovering: &HashSet<u64>,
) -> ProblematicNetwork {
    let mut graph = ProblematicNetwork::new();
    for store in stores {
        if recovering.contains(&store.id()) {
            continue;
        }
        let scores = filter_network_slow_scores(cluster, &store.network_slow_scores(), recovering);
        if scores.len() < 2 {
            continue;
        }
        for (id, score) in scores {
            if score >= 95 {
                graph.entry(id).or_default().insert(store.id());
                graph.entry(store.id()).or_default().insert(id);
            }
        }
    }
    graph
}

/// Skip stores without an issue edge or already present in the recovery map.
pub fn should_skip_store_evaluation(
    store: &StoreInfo,
    graph: &ProblematicNetwork,
    recovering: &HashSet<u64>,
) -> bool {
    !graph.contains_key(&store.id()) || recovering.contains(&store.id())
}

/// Require three filtered scores and either full live-peer consensus or two
/// fluctuating (>=10) scores outside the ambiguous issue edges.
pub fn is_network_slow_store(
    cluster: &BasicCluster,
    store: &StoreInfo,
    all_stores: &[Arc<StoreInfo>],
    graph: &ProblematicNetwork,
    recovering: &HashSet<u64>,
) -> bool {
    let scores = filter_network_slow_scores(cluster, &store.network_slow_scores(), recovering);
    if scores.len() <= 2 {
        return false;
    }
    let live_ids: HashSet<_> = all_stores.iter().map(|s| s.id()).collect();
    let live_recovering = recovering.intersection(&live_ids).count();
    let edges = graph.get(&store.id());
    // Go uses signed int subtraction, including degenerate direct-helper inputs.
    if edges.map_or(0, HashSet::len) as i128
        >= all_stores.len() as i128 - 1 - live_recovering as i128
    {
        return true;
    }
    scores
        .iter()
        .filter(|(id, score)| !edges.is_some_and(|e| e.contains(id)) && **score >= 10)
        .take(2)
        .count()
        >= 2
}

/// Return sorted live candidates without changing tracking, persistence or leader state.
/// Recheck registration/removal after evaluation to narrow the stale-snapshot window.
pub fn detect_network_slow_candidates(
    cluster: &BasicCluster,
    recovering: &HashSet<u64>,
) -> Vec<u64> {
    let live: Vec<_> = cluster
        .stores
        .get_stores()
        .into_iter()
        .filter(|s| !s.is_removed())
        .collect();
    let graph = build_problematic_network_map(cluster, &live, recovering);
    let mut candidates: Vec<_> = live
        .iter()
        .filter(|s| {
            !should_skip_store_evaluation(s, &graph, recovering)
                && is_network_slow_store(cluster, s, &live, &graph, recovering)
                && cluster
                    .stores
                    .get_store(s.id())
                    .is_some_and(|current| !current.is_removed())
        })
        .map(|s| s.id())
        .collect();
    candidates.sort_unstable();
    candidates
}
