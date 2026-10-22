// Copyright 2026 AsterSQL.
//! Store-role indexes maintained separately from the root index.
use crate::{Peer, RegionInfo, RegionItem, RegionTree, RegionsInfo};
use std::collections::HashMap;
use std::sync::Arc;

/// Membership index; witness membership is independent of voting role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegionRole {
    Leader,
    Follower,
    Learner,
    Witness,
    Pending,
}

#[derive(Default)]
pub(crate) struct SubTrees {
    pub(crate) regions: HashMap<u64, Arc<RegionInfo>>,
    overlap: Option<RegionTree>,
    roles: HashMap<(RegionRole, u64), RegionTree>,
}
fn memberships(region: &RegionInfo) -> Vec<(RegionRole, u64)> {
    let leader = region.leader().map_or(0, |p| p.id);
    region
        .voters()
        .iter()
        .map(|p| {
            (
                if p.id == leader {
                    RegionRole::Leader
                } else {
                    RegionRole::Follower
                },
                p.store_id,
            )
        })
        .chain(
            region
                .learners()
                .iter()
                .map(|p| (RegionRole::Learner, p.store_id)),
        )
        .chain(
            region
                .witnesses()
                .iter()
                .map(|p| (RegionRole::Witness, p.store_id)),
        )
        .chain(
            region
                .pending_peers()
                .iter()
                .map(|p| (RegionRole::Pending, p.store_id)),
        )
        .collect()
}
fn equal_peers(a: &[Peer], b: &[Peer]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.id == b.id && a.store_id == b.store_id)
}
fn peers_equal(a: &RegionInfo, b: &RegionInfo) -> bool {
    a.leader().map_or(0, |p| p.id) == b.leader().map_or(0, |p| p.id)
        && equal_peers(a.voters(), b.voters())
        && equal_peers(a.learners(), b.learners())
        && equal_peers(a.witnesses(), b.witnesses())
        && equal_peers(a.pending_peers(), b.pending_peers())
}
impl SubTrees {
    fn remove(&mut self, region: &RegionInfo) {
        for tree in self.roles.values_mut() {
            tree.remove(region);
        }
        if let Some(tree) = &mut self.overlap {
            tree.remove(region);
        }
        self.regions.remove(&region.meta().id);
    }
}
impl RegionsInfo {
    /// Returns a store's role index, including empty indexes retained after removal.
    pub fn role_tree(&self, role: RegionRole, store: u64) -> Option<&RegionTree> {
        self.subtrees.roles.get(&(role, store))
    }
    /// Applies a root update's origin, evictions and range-change result to role indexes.
    /// Callers serialize root/subtree updates; unordered delivery is a separate operation.
    pub fn update_sub_tree(
        &mut self,
        region: Arc<RegionInfo>,
        origin: Option<&RegionInfo>,
        overlaps: &[Arc<RegionInfo>],
        range_changed: bool,
    ) {
        let sub = &mut self.subtrees;
        if let Some(origin) = origin {
            if !range_changed && peers_equal(origin, &region) {
                if let Some(old) = sub.regions.get(&region.meta().id).cloned() {
                    for key in memberships(&region) {
                        if let Some(tree) = sub.roles.get_mut(&key) {
                            tree.update_stat(&old, &region);
                            tree.tree.replace_or_insert(RegionItem::new(region.clone()));
                        }
                    }
                    if let Some(tree) = &mut sub.overlap {
                        tree.update_stat(&old, &region);
                        tree.tree.replace_or_insert(RegionItem::new(region.clone()));
                    }
                    sub.regions.insert(region.meta().id, region);
                }
                return;
            }
            sub.remove(origin);
        }
        if range_changed {
            let evicted = if overlaps.is_empty() {
                sub.overlap
                    .as_ref()
                    .map_or_else(Vec::new, |tree| tree.overlaps(&region))
            } else {
                overlaps.to_vec()
            };
            for old in evicted {
                sub.remove(&old);
            }
        }
        sub.overlap
            .get_or_insert_with(RegionTree::with_count_ref)
            .update(region.clone());
        for key in memberships(&region) {
            sub.roles.entry(key).or_default().update(region.clone());
        }
        sub.regions.insert(region.meta().id, region);
    }
}
