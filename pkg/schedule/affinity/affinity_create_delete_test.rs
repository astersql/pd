// Copyright 2026 AsterSQL.
use super::affinity_restore_test::fixture;
use crate::{
    manager::Manager,
    ranges::{GroupKeyRanges, get_label_rule_id},
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{MemoryStore, MetaStore};
use std::sync::Arc;
fn change(id: &str, ranges: bool) -> GroupKeyRanges {
    GroupKeyRanges {
        group_id: id.into(),
        key_ranges: if ranges {
            vec![KeyRange::new(vec![1], vec![2])]
        } else {
            vec![]
        },
    }
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let p = ClusterPaths::new(34);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store), p.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    assert!(
        m.create_affinity_groups(
            &ep,
            &p,
            &mut labels,
            &[change("dup", false), change("dup", false)],
            100
        )
        .await
        .is_err()
    );
    assert!(m.groups.is_empty());
    m.create_affinity_groups(
        &ep,
        &p,
        &mut labels,
        &[change("a", true), change("empty", false)],
        100,
    )
    .await
    .unwrap();
    assert_eq!(m.groups.len(), 2);
    assert_eq!(m.groups["a"].affinity_version, 2);
    assert_eq!(m.groups["empty"].affinity_version, 2);
    assert_eq!(m.groups["a"].info.group.create_timestamp, 100);
    assert_eq!(m.groups["a"].info.range_count, 1);
    assert!(labels.index().rule(&get_label_rule_id("a")).is_some());
    assert!(
        m.create_affinity_groups(&ep, &p, &mut labels, &[change("a", false)], 101)
            .await
            .unwrap_err()
            .to_string()
            .contains("ErrAffinityGroupExist")
    );
    m.create_affinity_groups_if_not_exists(
        &ep,
        &p,
        &mut labels,
        &[change("a", true), change("new", false)],
        200,
    )
    .await
    .unwrap();
    assert_eq!(m.groups["a"].info.group.create_timestamp, 100);
    assert_eq!(m.groups.len(), 3);
    assert!(
        m.delete_affinity_groups(&ep, &p, &mut labels, &[], true)
            .await
            .is_err()
    );
    assert!(
        m.delete_affinity_groups(&ep, &p, &mut labels, &["a".into()], false)
            .await
            .is_err()
    );
    assert!(
        m.delete_affinity_groups(
            &ep,
            &p,
            &mut labels,
            &["empty".into(), "missing".into()],
            false
        )
        .await
        .is_err()
    );
    assert!(m.groups.contains_key("empty"));
    m.delete_affinity_groups(
        &ep,
        &p,
        &mut labels,
        &["empty".into(), "empty".into()],
        false,
    )
    .await
    .unwrap();
    m.delete_affinity_groups(
        &ep,
        &p,
        &mut labels,
        &["missing".into(), "a".into(), "a".into()],
        true,
    )
    .await
    .unwrap();
    assert!(!m.groups.contains_key("a"));
    assert!(m.key_ranges.is_empty());
    assert!(labels.index().rule(&get_label_rule_id("a")).is_none());
    let reloaded = Manager::initialize(&ep, &p, &labels, 300).await.unwrap();
    assert_eq!(reloaded.groups.keys().collect::<Vec<_>>(), vec!["new"]);
    assert!(ep.load_affinity_group(&p, "a").await.unwrap().is_empty());
}
#[tokio::test]
async fn affinity_create_delete_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_create_delete_etcd() {
    let mut f = fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
    f.command("stop", "stopped");
    f.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(f.store().await));
    let paths = ClusterPaths::new(34);
    let labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(f.store().await)),
        paths.clone(),
        0,
    )
    .await
    .unwrap();
    let manager = Manager::initialize(&ep, &paths, &labels, 300)
        .await
        .unwrap();
    assert_eq!(manager.groups.keys().collect::<Vec<_>>(), vec!["new"]);
}

use astersql_pd_storage_kv::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicIsize, Ordering};
pub(super) struct Gate {
    inner: Arc<dyn MetaStore>,
    reads: AtomicIsize,
    pub(super) writes: AtomicIsize,
}
impl Gate {
    pub(super) fn new(inner: Arc<dyn MetaStore>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            reads: AtomicIsize::new(-1),
            writes: AtomicIsize::new(-1),
        })
    }
}
#[async_trait]
impl MetaStore for Gate {
    async fn read(&self, r: Read) -> Result<Snapshot> {
        if self.reads.fetch_sub(1, Ordering::SeqCst) == 1 {
            return Err(StoreError::Unavailable);
        }
        self.inner.read(r).await
    }
    async fn transact(&self, c: Vec<Compare>, w: Vec<Mutation>) -> Result<Commit> {
        if self.writes.fetch_sub(1, Ordering::SeqCst) == 1 {
            return Err(StoreError::Unavailable);
        }
        self.inner.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(c, t, e).await
    }
    async fn watch(&self, p: Vec<u8>, r: Revision) -> Result<Box<dyn MetaWatch>> {
        self.inner.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> Result<Lease> {
        self.inner.grant(t).await
    }
    async fn keep_alive(&self, l: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(l).await
    }
    async fn revoke(&self, l: LeaseId) -> Result<()> {
        self.inner.revoke(l).await
    }
    async fn ttl(&self, l: LeaseId) -> Result<i64> {
        self.inner.ttl(l).await
    }
    async fn compact(&self, r: Revision) -> Result<()> {
        self.inner.compact(r).await
    }
}

#[tokio::test]
async fn affinity_create_delete_validation_duplicates() {
    let store = Arc::new(MemoryStore::default());
    let ep = StorageEndpoint::new(store.clone());
    let p = ClusterPaths::new(340);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store), p.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    // Canonical MetaStore follows etcd: repeated writes to the same key in one
    // transaction fail, even though Go's legacy memory backend accepts them.
    assert!(
        m.create_affinity_groups(
            &ep,
            &p,
            &mut labels,
            &[change("dup", false), change("dup", false)],
            1
        )
        .await
        .is_err()
    );
    assert!(m.groups.is_empty());
    m.create_affinity_groups(&ep, &p, &mut labels, &[change("dup", false)], 1)
        .await
        .unwrap();
    assert!(
        m.create_affinity_groups_if_not_exists(
            &ep,
            &p,
            &mut labels,
            &[change("dup", false), change("bad/id", false)],
            2
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("ErrInvalidGroupID")
    );
    assert!(
        m.create_affinity_groups(
            &ep,
            &p,
            &mut labels,
            &[change("x", true), change("y", true)],
            2
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("overlap")
    );
    assert_eq!(m.groups.len(), 1);
    let mut bad = change("bad", true);
    bad.key_ranges[0] = KeyRange::new(vec![2], vec![1]);
    assert!(
        m.create_affinity_groups(&ep, &p, &mut labels, &[bad], 2)
            .await
            .is_err()
    );
    assert!(ep.load_affinity_group(&p, "bad").await.unwrap().is_empty());
    m.create_affinity_groups(&ep, &p, &mut labels, &[], 3)
        .await
        .unwrap();
}
async fn fault_contract(inner: Arc<dyn MetaStore>) {
    let gate = Gate::new(inner.clone());
    let ep = StorageEndpoint::new(gate.clone());
    let p = ClusterPaths::new(341);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(gate.clone()), p.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    gate.writes.store(1, Ordering::SeqCst);
    assert!(
        m.create_affinity_groups(&ep, &p, &mut labels, &[change("a", true)], 1)
            .await
            .is_err()
    );
    assert!(m.groups.is_empty());
    assert_eq!(labels.index().rules().count(), 0);
    assert!(ep.load_affinity_group(&p, "a").await.unwrap().is_empty());
    gate.writes.store(-1, Ordering::SeqCst);
    m.create_affinity_groups(&ep, &p, &mut labels, &[change("a", true)], 1)
        .await
        .unwrap();
    gate.writes.store(1, Ordering::SeqCst);
    assert!(
        m.delete_affinity_groups(&ep, &p, &mut labels, &["a".into()], true)
            .await
            .is_err()
    );
    assert!(m.groups.contains_key("a"));
    assert!(labels.index().rule(&get_label_rule_id("a")).is_some());
    assert!(!ep.load_affinity_group(&p, "a").await.unwrap().is_empty());
    // Exactly 120 groups in the first commit; failure on the second leaves storage partial,
    // while memory remains entirely unpublished.
    let changes = (0..121)
        .map(|i| change(&format!("g{i:03}"), false))
        .collect::<Vec<_>>();
    gate.writes.store(2, Ordering::SeqCst);
    assert!(
        m.create_affinity_groups(&ep, &p, &mut labels, &changes, 2)
            .await
            .is_err()
    );
    assert_eq!(m.groups.len(), 1);
    assert!(!ep.load_affinity_group(&p, "g119").await.unwrap().is_empty());
    assert!(ep.load_affinity_group(&p, "g120").await.unwrap().is_empty());
    let loaded = Manager::initialize(&ep, &p, &labels, 3).await.unwrap();
    assert_eq!(loaded.groups.len(), 121);
    // Delete labels are ordered first. 61 groups produce 122 ops: first commit
    // deletes all 61 labels and 59 groups, then the last two group removals fail.
    let mut loaded = loaded;
    let ids = (0..61).map(|i| format!("g{i:03}")).collect::<Vec<_>>();
    gate.writes.store(2, Ordering::SeqCst);
    assert!(
        loaded
            .delete_affinity_groups(&ep, &p, &mut labels, &ids, true)
            .await
            .is_err()
    );
    assert_eq!(loaded.groups.len(), 121);
    assert!(ep.load_affinity_group(&p, "g058").await.unwrap().is_empty());
    assert!(!ep.load_affinity_group(&p, "g059").await.unwrap().is_empty());
}
#[tokio::test]
async fn affinity_create_delete_fault_memory() {
    fault_contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_create_delete_fault_etcd() {
    let f = fixture::Fixture::start();
    fault_contract(Arc::new(f.store().await)).await;
}

async fn label_batch_fault(inner: Arc<dyn MetaStore>) {
    let gate = Gate::new(inner);
    let ep = StorageEndpoint::new(gate.clone());
    let p = ClusterPaths::new(342);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(gate.clone()), p.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::default();
    let changes = (0..61u8)
        .map(|i| GroupKeyRanges {
            group_id: format!("g{i:03}"),
            key_ranges: vec![KeyRange::new(vec![i * 2 + 1], vec![i * 2 + 2])],
        })
        .collect::<Vec<_>>();
    gate.writes.store(2, Ordering::SeqCst);
    assert!(
        m.create_affinity_groups(&ep, &p, &mut labels, &changes, 1)
            .await
            .is_err()
    );
    assert!(m.groups.is_empty());
    assert!(m.key_ranges.is_empty());
    assert_eq!(labels.index().rules().count(), 0);
    assert!(!ep.load_affinity_group(&p, "g058").await.unwrap().is_empty());
    assert!(ep.load_affinity_group(&p, "g059").await.unwrap().is_empty());
    let restored_labels = RegionLabeler::load(StorageEndpoint::new(gate), p.clone(), 0)
        .await
        .unwrap();
    assert_eq!(restored_labels.index().rules().count(), 61);
    let restored = Manager::initialize(&ep, &p, &restored_labels, 2)
        .await
        .unwrap();
    assert_eq!(restored.groups.len(), 59);
    assert_eq!(restored.key_ranges.len(), 59);
}
#[tokio::test]
async fn affinity_create_delete_label_batch_memory() {
    label_batch_fault(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_create_delete_label_batch_etcd() {
    let f = fixture::Fixture::start();
    label_batch_fault(Arc::new(f.store().await)).await;
}
