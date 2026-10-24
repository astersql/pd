// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../storage/kv/etcd_fixture_test.rs"]
pub(crate) mod fixture;
fn config() -> ProviderSnapshot<String> {
    ProviderSnapshot::default()
}
fn rule(id: &str) -> Rule {
    Rule {
        group_id: "g".into(),
        id: id.into(),
        role: PeerRoleType::Voter,
        count: 1,
        ..Default::default()
    }
}
fn manager(store: Arc<dyn MetaStore>) -> RuleManager {
    RuleManager::new(Arc::new(StorageEndpoint::new(store)), ClusterPaths::new(24))
}
async fn init(m: &mut RuleManager) {
    m.initialize(3, &["zone".into()], "zone", false, &config(), &[])
        .await
        .unwrap();
}

#[tokio::test]
async fn placement_restore_defaults_skip_witness() {
    let store = Arc::new(MemoryStore::default());
    let mut m = manager(store.clone());
    m.initialize(3, &[], "", true, &config(), &[])
        .await
        .unwrap();
    assert!(m.is_initialized());
    assert!(m.config().rules().next().is_none());
    init(&mut m).await;
    assert!(m.config().rules().next().is_none());
    let mut m = manager(store.clone());
    init(&mut m).await;
    let r = m.get_rule("pd", "default").unwrap();
    assert_eq!(r.count, 3);
    assert_eq!(r.location_labels, ["zone"]);
    assert_eq!(r.create_timestamp, 0);
    let mut c = config();
    c.schedule.enable_witness = true;
    let store = Arc::new(MemoryStore::default());
    let mut m = manager(store);
    m.initialize(5, &[], "", false, &c, &[]).await.unwrap();
    assert_eq!(m.get_rule("pd", "default").unwrap().count, 3);
    assert_eq!(m.get_rule("pd", "witness").unwrap().count, 2);
}
async fn restore_contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(24);
    let base = Base::new(store.as_ref());
    let mut malformed = paths.rules_path_prefix().into_bytes();
    malformed.push(0xff);
    base.save(&malformed, b"bad json").await.unwrap();
    base.save(
        paths.rule_key_path("old-key").as_bytes(),
        rule("r").to_string().as_bytes(),
    )
    .await
    .unwrap();
    base.save(paths.rule_key_path("bad-json").as_bytes(), b"{")
        .await
        .unwrap();
    base.save(paths.rule_key_path("bad-rule").as_bytes(), b"{}")
        .await
        .unwrap();
    base.save(paths.rule_group_id_path("bad").as_bytes(), b"{")
        .await
        .unwrap();
    base.save(
        paths.rule_group_id_path("g").as_bytes(),
        br#"{"id":"g","index":7}"#,
    )
    .await
    .unwrap();
    let mut m = manager(store.clone());
    init(&mut m).await;
    assert_eq!(m.config().rules().count(), 1);
    assert_eq!(m.config().get_group("g").index, 7);
    assert!(
        base.load(&malformed).await.unwrap().is_empty(),
        "corrupt non-UTF8 suffix must be deleted byte-for-byte"
    );
    assert!(
        base.load(paths.rule_key_path("old-key").as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !base
            .load(paths.rule_key_path(&rule("r").store_key()).as_bytes())
            .await
            .unwrap()
            .is_empty()
    );
    m.set_rule(rule("other"), 123, &config(), &[])
        .await
        .unwrap();
    assert_eq!(m.get_rule("g", "other").unwrap().create_timestamp, 123);
    let mut changed = m.get_rule("g", "other").unwrap();
    changed.count = 2;
    m.set_rule(changed, 456, &config(), &[]).await.unwrap();
    assert_eq!(m.get_rule("g", "other").unwrap().version, 1);
    let old = m.get_rule("g", "other").unwrap();
    m.set_rule(old.clone(), 789, &config(), &[]).await.unwrap();
    assert_eq!(m.get_rule("g", "other").unwrap(), old);
    m.delete_rule("g", "r", 0).await.unwrap();
    m.delete_rule("missing", "id", 0).await.unwrap();
    assert!(m.delete_rule("g", "other", 0).await.is_err());
    let mut restarted = manager(store.clone());
    init(&mut restarted).await;
    // Go saves incoming values before in-memory commit: update persisted version 0 / time 123.
    let r = restarted.get_rule("g", "other").unwrap();
    assert_eq!((r.count, r.version, r.create_timestamp), (2, 0, 123));
    assert!(restarted.get_rule("g", "r").is_none());
}
#[tokio::test]
async fn placement_restore_memory_history_and_restart() {
    restore_contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn placement_restore_etcd_history_and_restart() {
    let mut f = fixture::Fixture::start();
    restore_contract(Arc::new(f.store().await)).await;
    f.command("stop", "stopped");
    f.command("start", "restarted");
    let mut m = manager(Arc::new(f.store().await));
    init(&mut m).await;
    assert_eq!(m.get_rule("g", "other").unwrap().count, 2);
}
#[tokio::test]
async fn placement_restore_validation_rejects_without_publish() {
    let store = Arc::new(MemoryStore::default());
    let mut m = manager(store);
    init(&mut m).await;
    for mut r in [
        rule(""),
        Rule {
            count: 0,
            ..rule("bad")
        },
        Rule {
            role: PeerRoleType::Leader,
            count: 2,
            ..rule("bad")
        },
        Rule {
            is_witness: true,
            count: 2,
            ..rule("bad")
        },
        Rule {
            start_key_hex: "a".into(),
            ..rule("bad")
        },
    ] {
        assert!(m.set_rule(r.clone(), 1, &config(), &[]).await.is_err());
        r.id = "bad".into();
        assert!(m.get_rule("g", &r.id).is_none());
    }
    m.set_key_type("txn");
    assert!(
        m.set_rule(
            Rule {
                start_key_hex: "aa".into(),
                ..rule("bad")
            },
            0,
            &config(),
            &[]
        )
        .await
        .is_err()
    );
    assert_eq!(m.config().rules().count(), 1);
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
    let mut m = manager(gate.clone());
    init(&mut m).await;
    m.set_rule(rule("keep"), 10, &config(), &[]).await.unwrap();
    let old = m.get_rule("g", "keep").unwrap();
    for delete in [false, true] {
        gate.writes.store(1, Ordering::SeqCst);
        let result = if delete {
            m.delete_rule("g", "keep", 20).await
        } else {
            m.set_rule(
                Rule {
                    count: 2,
                    ..rule("keep")
                },
                20,
                &config(),
                &[],
            )
            .await
        };
        assert!(matches!(
            result,
            Err(RuleManagerError::Transaction(CallbackTxnError::Store(
                StoreError::Unavailable
            )))
        ));
        assert_eq!(m.get_rule("g", "keep").unwrap(), old);
        let mut restart = manager(store.clone());
        init(&mut restart).await;
        assert_eq!(restart.get_rule("g", "keep").unwrap().count, 1);
    }
    gate.reads.store(1, Ordering::SeqCst);
    let mut m = manager(gate.clone());
    assert!(
        m.initialize(3, &[], "", false, &config(), &[])
            .await
            .is_err()
    );
    assert!(!m.is_initialized());
    gate.reads.store(-1, Ordering::SeqCst);
    init(&mut m).await;
    assert!(m.get_rule("g", "keep").is_some());
}
#[tokio::test]
async fn placement_restore_memory_io_failures() {
    failures(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn placement_restore_etcd_io_failures() {
    let f = fixture::Fixture::start();
    failures(Arc::new(f.store().await)).await;
}

#[tokio::test]
async fn placement_restore_go_runtime_fixture() {
    use serde_json::Value;
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task24/go.json"
    ))
    .unwrap();
    let store = Arc::new(MemoryStore::default());
    let mut m = manager(store.clone());
    m.initialize(3, &[], "", false, &config(), &[])
        .await
        .unwrap();
    let steps = oracle["steps"].as_array().unwrap();
    let now = steps[1]["rules"][0]["create_timestamp"].as_u64().unwrap();
    let r = rule("r");
    m.set_rule(r, now, &config(), &[]).await.unwrap();
    assert_eq!(
        serde_json::to_value(m.get_rule("g", "r").unwrap()).unwrap(),
        steps[1]["rules"][0]
    );
    m.set_rule(m.get_rule("g", "r").unwrap(), now + 1, &config(), &[])
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(m.get_rule("g", "r").unwrap()).unwrap(),
        steps[2]["rules"][0]
    );
    let mut r = m.get_rule("g", "r").unwrap();
    r.count = 2;
    m.set_rule(r, now + 2, &config(), &[]).await.unwrap();
    assert_eq!(
        serde_json::to_value(m.get_rule("g", "r").unwrap()).unwrap(),
        steps[3]["rules"][0]
    );
    assert_eq!(
        m.rule_list()
            .unwrap()
            .get_rules_by_key(&[])
            .iter()
            .find(|r| r.id == "r")
            .unwrap()
            .version,
        1,
        "published list must expose committed version"
    );
    let p = ClusterPaths::new(24);
    let base = Base::new(store.as_ref());
    base.save(
        p.rule_key_path("legacy-key").as_bytes(),
        rule("legacy").to_string().as_bytes(),
    )
    .await
    .unwrap();
    base.save(p.rule_key_path("bad-json").as_bytes(), b"{")
        .await
        .unwrap();
    base.save(p.rule_key_path("bad-rule").as_bytes(), b"{}")
        .await
        .unwrap();
    let mut bad = p.rules_path_prefix().into_bytes();
    bad.push(255);
    base.save(&bad, b"bad json").await.unwrap();
    let mut m = manager(store);
    m.initialize(3, &[], "", false, &config(), &[])
        .await
        .unwrap();
    let mut actual: Vec<Value> = m
        .config()
        .rules()
        .map(|r| serde_json::to_value(r.as_ref()).unwrap())
        .collect();
    let mut expected = oracle["restored_rules"].as_array().unwrap().clone();
    let sort = |v: &Value| format!("{}{}", v["group_id"], v["id"]);
    actual.sort_by_key(sort);
    expected.sort_by_key(sort);
    assert_eq!(actual, expected);
}
