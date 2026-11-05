// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, GroupReconcileReport, GroupReconcileTerm, GroupSplitError, Kernel,
    KeyspaceGroupManager, group_nodes::unique_tso_nodes, group_reconcile::needs_allocation,
};
use astersql_pd_mcs_constant::{
    DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT, MAX_KEYSPACE_GROUP_COUNT_IN_USE,
};
use astersql_pd_storage_domain::KeyspaceGroup;
use std::collections::BTreeSet;
use tokio_util::sync::CancellationToken;
/// Production threshold from Go's defaultKeyspaceCountSplitThreshold.
pub const AUTO_SPLIT_KEYSPACE_THRESHOLD: usize = 40_000;
/// One patrol attempts at most one split, then reconciles the loaded membership view.
#[derive(Debug, Default)]
pub struct GroupPatrolReport {
    pub split: Option<(u32, u32)>,
    pub split_error: Option<GroupSplitError>,
    pub reconcile: GroupReconcileReport,
}
/// Choose the smallest unused ID in [1,max_count), ignoring IDs above that boundary.
pub fn find_next_available_group_id(groups: &[KeyspaceGroup], max_count: u32) -> Option<u32> {
    let used: BTreeSet<_> = groups
        .iter()
        .filter(|group| group.id < max_count)
        .map(|group| group.id)
        .collect();
    (1..max_count).find(|id| !used.contains(id))
}
impl KeyspaceGroupManager {
    /// One standalone patrol; the host controls scheduling and operation cancellation.
    pub async fn patrol_auto_split(
        &self,
        cancel: &CancellationToken,
    ) -> Result<GroupPatrolReport, GroupCatalogError> {
        self.patrol_snapshot(cancel, None).await
    }
    /// One leadership-owned patrol, including an atomic epoch fence on the split.
    pub async fn patrol_auto_split_in_term(
        &self,
        term: &GroupReconcileTerm,
    ) -> Result<GroupPatrolReport, GroupCatalogError> {
        self.patrol_snapshot(&term.cancel, Some(term)).await
    }
    async fn patrol_snapshot(
        &self,
        cancel: &CancellationToken,
        term: Option<&GroupReconcileTerm>,
    ) -> Result<GroupPatrolReport, GroupCatalogError> {
        let mut report = GroupPatrolReport::default();
        if cancel.is_cancelled() {
            return Ok(report);
        }
        if let Some(term) = term {
            self.with_reconcile_term(term, || ())
                .map_err(GroupCatalogError::Store)?;
        }
        let groups = self.get_groups(0, 0).await?;
        if let Some(target) = find_next_available_group_id(&groups, MAX_KEYSPACE_GROUP_COUNT_IN_USE)
        {
            let mut sorted: Vec<_> = groups.iter().collect();
            sorted.sort_by(|a, b| {
                b.keyspaces
                    .as_ref()
                    .map_or(0, Vec::len)
                    .cmp(&a.keyspaces.as_ref().map_or(0, Vec::len))
                    .then_with(|| a.id.cmp(&b.id))
            });
            for group in sorted {
                if cancel.is_cancelled() {
                    break;
                }
                if group.is_splitting()
                    || group.is_merging()
                    || group.members.as_ref().map_or(0, Vec::len)
                        < DEFAULT_KEYSPACE_GROUP_REPLICA_COUNT
                {
                    continue;
                }
                let keys = group.keyspaces.as_deref().unwrap_or_default();
                if keys.len() <= AUTO_SPLIT_KEYSPACE_THRESHOLD {
                    continue;
                }
                let moved: Vec<_> = keys[keys.len() / 2..]
                    .iter()
                    .copied()
                    .filter(|&id| !Kernel::current().is_protected_id(id))
                    .collect();
                if moved.is_empty() {
                    continue;
                }
                match self
                    .split_group_in_term(group.id, target, &moved, &[], Some(cancel), term)
                    .await
                {
                    Ok(()) => report.split = Some((group.id, target)),
                    Err(error) => report.split_error = Some(error),
                }
                break;
            }
        }
        // Go defers this reconciliation after loading, even if no split or a failed split occurs.
        if !cancel.is_cancelled() {
            let nodes = unique_tso_nodes(&self.nodes.get_all());
            if !nodes.is_empty() {
                let candidates = groups
                    .iter()
                    .filter(|group| {
                        !group.is_splitting()
                            && !group.is_merging()
                            && needs_allocation(
                                group.members.as_deref().unwrap_or_default(),
                                &nodes,
                            )
                    })
                    .map(|group| group.id)
                    .collect();
                report.reconcile = self.reconcile_candidate_ids(candidates, cancel, term).await;
            }
        }
        Ok(report)
    }
}
