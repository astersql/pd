// Copyright 2026 AsterSQL.
//! Shared scheduling access to host-owned rules, state and allocation services.
use crate::BasicCluster;
use astersql_pd_core::{BasicCluster as CoreCluster, RegionsInfo, StoresInfo};
use astersql_pd_id::{Allocator, AllocatorError};
use astersql_pd_schedule_affinity::manager::Manager as AffinityManager;
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_keyrange::Manager as KeyRangeManager;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{HotPeerSnapshot, RegionStatInformer};
use astersql_pd_statistics_utils::RWType;
use std::{convert::Infallible, future::Future};

/// Host allocation boundary. Remote adapters retain their domain error type;
/// the shared view never retries or fabricates a successful allocation.
pub trait SharedIdAllocator {
    type Error;
    fn alloc_id(&self, count: u32) -> impl Future<Output = Result<(u64, u32), Self::Error>> + Send;
}

/// Existing allocation policies: classic ignores the request count; counted
/// hosts preserve it. Both use the canonical persistent, leader-fenced allocator.
#[derive(Clone, Copy)]
pub enum IdAllocation<'a> {
    Classic(&'a Allocator),
    Counted(&'a Allocator),
}
impl IdAllocation<'_> {
    pub async fn alloc_id(self, count: u32) -> Result<(u64, u32), AllocatorError> {
        match self {
            Self::Classic(allocator) => allocator.alloc_count(1).await,
            Self::Counted(allocator) => allocator.alloc_count(count).await,
        }
    }
}

impl SharedIdAllocator for IdAllocation<'_> {
    type Error = AllocatorError;
    fn alloc_id(&self, count: u32) -> impl Future<Output = Result<(u64, u32), Self::Error>> + Send {
        (*self).alloc_id(count)
    }
}

/// Host boundary for prepare counts. Remote implementations own their I/O and
/// timeout; errors must retain identity and must never become a zero-count success.
pub trait PrepareRegionCount {
    type Error;
    fn get_prepare_region_count(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send;
}
impl PrepareRegionCount for CoreCluster {
    type Error = Infallible;
    fn get_prepare_region_count(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        std::future::ready(Ok(self.regions.total_region_count()))
    }
}

/// Common scheduler/checker observations; domain APIs expose no protobuf,
/// transport, concrete metadata backend or private runtime.
pub trait SharedCluster: BasicCluster + RegionStatInformer {
    type Config: SharedConfigProvider + ?Sized;
    type Preparation: PrepareRegionCount + ?Sized;
    type Allocation: SharedIdAllocator;

    fn get_basic_cluster(&self) -> &CoreCluster;
    fn get_shared_config(&self) -> &Self::Config;
    fn get_region_labeler(&self) -> &RegionLabeler;
    fn get_rule_manager(&self) -> &RuleManager;
    fn get_key_range_manager(&self) -> &KeyRangeManager;
    fn get_affinity_manager(&self) -> &AffinityManager;
    fn id_allocation(&self) -> &Self::Allocation;
    fn prepare_region_count_provider(&self) -> &Self::Preparation;
    fn is_scheduling_halted(&self) -> bool;

    fn alloc_id(
        &self,
        count: u32,
    ) -> impl Future<Output = Result<(u64, u32), <Self::Allocation as SharedIdAllocator>::Error>> + Send
    {
        self.id_allocation().alloc_id(count)
    }
    fn get_prepare_region_count(
        &self,
    ) -> impl Future<Output = Result<usize, <Self::Preparation as PrepareRegionCount>::Error>> + Send
    {
        self.prepare_region_count_provider()
            .get_prepare_region_count()
    }
}

/// Borrowed generation assembled by the host under its publication discipline.
/// Mutations remain on the canonical owners. Borrowing prevents replacing a
/// rule/config generation while consumers retain it; key ranges and core retain
/// their existing synchronization. Recovery state is supplied by the classic host.
pub struct SharedClusterView<
    'a,
    C: SharedConfigProvider + ?Sized,
    P: PrepareRegionCount + ?Sized,
    A: SharedIdAllocator = IdAllocation<'a>,
> {
    pub basic_cluster: &'a CoreCluster,
    pub shared_config: &'a C,
    pub region_labeler: &'a RegionLabeler,
    pub rule_manager: &'a RuleManager,
    pub key_range_manager: &'a KeyRangeManager,
    pub affinity_manager: &'a AffinityManager,
    pub statistics: &'a (dyn RegionStatInformer + Sync),
    pub id_allocation: A,
    pub prepare_region_count: &'a P,
    pub recovery_running: bool,
}
impl<C: SharedConfigProvider + ?Sized, P: PrepareRegionCount + ?Sized, A: SharedIdAllocator>
    BasicCluster for SharedClusterView<'_, C, P, A>
{
    fn stores(&self) -> &StoresInfo {
        &self.basic_cluster.stores
    }
    fn regions(&self) -> &RegionsInfo {
        &self.basic_cluster.regions
    }
}
impl<C: SharedConfigProvider + ?Sized, P: PrepareRegionCount + ?Sized, A: SharedIdAllocator>
    RegionStatInformer for SharedClusterView<'_, C, P, A>
{
    fn get_hot_peer_stat(&self, rw: RWType, region: u64, store: u64) -> Option<HotPeerSnapshot> {
        self.statistics.get_hot_peer_stat(rw, region, store)
    }
}
impl<C: SharedConfigProvider + ?Sized, P: PrepareRegionCount + ?Sized, A: SharedIdAllocator>
    SharedCluster for SharedClusterView<'_, C, P, A>
{
    type Config = C;
    type Preparation = P;
    type Allocation = A;
    fn get_basic_cluster(&self) -> &CoreCluster {
        self.basic_cluster
    }
    fn get_shared_config(&self) -> &C {
        self.shared_config
    }
    fn get_region_labeler(&self) -> &RegionLabeler {
        self.region_labeler
    }
    fn get_rule_manager(&self) -> &RuleManager {
        self.rule_manager
    }
    fn get_key_range_manager(&self) -> &KeyRangeManager {
        self.key_range_manager
    }
    fn get_affinity_manager(&self) -> &AffinityManager {
        self.affinity_manager
    }
    fn id_allocation(&self) -> &A {
        &self.id_allocation
    }
    fn prepare_region_count_provider(&self) -> &P {
        self.prepare_region_count
    }
    fn is_scheduling_halted(&self) -> bool {
        self.shared_config.snapshot().schedule.halt_scheduling || self.recovery_running
    }
}
