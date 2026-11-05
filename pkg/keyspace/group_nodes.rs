// Copyright 2026 AsterSQL.
use crate::{
    GroupCatalogError, KeyspaceGroupManager,
    group_catalog::{commit_error, put_index},
};
use astersql_pd_balancer::RoundRobin;
use astersql_pd_mcs_constant::DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY;
use astersql_pd_storage_domain::{
    Base, CallbackTxn, KeyspaceGroup, KeyspaceGroupMember, StorageEndpoint, StoreError,
};
use astersql_pd_typeutil::{equal_base_urls, trim_scheme};
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
/// Allocation and persisted membership failures.
#[derive(Debug, thiserror::Error)]
pub enum GroupNodesError {
    #[error(
        "[PD:keyspace:ErrKeyspaceGroupPrimaryNotFound]primary of keyspace group does not exist"
    )]
    PrimaryNotFound,
    #[error(transparent)]
    Catalog(#[from] GroupCatalogError),
    #[error(transparent)]
    Leadership(#[from] astersql_pd_election::ElectionError),
    #[error("[PD:keyspace:ErrKeyspaceGroupNotExists]keyspace group {0} does not exist")]
    NotExists(u32),
    #[error("[PD:keyspace:ErrKeyspaceGroupInMerging]keyspace group {0} is in merging state")]
    InMerging(u32),
    #[error("[PD:keyspace:ErrNoAvailableNode]no available node")]
    NoAvailableNode,
    #[error("[PD:keyspace:ErrNodeNotInKeyspaceGroup]the tso node is not in this keyspace group")]
    NodeNotInGroup,
    #[error("context canceled")]
    Cancelled,
    #[error("context deadline exceeded")]
    Deadline,
}
pub(crate) fn unique_tso_nodes(nodes: &[String]) -> BTreeMap<String, String> {
    let mut unique = BTreeMap::new();
    for node in nodes {
        unique
            .entry(trim_scheme(node).into())
            .or_insert_with(|| node.clone());
    }
    unique
}
async fn checked_group(
    endpoint: &StorageEndpoint,
    txn: &mut CallbackTxn<'_>,
    paths: &astersql_pd_keypath::ClusterPaths,
    id: u32,
) -> Result<KeyspaceGroup, GroupNodesError> {
    let group = endpoint
        .load_keyspace_group(txn, paths, id)
        .await
        .map_err(GroupCatalogError::from)?
        .ok_or(GroupNodesError::NotExists(id))?;
    if group.is_splitting() {
        return Err(GroupCatalogError::InSplit(id).into());
    }
    if group.is_merging() {
        return Err(GroupNodesError::InMerging(id));
    }
    Ok(group)
}
async fn commit_alloc(
    txn: CallbackTxn<'_>,
    cancel: &CancellationToken,
    term: Option<&astersql_pd_election::Leadership>,
) -> Result<(), GroupNodesError> {
    if cancel.is_cancelled() {
        return Err(GroupNodesError::Cancelled);
    }
    let commit = async {
        if let Some(term) = term {
            let (compares, writes) = txn.into_parts();
            let commit = term.transact(compares, writes).await?;
            if !commit.succeeded {
                return Err(GroupCatalogError::Conflict.into());
            }
            Ok(())
        } else {
            txn.commit::<Infallible>()
                .await
                .map_err(commit_error)
                .map_err(Into::into)
        }
    };
    tokio::select! {biased;result=commit=>result,_=cancel.cancelled()=>Err(GroupCatalogError::Store(StoreError::Unavailable).into())}
}

impl KeyspaceGroupManager {
    /// Use the host's shared discovery balancer; no discovery worker is spawned here.
    pub fn with_nodes_balancer(mut self, nodes: Arc<RoundRobin<String>>) -> Self {
        self.nodes = nodes;
        self
    }
    /// Count base URLs, treating HTTP and HTTPS spellings as the same service.
    pub fn get_nodes_count(&self) -> usize {
        unique_tso_nodes(&self.nodes.get_all()).len()
    }
    /// Return the registered spelling, for the HTTP adapter's candidate validation.
    pub fn registered_node(&self, node: &str) -> Option<String> {
        self.nodes
            .get_all()
            .into_iter()
            .find(|candidate| equal_base_urls(candidate, node))
    }
    /// Allocate with Go's one-second budget and ten-millisecond selection ticks.
    /// None means the persisted healthy membership already satisfies the automatic path.
    /// Explicit existing members are supplied by a caller that validated registration.
    pub async fn alloc_nodes(
        &self,
        id: u32,
        existing: Option<BTreeSet<String>>,
        desired: usize,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<KeyspaceGroupMember>>, GroupNodesError> {
        self.alloc_nodes_in_term(id, existing, desired, cancel, None)
            .await
    }
    pub(crate) async fn alloc_nodes_in_term(
        &self,
        id: u32,
        existing: Option<BTreeSet<String>>,
        desired: usize,
        cancel: &CancellationToken,
        term: Option<&crate::GroupReconcileTerm>,
    ) -> Result<Option<Vec<KeyspaceGroupMember>>, GroupNodesError> {
        let mut index = self.groups.write().await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let mut ticker = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_millis(10),
            Duration::from_millis(10),
        );
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut group = tokio::select! {biased;_ =cancel.cancelled()=>return Err(GroupNodesError::Cancelled),result=checked_group(&endpoint,&mut txn,&self.paths,id)=>result?};
        let available = unique_tso_nodes(&self.nodes.get_all());
        let mut existing = if let Some(existing) = existing {
            if desired > available.len() {
                return Err(GroupNodesError::NoAvailableNode);
            }
            existing
        } else {
            let existing: BTreeSet<_> = group
                .members
                .iter()
                .flatten()
                .filter_map(|member| available.get(trim_scheme(&member.address)).cloned())
                .collect();
            let count = existing.len();
            if (count != 0
                && count == group.members.as_ref().map_or(0, Vec::len)
                && count == available.len())
                || count >= desired
            {
                commit_alloc(txn, cancel, term.map(|term| term.leadership.as_ref())).await?;
                self.publish_allocated(term, &mut index, group)?;
                return Ok(None);
            }
            existing
        };
        let mut nodes: Vec<_> = existing
            .iter()
            .map(|address| KeyspaceGroupMember {
                address: address.clone(),
                priority: i64::from(DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY),
            })
            .collect();
        while existing.len() < desired {
            tokio::select! {biased;_=cancel.cancelled()=>return Err(GroupNodesError::Cancelled),_=tokio::time::sleep_until(deadline)=>return Err(GroupNodesError::Deadline),_=ticker.tick()=>{}}
            if self.get_nodes_count() == 0 {
                return Err(GroupNodesError::NoAvailableNode);
            }
            if existing.len() == available.len() {
                break;
            }
            let address = self.nodes.next();
            if address.is_empty() {
                return Err(GroupNodesError::NoAvailableNode);
            }
            if existing.iter().any(|old| equal_base_urls(old, &address)) {
                continue;
            }
            existing.insert(address.clone());
            nodes.push(KeyspaceGroupMember {
                address,
                priority: i64::from(DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY),
            });
        }
        group.members = Some(nodes.clone());
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &group)
            .map_err(GroupCatalogError::from)?;
        commit_alloc(txn, cancel, term.map(|term| term.leadership.as_ref())).await?;
        self.publish_allocated(term, &mut index, group)?;
        Ok(Some(nodes))
    }
    fn publish_allocated(
        &self,
        term: Option<&crate::GroupReconcileTerm>,
        index: &mut crate::group_catalog::GroupIndex,
        group: KeyspaceGroup,
    ) -> Result<(), GroupNodesError> {
        if let Some(term) = term {
            self.with_reconcile_term(term, || put_index(index, group))
                .map_err(GroupCatalogError::Store)?;
        } else {
            put_index(index, group);
        }
        Ok(())
    }
    async fn change_nodes<F>(&self, id: u32, change: F) -> Result<(), GroupNodesError>
    where
        F: FnOnce(&mut KeyspaceGroup) -> Result<(), GroupNodesError> + Send,
    {
        let mut index = self.groups.write().await;
        let endpoint = StorageEndpoint::new(self.store.clone());
        let mut txn = Base::new(self.store.as_ref()).begin_txn();
        let mut group = checked_group(&endpoint, &mut txn, &self.paths, id).await?;
        change(&mut group)?;
        endpoint
            .save_keyspace_group(&mut txn, &self.paths, &group)
            .map_err(GroupCatalogError::from)?;
        txn.commit::<Infallible>().await.map_err(commit_error)?;
        put_index(&mut index, group);
        Ok(())
    }
    /// Persist the supplied spellings and order, like Go's domain method.
    /// The HTTP adapter validates each candidate with registered_node before calling.
    pub async fn set_nodes(&self, id: u32, nodes: &[String]) -> Result<(), GroupNodesError> {
        self.change_nodes(id, |group| {
            group.members = Some(
                nodes
                    .iter()
                    .map(|address| KeyspaceGroupMember {
                        address: address.clone(),
                        priority: i64::from(DEFAULT_KEYSPACE_GROUP_REPLICA_PRIORITY),
                    })
                    .collect(),
            );
            Ok(())
        })
        .await
    }
    /// Change every equivalent member spelling while retaining all other members.
    pub async fn set_priority(
        &self,
        id: u32,
        node: &str,
        priority: i64,
    ) -> Result<(), GroupNodesError> {
        self.change_nodes(id, |group| {
            let mut found = false;
            for member in group.members.iter_mut().flatten() {
                if member.is_address_equivalent(node) {
                    member.priority = priority;
                    found = true;
                }
            }
            if !found {
                return Err(GroupNodesError::NodeNotInGroup);
            }
            Ok(())
        })
        .await
    }
}

impl KeyspaceGroupManager {
    /// Refresh the registered candidates from the same authoritative host store.
    pub async fn refresh_registered_nodes(&self) -> Result<(), GroupNodesError> {
        use astersql_pd_storage_domain::{Consistency, Read};
        let rows = self
            .store
            .read(Read::prefix(
                self.paths.service_path("tso").as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .map_err(GroupCatalogError::from)?;
        let mut nodes = Vec::new();
        for row in rows.entries {
            nodes.push(
                crate::group_reconcile::registry_address(&row.value)
                    .map_err(GroupCatalogError::from)?,
            );
        }
        for node in self.nodes.get_all() {
            if !nodes.contains(&node) {
                self.nodes.delete(node);
            }
        }
        for node in nodes {
            self.nodes.put(node);
        }
        Ok(())
    }
    /// Return the opaque election participant for conversion by the transport adapter.
    pub async fn primary_record(&self, id: u32) -> Result<Vec<u8>, GroupNodesError> {
        use astersql_pd_storage_domain::{Consistency, Read};
        if self.get_group(id).await?.is_none() {
            return Err(GroupNodesError::NotExists(id));
        }
        let key = self
            .paths
            .election_path(Some(&astersql_pd_keypath::MsParam {
                service_name: "tso".into(),
                group_id: id,
            }));
        let rows = self
            .store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .map_err(GroupCatalogError::from)?;
        rows.entries
            .into_iter()
            .next()
            .map(|r| r.value)
            .ok_or(GroupNodesError::PrimaryNotFound)
    }
}

impl KeyspaceGroupManager {
    /// Host-driven periodic reconciliation; commits use the injected term-fenced store.
    pub async fn reconcile_registered_nodes(
        &self,
        cancel: &CancellationToken,
    ) -> Result<crate::GroupReconcileReport, GroupNodesError> {
        self.refresh_registered_nodes().await?;
        let nodes = unique_tso_nodes(&self.nodes.get_all());
        if nodes.is_empty() {
            return Ok(crate::GroupReconcileReport::default());
        }
        let candidates = self
            .get_groups(0, 0)
            .await?
            .into_iter()
            .filter(|g| {
                !g.is_splitting()
                    && !g.is_merging()
                    && crate::group_reconcile::needs_allocation(
                        g.members.as_deref().unwrap_or_default(),
                        &nodes,
                    )
            })
            .map(|g| g.id)
            .collect();
        Ok(self.reconcile_candidate_ids(candidates, cancel, None).await)
    }
}
