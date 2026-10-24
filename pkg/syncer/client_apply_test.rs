// Copyright 2026 AsterSQL.
use super::{
    ClientApply, HistoryBuffer, HistoryIndex as I, SyncApplyResponse, SyncRegionStats,
    SyncResponseError,
};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionBucketMeta, RegionEpoch, RegionMeta, RegionSource,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::Storage;
use astersql_pd_storage_kv::{MemoryStore, MetaStore};
use std::sync::Arc;

fn region(id: u64, version: u64) -> RegionMeta {
    RegionMeta {
        id,
        key_range: KeyRange {
            start_key: vec![0, 1],
            end_key: vec![0, 2],
        },
        epoch: Some(RegionEpoch {
            version,
            conf_ver: 1,
        }),
        peers: vec![Peer {
            id: id + 1,
            store_id: 1,
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn history() -> HistoryBuffer {
    HistoryBuffer::with_config(10, 10, 1).unwrap()
}
fn response() -> SyncApplyResponse {
    SyncApplyResponse {
        start_index: I(10),
        regions: vec![region(1, 5), region(2, 4), region(3, 6)],
        stats: vec![
            SyncRegionStats {
                written_bytes: 101,
                ..Default::default()
            },
            SyncRegionStats::default(),
            SyncRegionStats {
                written_bytes: 303,
                ..Default::default()
            },
        ],
        buckets: vec![
            Some(RegionBucketMeta::default()),
            Some(RegionBucketMeta::default()),
            Some(RegionBucketMeta {
                version: 7,
                keys: vec![],
            }),
        ],
        leaders: vec![Some(Peer {
            id: 2,
            store_id: 1,
            ..Default::default()
        })],
        ..Default::default()
    }
}

async fn overlap_contract(store: Arc<dyn MetaStore>) {
    let storage = Storage::new(store);
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = history();
    let out = client
        .apply(response(), &mut cluster, &storage, &paths, &mut h)
        .await
        .unwrap();
    assert!(out.handled);
    assert_eq!(
        (out.applied, out.stale, out.save_errors, out.delete_errors),
        (2, 1, 0, 0)
    );
    assert_eq!(h.next_index(), I(12));
    assert_eq!(
        h.records_from(I(10))
            .iter()
            .map(|r| r.meta().id)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(h.get(I(10)).unwrap().leader().unwrap().id, 2);
    assert!(cluster.regions.get_region(1).is_none());
    assert!(cluster.regions.get_region(2).is_none());
    let final_region = cluster.regions.get_region(3).unwrap();
    assert_eq!(final_region.stats().written_bytes, 303);
    assert_eq!(final_region.buckets().unwrap().version, 7);
    assert!(final_region.leader().is_none());
    assert_eq!(final_region.source(), RegionSource::Sync);
    assert!(storage.load_region(&paths, 1).await.unwrap().is_none());
    assert!(storage.load_region(&paths, 2).await.unwrap().is_none());
    assert_eq!(
        storage.load_region(&paths, 3).await.unwrap(),
        Some(region(3, 6))
    );
    assert!(client.is_running());
    assert!(!client.full_syncing());
}
#[tokio::test]
async fn state_task_37_client_apply_memory_overlap_oracle() {
    overlap_contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn state_task_37_client_apply_etcd_overlap_oracle() {
    let fixture = super::persist_fixture::Fixture::start();
    overlap_contract(Arc::new(fixture.store().await)).await;
}
#[tokio::test]
async fn state_task_37_client_apply_error_does_not_mutate() {
    let storage = Storage::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = history();
    h.reset_with_index(I(10));
    client
        .apply(
            SyncApplyResponse {
                start_index: I(10),
                ..Default::default()
            },
            &mut cluster,
            &storage,
            &paths,
            &mut h,
        )
        .await
        .unwrap();
    assert!(client.is_running());
    let mut resp = response();
    resp.start_index = I(999);
    resp.error = Some(SyncResponseError {
        code: 0,
        message: "server stopped".into(),
    });
    let out = client
        .apply(resp, &mut cluster, &storage, &paths, &mut h)
        .await
        .unwrap();
    assert!(!out.handled);
    assert_eq!(h.next_index(), I(10));
    assert!(h.is_empty());
    assert!(!client.is_running());
    assert!(cluster.regions.get_region(1).is_none());
    assert!(storage.load_region(&paths, 1).await.unwrap().is_none());
}
#[tokio::test]
async fn state_task_37_client_apply_missing_arrays_and_zero_leader() {
    let storage = Storage::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(37);
    for mismatch in [true, false] {
        let mut cluster = BasicCluster::new();
        let mut client = ClientApply::default();
        let mut h = history();
        let mut resp = response();
        resp.stats.truncate(usize::from(mismatch));
        resp.buckets.truncate(usize::from(mismatch));
        resp.leaders[0].as_mut().unwrap().id = 0;
        client
            .apply(resp, &mut cluster, &storage, &paths, &mut h)
            .await
            .unwrap();
        assert_eq!(h.next_index(), I(12));
        for record in h.records_from(I(10)) {
            assert_eq!(record.stats().written_bytes, 0);
            assert!(record.buckets().is_none());
            assert!(record.leader().is_none());
        }
    }
}
#[tokio::test]
async fn state_task_37_client_apply_full_sync_positional_completion() {
    let storage = Storage::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = history();
    h.reset_with_index(I(42));
    for start in [0, 42] {
        let out = client
            .apply(
                SyncApplyResponse {
                    start_index: I(start),
                    regions: vec![region(start + 1, start + 5)],
                    ..Default::default()
                },
                &mut cluster,
                &storage,
                &paths,
                &mut h,
            )
            .await
            .unwrap();
        assert!(out.handled);
        assert!(client.full_syncing());
        assert!(!client.is_running());
        assert_eq!(h.next_index(), I(0));
        assert!(h.is_empty());
    }
    client
        .apply(
            SyncApplyResponse {
                error: Some(SyncResponseError {
                    code: 0,
                    message: "stopped".into(),
                }),
                start_index: I(900),
                ..Default::default()
            },
            &mut cluster,
            &storage,
            &paths,
            &mut h,
        )
        .await
        .unwrap();
    assert!(client.full_syncing());
    assert_eq!(h.next_index(), I(0));
    client
        .apply(
            SyncApplyResponse {
                start_index: I(43),
                ..Default::default()
            },
            &mut cluster,
            &storage,
            &paths,
            &mut h,
        )
        .await
        .unwrap();
    assert!(!client.full_syncing());
    assert!(client.is_running());
    assert_eq!(h.next_index(), I(43));
    client.reset_stream();
    assert!(!client.is_running());
    assert!(!client.full_syncing());
}

#[derive(Default)]
struct FaultStore {
    inner: MemoryStore,
    fault: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl MetaStore for FaultStore {
    async fn read(
        &self,
        request: astersql_pd_storage_kv::Read,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<astersql_pd_storage_kv::Compare>,
        writes: Vec<astersql_pd_storage_kv::Mutation>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Commit> {
        use astersql_pd_storage_kv::{Mutation, StoreError};
        use std::sync::atomic::Ordering;
        let fault = self.fault.load(Ordering::SeqCst);
        if writes.iter().any(|op| {
            matches!(
                (fault, op),
                (1, Mutation::Put { .. }) | (2, Mutation::Delete { .. })
            )
        }) {
            return Err(StoreError::Backend);
        }
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<astersql_pd_storage_kv::Compare>,
        then_writes: Vec<astersql_pd_storage_kv::Mutation>,
        else_writes: Vec<astersql_pd_storage_kv::Mutation>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<astersql_pd_storage_kv::Compare>,
        then_ops: Vec<astersql_pd_storage_kv::RawTxnOp>,
        else_ops: Vec<astersql_pd_storage_kv::RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn astersql_pd_storage_kv::MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(
        &self,
        ttl_seconds: i64,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(
        &self,
        lease: i64,
    ) -> astersql_pd_storage_kv::Result<astersql_pd_storage_kv::Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: i64) -> astersql_pd_storage_kv::Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: i64) -> astersql_pd_storage_kv::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: i64) -> astersql_pd_storage_kv::Result<()> {
        self.inner.compact(revision).await
    }
}

#[tokio::test]
async fn state_task_37_client_apply_storage_failures_keep_cache_and_cleanup() {
    use std::sync::atomic::Ordering;
    for fault in [1, 2] {
        let backend = Arc::new(FaultStore::default());
        let storage = Storage::new(backend.clone());
        let paths = ClusterPaths::new(37);
        let mut cluster = BasicCluster::new();
        let mut client = ClientApply::default();
        let mut h = history();
        client
            .apply(
                SyncApplyResponse {
                    start_index: I(10),
                    regions: vec![region(1, 5)],
                    ..Default::default()
                },
                &mut cluster,
                &storage,
                &paths,
                &mut h,
            )
            .await
            .unwrap();
        backend.fault.store(fault, Ordering::SeqCst);
        let out = client
            .apply(
                SyncApplyResponse {
                    start_index: I(11),
                    regions: vec![region(3, 6)],
                    ..Default::default()
                },
                &mut cluster,
                &storage,
                &paths,
                &mut h,
            )
            .await
            .unwrap();
        assert_eq!(out.applied, 1);
        assert!(cluster.regions.get_region(1).is_none());
        assert!(cluster.regions.get_region(3).is_some());
        if fault == 1 {
            assert_eq!((out.save_errors, out.delete_errors), (1, 0));
            assert_eq!(h.next_index(), I(11));
            assert!(storage.load_region(&paths, 1).await.unwrap().is_none());
            assert!(storage.load_region(&paths, 3).await.unwrap().is_none());
        } else {
            assert_eq!((out.save_errors, out.delete_errors), (0, 1));
            assert_eq!(h.next_index(), I(12));
            assert_eq!(
                storage.load_region(&paths, 1).await.unwrap(),
                Some(region(1, 5))
            );
            assert_eq!(
                storage.load_region(&paths, 3).await.unwrap(),
                Some(region(3, 6))
            );
        }
    }
}
#[tokio::test]
async fn state_task_37_client_apply_raw_stats_and_unchanged_meta() {
    let backend = Arc::new(FaultStore::default());
    let storage = Storage::new(backend.clone());
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = history();
    let mut r = SyncApplyResponse {
        start_index: I(10),
        regions: vec![region(1, 5)],
        stats: vec![SyncRegionStats {
            written_bytes: u64::MAX,
            written_keys: 2,
            read_bytes: 3,
            read_keys: 4,
        }],
        buckets: vec![None],
        ..Default::default()
    };
    client
        .apply(r.clone(), &mut cluster, &storage, &paths, &mut h)
        .await
        .unwrap();
    let before = cluster.regions.get_region(1).unwrap();
    assert_eq!(
        (
            before.stats().written_bytes,
            before.stats().written_keys,
            before.stats().read_bytes,
            before.stats().read_keys
        ),
        (u64::MAX, 2, 3, 4)
    );
    assert!(before.buckets().is_none());
    backend.fault.store(1, std::sync::atomic::Ordering::SeqCst);
    r.start_index = I(11);
    r.stats[0].read_keys = 55;
    let out = client
        .apply(r, &mut cluster, &storage, &paths, &mut h)
        .await
        .unwrap();
    assert_eq!(out.save_errors, 0); // guide does not save unchanged metadata
    assert_eq!(h.next_index(), I(12));
    assert_eq!(cluster.regions.get_region(1).unwrap().stats().read_keys, 55);
    assert_eq!(before.stats().read_keys, 4); // retained snapshots stay immutable
}
#[tokio::test]
async fn state_task_37_client_apply_overlong_arrays_discarded() {
    let storage = Storage::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = history();
    let mut r = response();
    r.stats.push(SyncRegionStats::default());
    r.buckets.push(None);
    client
        .apply(r, &mut cluster, &storage, &paths, &mut h)
        .await
        .unwrap();
    assert_eq!(h.next_index(), I(12));
    for record in h.records_from(I(10)) {
        assert_eq!(record.stats().written_bytes, 0);
        assert!(record.buckets().is_none());
    }
}

#[tokio::test]
async fn state_task_37_client_apply_persistent_history_cadence() {
    use super::{HISTORY_KEY, PersistentHistory};
    use astersql_pd_storage_kv::Base;
    let backend = Arc::new(MemoryStore::default());
    let storage = Storage::new(backend.clone());
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = PersistentHistory::load(history(), backend.as_ref())
        .await
        .unwrap();
    for index in 10..110 {
        let out = client
            .apply(
                SyncApplyResponse {
                    start_index: I(index),
                    regions: vec![region(1, 5)],
                    ..Default::default()
                },
                &mut cluster,
                &storage,
                &paths,
                &mut h,
            )
            .await
            .unwrap();
        assert_eq!(out.checkpoint_errors, 0);
        if index < 109 {
            assert!(
                Base::new(backend.as_ref())
                    .load(HISTORY_KEY)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    }
    assert_eq!(h.history().next_index(), I(110));
    assert_eq!(
        Base::new(backend.as_ref()).load(HISTORY_KEY).await.unwrap(),
        b"110"
    );
}

#[tokio::test]
async fn state_task_37_client_apply_checkpoint_failure_keeps_assigned_record() {
    use super::{HISTORY_KEY, PersistentHistory};
    use astersql_pd_storage_kv::Base;
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let storage = Storage::new(backend.clone());
    let paths = ClusterPaths::new(37);
    let mut cluster = BasicCluster::new();
    let mut client = ClientApply::default();
    let mut h = PersistentHistory::load(history(), backend.as_ref())
        .await
        .unwrap();
    for index in 10..111 {
        if index == 109 {
            backend.fault.store(1, Ordering::SeqCst);
        }
        let out = client
            .apply(
                SyncApplyResponse {
                    start_index: I(index),
                    regions: vec![region(1, 5)],
                    ..Default::default()
                },
                &mut cluster,
                &storage,
                &paths,
                &mut h,
            )
            .await
            .unwrap();
        assert_eq!(out.save_errors, 0);
        assert_eq!(out.checkpoint_errors, usize::from(index == 109));
    }
    assert_eq!(h.history().next_index(), I(111));
    assert_eq!(h.history().get(I(109)).unwrap().meta().id, 1);
    assert!(
        Base::new(backend.as_ref())
            .load(HISTORY_KEY)
            .await
            .unwrap()
            .is_empty()
    );
    backend.fault.store(0, Ordering::SeqCst);
    h.persist().await.unwrap();
    assert_eq!(
        Base::new(backend.as_ref()).load(HISTORY_KEY).await.unwrap(),
        b"111"
    );
}
