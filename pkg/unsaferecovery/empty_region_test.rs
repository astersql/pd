// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, Peer, RegionEpoch, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_id::Allocator;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_member::Leadership;
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
struct Ids {
    allocator: Arc<Allocator>,
    cancel: CancellationToken,
}
#[async_trait::async_trait]
impl RecoveryIdAllocator for Ids {
    async fn allocate(&mut self) -> Result<u64, RecoveryError> {
        self.allocator
            .alloc()
            .await
            .map_err(|e| RecoveryError::Backend(e.to_string()))
    }
}
async fn ids(store: Arc<dyn MetaStore>, step: u64) -> (Arc<Leadership>, Ids) {
    let term = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            2200,
            b"empty-owner".to_vec(),
            30,
        )
        .await
        .unwrap(),
    );
    let cancel = CancellationToken::new();
    let allocator = Arc::new(Allocator::new(store, term.clone(), cancel.clone(), step).unwrap());
    (term, Ids { allocator, cancel })
}
fn controller(regions: Vec<RegionMeta>) -> (Controller, RecoveryIndex, StoresInfo) {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(22)));
    c.auto_detect = true;
    c.expected_reports.insert(
        1,
        Some(StoreReport {
            step: 22,
            peers: regions
                .into_iter()
                .map(|region| PeerReport {
                    region: Some(region),
                    ..Default::default()
                })
                .collect(),
        }),
    );
    let stores = StoresInfo::new();
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    })));
    let index = c.build_region_index().unwrap();
    (c, index, stores)
}
fn region(id: u64, start: &[u8], end: &[u8], version: u64) -> RegionMeta {
    RegionMeta {
        id,
        key_range: KeyRange {
            start_key: start.to_vec(),
            end_key: end.to_vec(),
        },
        epoch: Some(RegionEpoch {
            version,
            conf_ver: 1,
        }),
        peers: vec![Peer {
            id: 100 + id,
            store_id: 1,
            ..Default::default()
        }],
        ..Default::default()
    }
}
async fn boundary(store: &dyn MetaStore) -> Option<u64> {
    store
        .read(Read::range(
            ClusterPaths::new(2200).alloc_id_path().as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .first()
        .map(|e| u64::from_be_bytes(e.value.as_slice().try_into().unwrap()))
}
#[tokio::test]
async fn mcs_022_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let (term, mut ids) = ids(store.clone(), 1000).await;
    let (c, index, stores) = controller(vec![region(7, b"b", b"d", 10)]);
    let batch = c
        .generate_create_empty_region_plan(&index, &stores, &mut ids)
        .await
        .unwrap();
    let creates = &batch
        .plans
        .get(&1)
        .expect("head and tail holes need actual allocated Region plans")
        .creates;
    assert_eq!(creates.len(), 2);
    assert_eq!(creates[0].id, 1);
    assert_eq!(creates[0].peers[0].id, 2);
    assert_eq!(creates[1].id, 3);
    assert_eq!(creates[1].peers[0].id, 4);
    assert_eq!(
        creates[0].key_range,
        KeyRange {
            start_key: vec![],
            end_key: b"b".to_vec()
        }
    );
    assert_eq!(
        creates[1].key_range,
        KeyRange {
            start_key: b"d".to_vec(),
            end_key: vec![]
        }
    );
    assert_eq!(boundary(store.as_ref()).await, Some(1000));
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_022_full_space_and_contiguous_ranges_use_real_id_positions() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let (term, mut ids) = ids(store.clone(), 1000).await;
    let (c, index, stores) = controller(vec![region(7, b"", b"b", 10), region(8, b"b", b"", 9)]);
    assert!(
        !c.generate_create_empty_region_plan(&index, &stores, &mut ids)
            .await
            .unwrap()
            .has_work()
    );
    assert_eq!(boundary(store.as_ref()).await, None);
    let (c, index, stores) = controller(vec![]);
    let batch = c
        .generate_create_empty_region_plan(&index, &stores, &mut ids)
        .await
        .unwrap();
    let r = &batch.plans[&1].creates[0];
    assert_eq!((r.id, r.peers[0].id), (1, 2));
    assert!(r.key_range.start_key.is_empty() && r.key_range.end_key.is_empty());
    assert_eq!(batch.newly_created_regions, [1].into());
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_022_paranoid_rejection_consumes_ids_without_publishing_then_opt_out_works() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let (term, mut ids) = ids(store.clone(), 1000).await;
    let (mut c, index, stores) =
        controller(vec![region(7, b"b", b"d", 10), region(8, b"a", b"e", 9)]);
    let original = c.expected_reports[&1].clone();
    assert!(
        c.generate_create_empty_region_plan(&index, &stores, &mut ids)
            .await
            .unwrap_err()
            .to_string()
            .starts_with("Find overlap peer")
    );
    assert_eq!(c.expected_reports[&1], original);
    assert_eq!(boundary(store.as_ref()).await, Some(1000));
    assert_eq!(ids.allocator.alloc().await.unwrap(), 5);
    c.disable_paranoid_check = true;
    let batch = c
        .generate_create_empty_region_plan(&index, &stores, &mut ids)
        .await
        .unwrap();
    assert_eq!(batch.newly_created_regions, [6, 8].into());
    assert_eq!(batch.plans[&1].creates.len(), 2);
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_022_tiflash_fallback_and_no_tikv_do_not_use_private_counters() {
    use astersql_pd_core::StoreLabel;
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let (term, mut ids) = ids(store.clone(), 1000).await;
    let (mut c, index, stores) = controller(vec![]);
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        labels: vec![StoreLabel {
            key: "engine".into(),
            value: "tiflash".into(),
        }],
        ..Default::default()
    })));
    assert_eq!(
        c.generate_create_empty_region_plan(&index, &stores, &mut ids)
            .await
            .unwrap_err()
            .to_string(),
        "can't find available store(exclude tiflash) to create new region"
    );
    assert_eq!(boundary(store.as_ref()).await, None);
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 2,
        ..Default::default()
    })));
    c.expected_reports.insert(2, Some(StoreReport::default()));
    let batch = c
        .generate_create_empty_region_plan(&index, &stores, &mut ids)
        .await
        .unwrap();
    assert_eq!(batch.plans[&2].creates[0].peers[0].store_id, 2);
    assert!(!batch.plans.contains_key(&1));
    term.resign().await.unwrap();
}
struct RevokeAfterRegion {
    ids: Ids,
    term: Option<Arc<Leadership>>,
}
#[async_trait::async_trait]
impl RecoveryIdAllocator for RevokeAfterRegion {
    async fn allocate(&mut self) -> Result<u64, RecoveryError> {
        let id = self.ids.allocate().await?;
        if let Some(term) = self.term.take() {
            term.resign()
                .await
                .map_err(|e| RecoveryError::Backend(e.to_string()))?;
        }
        Ok(id)
    }
}
#[tokio::test]
async fn mcs_022_revoked_term_after_region_id_leaves_no_candidate() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let (term, ids) = ids(store.clone(), 1).await;
    let mut ids = RevokeAfterRegion {
        ids,
        term: Some(term),
    };
    let (c, index, stores) = controller(vec![]);
    assert!(
        c.generate_create_empty_region_plan(&index, &stores, &mut ids)
            .await
            .is_err()
    );
    assert_eq!(boundary(store.as_ref()).await, Some(1));
    let next = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            2200,
            b"replacement".to_vec(),
            30,
        )
        .await
        .unwrap(),
    );
    let replacement = Allocator::new(store, next.clone(), CancellationToken::new(), 1).unwrap();
    assert_eq!(replacement.alloc().await.unwrap(), 2);
    next.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_022_cancel_actual_pending_allocation_does_not_publish() {
    let mut fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let (term, mut ids) = ids(store.clone(), 1000).await;
    let cancel = ids.cancel.clone();
    let (c, index, stores) = controller(vec![]);
    let paused = fixture.suspend();
    let mut future = Box::pin(c.generate_create_empty_region_plan(&index, &stores, &mut ids));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), future.as_mut())
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(500), future)
            .await
            .unwrap()
            .is_err()
    );
    drop(paused);
    assert_eq!(boundary(store.as_ref()).await, None);
    term.resign().await.unwrap();
}
