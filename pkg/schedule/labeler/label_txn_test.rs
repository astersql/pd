// Copyright 2026 AsterSQL.
use super::label_restore_test::fixture;
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_domain::{MAX_BATCH_OPS, StorageEndpoint};
use serde_json::{Value, json};
use std::sync::Arc;
fn rule(id: &str, start: &str, end: &str) -> LabelRule {
    LabelRule::from_json(&serde_json::to_vec(&json!({"id":id,"labels":[{"key":"k","value":id}],"rule_type":"key-range","data":[{"start_key":start,"end_key":end}]})).unwrap(),0).unwrap()
}
use astersql_pd_storage_domain::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicIsize, Ordering};
struct Gate {
    inner: Arc<dyn MetaStore>,
    reads: AtomicIsize,
    writes: AtomicIsize,
}
impl Gate {
    fn new(inner: Arc<dyn MetaStore>) -> Arc<Self> {
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

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(18);
    let gate = Gate::new(store.clone());
    let mut owner = RegionLabeler::load(StorageEndpoint::new(gate.clone()), paths, 0)
        .await
        .unwrap();
    let base = Base::new(store.as_ref());
    owner
        .set_label_rule(rule("existing", "10", "20"), 0)
        .await
        .unwrap();
    owner
        .patch(
            LabelRulePatch {
                set_rules: vec![
                    rule("r", "10", "20"),
                    rule("r", "30", "40"),
                    rule("s", "50", "60"),
                ],
                delete_rules: vec!["existing".into(), "r".into(), "absent".into()],
            },
            0,
        )
        .await
        .unwrap();
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task18/go.json"
    ))
    .unwrap();
    let snapshot: Vec<_> = owner
        .index()
        .rules()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    assert_eq!(snapshot, oracle["rules"].as_array().unwrap().clone());
    let mut bad = rule("bad", "10", "20");
    bad.id.clear();
    assert!(
        owner
            .patch(
                LabelRulePatch {
                    set_rules: vec![rule("x", "10", "20"), bad.clone()],
                    delete_rules: vec!["r".into()]
                },
                0
            )
            .await
            .is_err()
    );
    assert_eq!(
        owner
            .index()
            .rules()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect::<Vec<_>>(),
        snapshot
    );
    assert!(
        base.load(paths.region_label_key_path("x").as_bytes())
            .await
            .unwrap()
            .is_empty(),
        "validation must precede every write"
    );
    let mut plan = LabelPlan::new();
    plan.set_label_rule(rule("x", "10", "20"), 0).unwrap();
    assert!(plan.set_label_rule(bad, 0).is_err());
    let first = plan.error().unwrap().to_string();
    assert_eq!(plan.delete_label_rule("r").unwrap_err().to_string(), first);
    assert_eq!(
        plan.set_label_rule(rule("y", "10", "20"), 0)
            .unwrap_err()
            .to_string(),
        first
    );
    assert!(plan.commit_ops().is_empty());
    plan.apply(&mut owner);
    assert!(owner.index().rule("x").is_none());
    let mut duplicate = LabelPlan::new();
    duplicate.set_label_rule(rule("x", "10", "20"), 0).unwrap();
    duplicate.delete_label_rule("x").unwrap();
    duplicate.set_label_rule(rule("x", "70", "80"), 0).unwrap();
    // etcd rejects duplicate writes to the same key in one transaction.
    assert!(owner.commit_plan(&duplicate).await.is_err());
    assert!(owner.index().rule("x").is_none());
    let mut plan = LabelPlan::new();
    plan.set_label_rule(rule("x", "70", "80"), 0).unwrap();
    plan.delete_label_rule("r").unwrap();
    plan.delete_label_rule("missing").unwrap();
    assert_eq!(plan.commit_ops().len(), 3);
    // Composable transaction: another module's write commits alongside label operations.
    let endpoint = StorageEndpoint::new(store.clone());
    endpoint
        .run_in_txn(async |txn| {
            txn.save(b"/companion", b"ok").unwrap();
            for op in plan.commit_ops() {
                op.buffer(txn, &endpoint, &paths)?;
            }
            Ok::<_, astersql_pd_storage_domain::EndpointError>(())
        })
        .await
        .unwrap();
    assert!(owner.index().rule("x").is_none());
    assert_eq!(base.load(b"/companion").await.unwrap(), b"ok");
    plan.apply(&mut owner);
    assert!(owner.index().rule("r").is_none());
    assert_eq!(
        owner.index().get_split_keys(&[], &[]),
        vec![&[0x50][..], &[0x60][..], &[0x70][..], &[0x80][..]]
    );
    let mut failed = LabelPlan::new();
    failed.delete_label_rule("x").unwrap();
    gate.writes.store(1, Ordering::SeqCst);
    assert!(owner.commit_plan(&failed).await.is_err());
    assert!(owner.index().rule("x").is_some());
    assert!(
        !base
            .load(paths.region_label_key_path("x").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    owner.commit_plan(&failed).await.unwrap();
    assert!(owner.index().rule("x").is_none());
    // Ordered deletes make the partial-commit boundary deterministic (Go map set order is unspecified).
    let ids: Vec<_> = (0..=MAX_BATCH_OPS)
        .map(|i| format!("batch-{i:03}"))
        .collect();
    owner
        .patch(
            LabelRulePatch {
                set_rules: ids.iter().map(|id| rule(id, "10", "20")).collect(),
                delete_rules: vec![],
            },
            0,
        )
        .await
        .unwrap();
    gate.writes.store(2, Ordering::SeqCst);
    assert!(
        owner
            .patch(
                LabelRulePatch {
                    set_rules: vec![],
                    delete_rules: ids.clone()
                },
                0
            )
            .await
            .is_err()
    );
    for (i, id) in ids.iter().enumerate() {
        assert!(
            owner.index().rule(id).is_some(),
            "failed patch must not publish any changes"
        );
        assert_eq!(
            base.load(paths.region_label_key_path(id).as_bytes())
                .await
                .unwrap()
                .is_empty(),
            i < MAX_BATCH_OPS
        );
    }
    owner
        .patch(
            LabelRulePatch {
                set_rules: vec![],
                delete_rules: ids,
            },
            0,
        )
        .await
        .unwrap();
    let restored = RegionLabeler::load(StorageEndpoint::new(store), paths, 0)
        .await
        .unwrap();
    assert_eq!(
        restored
            .index()
            .rules()
            .map(|r| r.id.clone())
            .collect::<Vec<_>>(),
        vec!["s"]
    );
}
#[tokio::test]
async fn label_txn_memory_contract() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn label_txn_etcd_contract() {
    let f = fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}
