// Copyright 2026 AsterSQL.
use crate::placement_restore_test::fixture;
use crate::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;
fn rule(id: &str) -> Rule {
    Rule {
        group_id: "g".into(),
        id: id.into(),
        role: PeerRoleType::Voter,
        count: 1,
        ..Default::default()
    }
}
fn conf() -> ProviderSnapshot<String> {
    ProviderSnapshot::default()
}
fn manager(s: Arc<dyn MetaStore>) -> RuleManager {
    RuleManager::new(Arc::new(StorageEndpoint::new(s)), ClusterPaths::new(25))
}
async fn init(m: &mut RuleManager) {
    m.initialize(3, &[], "", false, &conf(), &[]).await.unwrap();
}
fn op(id: &str, action: &str, prefix: bool) -> RuleOp {
    RuleOp {
        rule: rule(id),
        action: action.into(),
        delete_by_id_prefix: prefix,
    }
}
// Errors are injected before backend dispatch; no rule, index or transaction result is mocked.
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
    let gate = Gate::new(store.clone());
    let mut m = manager(gate.clone());
    init(&mut m).await;
    // Empty and identical patches perform no transaction.
    gate.writes.store(1, Ordering::SeqCst);
    m.try_commit_patch(m.begin_patch(), 1).await.unwrap();
    m.batch(vec![op("x", "unknown", false)], 1, &conf(), &[])
        .await
        .unwrap();
    assert_eq!(gate.writes.load(Ordering::SeqCst), 1);
    gate.writes.store(-1, Ordering::SeqCst);
    m.set_rules(vec![rule("pre1"), rule("pre2")], 10, &conf(), &[])
        .await
        .unwrap();
    m.batch(
        vec![op("pre3", "add", false), op("pre", "del", true)],
        11,
        &conf(),
        &[],
    )
    .await
    .unwrap();
    assert!(m.get_rule("g", "pre1").is_none());
    assert!(m.get_rule("g", "pre3").is_some());
    let before = m.get_rule("g", "pre3").unwrap();
    assert!(
        m.set_rules(
            vec![
                rule("ok"),
                Rule {
                    count: 0,
                    ..rule("bad")
                }
            ],
            12,
            &conf(),
            &[]
        )
        .await
        .is_err()
    );
    assert!(m.get_rule("g", "ok").is_none());
    m.set_rule_group(
        RuleGroup {
            id: "g".into(),
            index: 8,
            override_: false,
        },
        12,
    )
    .await
    .unwrap();
    m.delete_rule_group("g", 12).await.unwrap();
    assert_eq!(m.config().get_group("g").index, 0);
    assert_eq!(m.get_rule("g", "pre3").unwrap(), before);
    let bundle = GroupBundle {
        id: "g".into(),
        index: 9,
        rules: Some(vec![Some(Rule {
            group_id: String::new(),
            ..rule("bundle")
        })]),
        ..Default::default()
    };
    m.set_group_bundle(bundle.clone(), 13, &conf(), &[])
        .await
        .unwrap();
    assert!(m.get_rule("g", "pre3").is_none());
    assert_eq!(m.config().get_group("g").index, 9);
    let mut bad = bundle.clone();
    bad.rules.as_mut().unwrap()[0].as_mut().unwrap().group_id = "other".into();
    assert!(m.set_group_bundle(bad, 14, &conf(), &[]).await.is_err());
    gate.writes.store(1, Ordering::SeqCst);
    assert!(m.delete_group_bundle("g", false, 15).await.is_err());
    assert!(m.get_rule("g", "bundle").is_some());
    gate.writes.store(-1, Ordering::SeqCst);
    assert!(m.delete_group_bundle("[", true, 15).await.is_err());
    m.delete_group_bundle("^g$", true, 15).await.unwrap();
    assert!(m.get_rule("g", "bundle").is_none());
    // 121 saves: txn 1 succeeds, txn 2 fails. Storage retains 120, memory retains none.
    gate.writes.store(2, Ordering::SeqCst);
    let rules = (0..121).map(|n| rule(&format!("many{n:03}"))).collect();
    assert!(m.set_rules(rules, 20, &conf(), &[]).await.is_err());
    assert_eq!(m.config().rules().count(), 1);
    let mut restart = manager(store.clone());
    init(&mut restart).await;
    assert_eq!(
        restart
            .config()
            .rules()
            .filter(|r| r.group_id == "g")
            .count(),
        120
    );
    gate.writes.store(2, Ordering::SeqCst);
    let mut patch = m.begin_patch();
    for n in 0..120 {
        patch.set_rule(rule(&format!("boundary{n:03}")));
    }
    patch.set_group(RuleGroup {
        id: "g".into(),
        index: 77,
        override_: false,
    });
    assert!(m.try_commit_patch(patch, 20).await.is_err());
    assert_eq!(m.config().get_group("g").index, 0);
    assert!(m.get_rule("g", "boundary000").is_none());
    let mut restart = manager(store.clone());
    init(&mut restart).await;
    assert!(restart.get_rule("g", "boundary000").is_some());
    assert_eq!(restart.config().get_group("g").index, 0);
    gate.writes.store(-1, Ordering::SeqCst);
    m.set_all_group_bundles(vec![bundle], true, 21, &conf(), &[])
        .await
        .unwrap();
    assert!(m.get_rule("pd", "default").is_none());
    assert!(m.delete_group_bundle("g", false, 22).await.is_err()); // cannot leave uncovered keyspace
}
#[tokio::test]
async fn placement_batch_memory_contract() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn placement_batch_etcd_contract() {
    let f = fixture::Fixture::start();
    contract(Arc::new(f.store().await)).await;
}
#[tokio::test]
async fn placement_batch_go_regexp_ascii_classes() {
    let mut m = manager(Arc::new(MemoryStore::default()));
    init(&mut m).await;
    m.set_rules(
        vec![
            Rule {
                group_id: "١".into(),
                ..rule("keep")
            },
            Rule {
                group_id: "1".into(),
                ..rule("delete")
            },
        ],
        1,
        &conf(),
        &[],
    )
    .await
    .unwrap();
    m.delete_group_bundle(r"^\d$", true, 2).await.unwrap();
    assert!(
        m.get_rule("١", "keep").is_some(),
        "Go regexp \\d matches ASCII digits only"
    );
    assert!(m.get_rule("1", "delete").is_none());
}

#[tokio::test]
async fn placement_batch_go_runtime_fixture() {
    use serde_json::Value;
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/placement/task25/go.json"
    ))
    .unwrap();
    let steps = oracle["steps"].as_array().unwrap();
    let mut m = manager(Arc::new(MemoryStore::default()));
    init(&mut m).await;
    fn now(step: &Value, id: &str) -> u64 {
        step["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()["create_timestamp"]
            .as_u64()
            .unwrap()
    }
    fn check(m: &RuleManager, step: &Value) {
        let mut rules: Vec<Value> = m
            .config()
            .rules()
            .map(|r| serde_json::to_value(r.as_ref()).unwrap())
            .collect();
        let mut expected = step["rules"].as_array().unwrap().clone();
        let key = |r: &Value| format!("{}:{}", r["group_id"], r["id"]);
        rules.sort_by_key(key);
        expected.sort_by_key(key);
        assert_eq!(rules, expected, "{}", step["name"]);
        let mut groups: Vec<Value> = m
            .config()
            .groups()
            .values()
            .map(|g| serde_json::to_value(g).unwrap())
            .collect();
        let mut expected = step["groups"].as_array().unwrap().clone();
        let key = |g: &Value| g["id"].as_str().unwrap().to_owned();
        groups.sort_by_key(key);
        expected.sort_by_key(key);
        assert_eq!(groups, expected, "{}", step["name"]);
    }
    m.set_rules(
        vec![rule("pre1"), rule("pre2")],
        now(&steps[0], "pre1"),
        &conf(),
        &[],
    )
    .await
    .unwrap();
    check(&m, &steps[0]);
    m.batch(
        vec![op("pre3", "add", false), op("pre", "del", true)],
        now(&steps[1], "pre3"),
        &conf(),
        &[],
    )
    .await
    .unwrap();
    check(&m, &steps[1]);
    m.set_rule_group(
        RuleGroup {
            id: "g".into(),
            index: 8,
            override_: false,
        },
        0,
    )
    .await
    .unwrap();
    check(&m, &steps[2]);
    m.delete_rule_group("g", 0).await.unwrap();
    check(&m, &steps[3]);
    m.set_group_bundle(
        GroupBundle {
            id: "g".into(),
            index: 9,
            rules: Some(vec![Some(Rule {
                group_id: String::new(),
                count: 2,
                ..rule("bundle")
            })]),
            ..Default::default()
        },
        now(&steps[4], "bundle"),
        &conf(),
        &[],
    )
    .await
    .unwrap();
    check(&m, &steps[4]);
    m.set_all_group_bundles(
        vec![GroupBundle {
            id: "h".into(),
            rules: Some(vec![Some(Rule {
                group_id: String::new(),
                ..rule("h1")
            })]),
            ..Default::default()
        }],
        false,
        now(&steps[5], "h1"),
        &conf(),
        &[],
    )
    .await
    .unwrap();
    check(&m, &steps[5]);
    m.delete_group_bundle("g", true, 0).await.unwrap();
    check(&m, &steps[6]);
    let err = m
        .set_rules(
            vec![Rule {
                count: 0,
                ..rule("bad")
            }],
            0,
            &conf(),
            &[],
        )
        .await
        .unwrap_err();
    assert_eq!(
        format!("{}\n", serde_json::to_string(&err.to_string()).unwrap()),
        oracle["invalid_error"]["body"].as_str().unwrap()
    );
    check(&m, &steps[7]);
    m.set_rules(
        vec![
            Rule {
                group_id: "١".into(),
                ..rule("keep")
            },
            Rule {
                group_id: "1".into(),
                ..rule("delete")
            },
        ],
        now(&steps[8], "keep"),
        &conf(),
        &[],
    )
    .await
    .unwrap();
    m.delete_group_bundle(r"^\d$", true, 0).await.unwrap();
    check(&m, &steps[8]);
}
