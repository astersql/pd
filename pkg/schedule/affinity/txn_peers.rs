// Copyright 2026 AsterSQL.
use crate::{
    group::{Availability, GroupState, validate_group_id},
    manager::Manager,
    txn::TransactionError,
};
use astersql_pd_core::{RegionInfo, StoreInfo};
use astersql_pd_errs::catalog::{ERR_AFFINITY_GROUP_CONTENT, ERR_AFFINITY_GROUP_NOT_FOUND};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{EndpointError, StorageEndpoint};

/// Owned observation token; version zero is reserved for administrative updates.
#[derive(Clone, Debug)]
pub struct GroupObservation {
    pub state: GroupState,
    pub affinity_version: u64,
}
impl Manager {
    /// Snapshots runtime scheduling flags and the version at the supplied Unix second.
    pub fn group_observation(&self, id: &str, now: u64) -> Option<GroupObservation> {
        self.groups.get(id).map(|g| GroupObservation {
            state: g.info.state(now),
            affinity_version: g.affinity_version,
        })
    }
    /// Administrative update bypassing observation version and voter-set guards.
    pub async fn update_affinity_group_peers(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        stores: &[StoreInfo],
        id: &str,
        leader: u64,
        voters: &[u64],
        now: u64,
    ) -> Result<GroupObservation, TransactionError> {
        self.update_affinity_group_peers_with_version(
            storage, paths, stores, id, 0, leader, voters, now,
        )
        .await
    }
    /// Validates before version checks; commits storage before publishing memory.
    pub async fn update_affinity_group_peers_with_version(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        stores: &[StoreInfo],
        id: &str,
        version: u64,
        leader: u64,
        voters: &[u64],
        now: u64,
    ) -> Result<GroupObservation, TransactionError> {
        if leader == 0 || voters.is_empty() {
            return Err(ERR_AFFINITY_GROUP_CONTENT
                .instantiate(["leader store ID and voter store IDs must be provided"])
                .into());
        }
        validate_group_id(id)?;
        let mut sorted = voters.to_vec();
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| w[0] == w[1]) {
            return Err(ERR_AFFINITY_GROUP_CONTENT
                .instantiate(["duplicate voter store ID"])
                .into());
        }
        if !sorted.contains(&leader) {
            return Err(ERR_AFFINITY_GROUP_CONTENT
                .instantiate(["leader must be in voter stores"])
                .into());
        }
        for voter in &sorted {
            if !stores.iter().any(|s| s.id() == *voter) {
                return Err(ERR_AFFINITY_GROUP_CONTENT
                    .instantiate([format!("store {voter} does not exist")])
                    .into());
            }
        }
        self.check_has_unavailable_store(leader, voters)?;
        let current = self
            .group_observation(id, now)
            .ok_or_else(|| ERR_AFFINITY_GROUP_NOT_FOUND.instantiate([id]))?;
        if version != 0
            && (version != current.affinity_version
                || (!current.state.regular_scheduling_allowed
                    && sorted != current.state.group.voter_store_ids))
        {
            return Ok(current);
        }
        let mut group = current.state.group;
        group.leader_store_id = leader;
        group.voter_store_ids = sorted;
        storage
            .run_in_txn(async |txn| {
                storage.save_affinity_group(txn, paths, id, &group)?;
                Ok::<_, EndpointError>(())
            })
            .await?;
        let entry = self
            .groups
            .get_mut(id)
            .expect("exclusive manager access preserves group");
        entry.info.group = group;
        entry.info.set_availability(Availability::Available, now);
        entry.info.affinity_region_count = 0;
        entry.affinity_version = entry.affinity_version.wrapping_add(1);
        Ok(self.group_observation(id, now).expect("group preserved"))
    }
    /// First eligible observation wins; invalid or failed observations leave state untouched.
    pub async fn observe_available_region(
        &mut self,
        storage: &StorageEndpoint,
        paths: &ClusterPaths,
        stores: &[StoreInfo],
        region: &RegionInfo,
        observation: Option<&GroupObservation>,
        now: u64,
    ) {
        let Some(g) = observation else { return };
        if g.state.affinity_scheduling_allowed {
            return;
        }
        let voters = region
            .voters()
            .iter()
            .map(|p| p.store_id)
            .collect::<Vec<_>>();
        let _ = self
            .update_affinity_group_peers_with_version(
                storage,
                paths,
                stores,
                &g.state.group.id,
                g.affinity_version,
                region.leader().map_or(0, |p| p.store_id),
                &voters,
                now,
            )
            .await;
    }
}
