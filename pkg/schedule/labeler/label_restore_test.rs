// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use serde_json::{Value, json};
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../storage/kv/etcd_fixture_test.rs"]
mod fixture;

fn rule(id: &str, start: &str, end: &str) -> LabelRule {
    LabelRule::from_json(&serde_json::to_vec(&json!({"id":id,"labels":[{"key":"k","value":id}],"rule_type":"key-range","data":[{"start_key":start,"end_key":end}]})).unwrap(),0).unwrap()
}
async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(17);
    let base = Base::new(store.as_ref());
    let mut owner = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
        .await
        .unwrap();
    assert_eq!(owner.index().rules().count(), 0);
    owner
        .set_label_rule(rule("restore", "10", "20"), 0)
        .await
        .unwrap();
    assert_eq!(
        owner.index().get_split_keys(&[], &[]),
        vec![&[0x10][..], &[0x20][..]]
    );
    owner
        .set_label_rule(rule("restore", "30", "40"), 0)
        .await
        .unwrap();
    assert_eq!(
        owner.index().get_split_keys(&[], &[]),
        vec![&[0x30][..], &[0x40][..]]
    );
    assert!(
        matches!(owner.delete_label_rule("absent").await,Err(LabelerError::NotFound(e)) if e.code()=="PD:region:ErrRegionRuleNotFound")
    );
    owner.delete_label_rule("restore").await.unwrap();
    assert_eq!(owner.index().get_split_keys(&[], &[]).len(), 0);
    owner.delete_label_rule_if_present("restore").await.unwrap();
    owner
        .set_label_rule(rule("restore", "10", "20"), 0)
        .await
        .unwrap();
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task17/go.json"
    ))
    .unwrap();
    for (key, value) in oracle["seeded"].as_object().unwrap() {
        base.save(
            paths.region_label_key_path(key).as_bytes(),
            value.as_str().unwrap().as_bytes(),
        )
        .await
        .unwrap();
    }
    let restored = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
        .await
        .unwrap();
    let actual: Vec<Value> = restored
        .index()
        .rules()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    assert_eq!(actual, oracle["restored"].as_array().unwrap().clone());
    assert!(
        base.load(paths.region_label_key_path("corrupt").as_bytes())
            .await
            .unwrap()
            .is_empty(),
        "corrupt persisted value must be deleted"
    );
    assert!(
        base.load(paths.region_label_key_path("invalid").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !base
            .load(paths.region_label_key_path("alias").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        restored.index().get_split_keys(&[], &[]),
        vec![&[0x10][..], &[0x20][..], &[0x50][..], &[0x60][..]]
    );
}
#[tokio::test]
async fn label_restore_memory_contract() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn label_restore_etcd_contract_and_same_wal() {
    let mut fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    contract(store.clone()).await;
    drop(store);
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let restored = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(fixture.store().await)),
        ClusterPaths::new(17),
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        restored
            .index()
            .rules()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["embedded", "restore"]
    );
    assert_eq!(restored.index().get_split_keys(&[], &[]).len(), 4);
}

// Errors are injected before backend dispatch; no rule, index or transaction result is mocked.
use astersql_pd_storage_kv::*;
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

async fn failures(store: Arc<dyn MetaStore>) {
    let gate = Gate::new(store.clone());
    let paths = ClusterPaths::new(170);
    let base = Base::new(store.as_ref());
    let mut owner = RegionLabeler::load(StorageEndpoint::new(gate.clone()), paths, 0)
        .await
        .unwrap();
    owner
        .set_label_rule(rule("keep", "10", "20"), 0)
        .await
        .unwrap();
    for delete in [false, true] {
        gate.writes.store(1, Ordering::SeqCst);
        let result = if delete {
            owner.delete_label_rule("keep").await
        } else {
            owner.set_label_rule(rule("keep", "30", "40"), 0).await
        };
        assert!(matches!(
            result,
            Err(LabelerError::Transaction(CallbackTxnError::Store(
                StoreError::Unavailable
            )))
        ));
        assert_eq!(
            owner.index().get_split_keys(&[], &[]),
            vec![&[0x10][..], &[0x20][..]]
        );
        let saved = base
            .load(paths.region_label_key_path("keep").as_bytes())
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&saved).unwrap()["data"][0]["start_key"],
            "10"
        );
    }
    let before = store
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap()
        .revision;
    let mut bad = rule("bad", "10", "20");
    bad.labels.clear();
    assert!(matches!(
        owner.set_label_rule(bad, 0).await,
        Err(LabelerError::Rule(_))
    ));
    assert_eq!(
        store
            .read(Read::prefix(b"", Consistency::Linearizable))
            .await
            .unwrap()
            .revision,
        before
    );
    // Missing regular deletion still removes an externally persisted key before returning NotFound.
    base.save(paths.region_label_key_path("external").as_bytes(), b"{}")
        .await
        .unwrap();
    assert!(matches!(
        owner.delete_label_rule("external").await,
        Err(LabelerError::NotFound(_))
    ));
    assert!(
        base.load(paths.region_label_key_path("external").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    base.save(paths.region_label_key_path("a-bad").as_bytes(), b"{")
        .await
        .unwrap();
    base.save(paths.region_label_key_path("b-bad").as_bytes(), b"{}")
        .await
        .unwrap();
    gate.writes.store(2, Ordering::SeqCst);
    assert!(matches!(
        RegionLabeler::load(StorageEndpoint::new(gate.clone()), paths, 0).await,
        Err(LabelerError::Transaction(_))
    ));
    assert!(
        base.load(paths.region_label_key_path("a-bad").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !base
            .load(paths.region_label_key_path("b-bad").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    // Fail the second scan page: construction must not start deleting earlier bad records.
    for n in 0..101 {
        base.save(
            paths.region_label_key_path(&format!("p{n:03}")).as_bytes(),
            b"{",
        )
        .await
        .unwrap();
    }
    gate.reads.store(2, Ordering::SeqCst);
    assert!(matches!(
        RegionLabeler::load(StorageEndpoint::new(gate.clone()), paths, 0).await,
        Err(LabelerError::Load(_))
    ));
    assert!(
        !base
            .load(paths.region_label_key_path("p000").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    let restored = RegionLabeler::load(StorageEndpoint::new(gate), paths, 0)
        .await
        .unwrap();
    assert_eq!(restored.index().rules().count(), 1);
    assert_eq!(restored.index().rule("keep").unwrap().id, "keep");
}
#[tokio::test]
async fn label_restore_memory_io_failures() {
    failures(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn label_restore_etcd_io_failures() {
    let f = fixture::Fixture::start();
    failures(Arc::new(f.store().await)).await;
}

async fn restore_edges(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(171);
    let base = Base::new(store.as_ref());
    for n in 0..105 {
        let r = rule(&format!("page{n:03}"), "10", "20");
        base.save(
            paths.region_label_key_path(&r.id).as_bytes(),
            &serde_json::to_vec(&r).unwrap(),
        )
        .await
        .unwrap();
    }
    // Byte-ordered physical scan: the later duplicate embedded ID wins.
    for (suffix, start, end) in [("a-alias", "30", "40"), ("z-alias", "50", "60")] {
        base.save(
            paths.region_label_key_path(suffix).as_bytes(),
            &serde_json::to_vec(&rule("duplicate", start, end)).unwrap(),
        )
        .await
        .unwrap();
    }
    let expired =
        json!({"key":"old","value":"v","ttl":"1h","start_at":"Mon Jan  2 15:04:05 UTC 2006"});
    let mut partial = serde_json::to_value(rule("partial", "70", "80")).unwrap();
    partial["labels"]
        .as_array_mut()
        .unwrap()
        .push(expired.clone());
    base.save(
        paths.region_label_key_path("partial").as_bytes(),
        &serde_json::to_vec(&partial).unwrap(),
    )
    .await
    .unwrap();
    let mut dead = partial.clone();
    dead["id"] = json!("dead");
    dead["labels"] = json!([expired]);
    base.save(
        paths.region_label_key_path("dead").as_bytes(),
        &serde_json::to_vec(&dead).unwrap(),
    )
    .await
    .unwrap();
    let mut invalid_key = paths.region_label_path_prefix().into_bytes();
    invalid_key.push(255);
    base.save(&invalid_key, b"{").await.unwrap();
    // Adjacent prefix/other cluster are untouched even when their values are corrupt.
    let other = ClusterPaths::new(172).region_label_key_path("corrupt");
    base.save(other.as_bytes(), b"{").await.unwrap();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let start = astersql_pd_codec::encode_keyspace_boundary(b'x', 42);
    let end = astersql_pd_codec::encode_keyspace_boundary(b'x', 43);
    let mut canonical =
        serde_json::to_value(rule("keyspaces/42", &hex(&start), &hex(&end))).unwrap();
    canonical["labels"] = json!([{"key":"id","value":"42"}]);
    base.save(
        paths.region_label_key_path("keyspaces/42").as_bytes(),
        &serde_json::to_vec(&canonical).unwrap(),
    )
    .await
    .unwrap();
    let now = 2_000_000_000_000_000_000;
    let mut owner = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths, now)
        .await
        .unwrap();
    assert_eq!(owner.index().rules().count(), 108); // 105 pages, duplicate, partial, canonical.
    assert_eq!(
        owner
            .index()
            .rule("duplicate")
            .unwrap()
            .key_ranges()
            .unwrap()[0]
            .start_key,
        vec![0x50]
    );
    assert_eq!(owner.index().rule("partial").unwrap().labels.len(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(
            &base
                .load(paths.region_label_key_path("partial").as_bytes())
                .await
                .unwrap()
        )
        .unwrap(),
        partial,
        "load filters in memory without rewriting valid stored JSON"
    );
    assert!(
        base.load(paths.region_label_key_path("dead").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(base.load(&invalid_key).await.unwrap().is_empty());
    assert_eq!(base.load(other.as_bytes()).await.unwrap(), b"{");
    assert!(
        owner
            .index()
            .get_range_rules(&start, &end)
            .unwrap()
            .iter()
            .any(|r| r.id == "keyspaces/42")
    );
    owner.delete_label_rule("keyspaces/42").await.unwrap();
    assert!(
        owner
            .index()
            .get_range_rules(&start, &end)
            .unwrap()
            .iter()
            .all(|r| r.id != "keyspaces/42")
    );
    owner
        .set_validated_label_rule(rule("watcher", "90", "a0"))
        .await
        .unwrap();
    assert!(
        owner
            .index()
            .get_split_keys(&[], &[])
            .contains(&&[0x90][..])
    );
    owner.delete_label_rule_if_present("watcher").await.unwrap();
    assert!(
        !owner
            .index()
            .get_split_keys(&[], &[])
            .contains(&&[0x90][..])
    );
}
#[tokio::test]
async fn label_restore_memory_pages_expiry_and_keyspaces() {
    restore_edges(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn label_restore_etcd_pages_expiry_and_keyspaces() {
    let f = fixture::Fixture::start();
    restore_edges(Arc::new(f.store().await)).await;
}
