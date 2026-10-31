// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn wire(url: &str, path: &str, body: &str) -> (u16, String, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-X",
            "POST",
            "-H",
            "Content-Type: text/plain",
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}/pd/api/v1/admin/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into(), content.to_lowercase())
}
fn reset(url: &str, body: &str) -> (u16, String, String) {
    wire(url, "reset-ts", body)
}
async fn node(
    store: Arc<dyn astersql_pd_storage::MetaStore>,
    id: u64,
    member: u64,
) -> (astersql_pd_runtime::PdNode, String) {
    node_at(store, id, member, None).await
}
async fn node_at(
    store: Arc<dyn astersql_pd_storage::MetaStore>,
    id: u64,
    member: u64,
    url: Option<&str>,
) -> (astersql_pd_runtime::PdNode, String) {
    let listener = tokio::net::TcpListener::bind(
        url.map(|u| u.trim_start_matches("http://"))
            .unwrap_or("127.0.0.1:0"),
    )
    .await
    .unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: id,
                member_id: member,
                name: format!("marker-{member}"),
                client_url: url.clone(),
                peer_url: format!("http://peer{member}"),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    (node, url)
}

async fn timestamp(url: &str, id: u64) -> u64 {
    let mut c = PdClient::connect(url.to_owned()).await.unwrap();
    let t = crate::grpc_tso_scenarios_test::minimum(&mut c, id).await;
    (t.physical as u64) << 18 | t.logical as u64
}
async fn observe(url: &str, id: u64) -> Vec<(u16, String, String)> {
    let mut result = vec![];
    for (body, status) in [
        (r#"{}"#, 400),
        ("null", 400),
        (r#"{"tso":1}"#, 400),
        (r#"{"tso":""}"#, 400),
        (r#"{"tso":"+1"}"#, 400),
        (r#"{"tso":"18446744073709551616"}"#, 400),
        (r#"{"tso":"1","force-use-larger":1}"#, 400),
        (r#"{"tso":"1"}"#, 403),
        (r#"{"tso":"1","force-use-larger":true}"#, 200),
    ] {
        let r = reset(url, body);
        assert_eq!(r.0, status, "{r:?}");
        result.push(r);
    }
    let before = timestamp(url, id).await;
    let target = before + (10000 << 18) + 123;
    let r = reset(url, &format!(r#"{{"tso":"{target}"}}"#));
    assert_eq!(r.0, 200, "{r:?}");
    result.push(r);
    assert!(timestamp(url, id).await > target);
    let far = target + (86400000u64 << 18);
    let r = reset(url, &format!(r#"{{"tso":"{far}"}}"#));
    assert_eq!(r.0, 403, "{r:?}");
    result.push(r);
    let r = reset(
        url,
        &format!(r#"{{"tso":"{far}","force-use-larger":true}}"#),
    );
    assert_eq!(r.0, 200, "{r:?}");
    result.push(r);
    assert!(timestamp(url, id).await > far);
    result
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_076_behavior() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 776, 1).await;
    observe(&url, 776).await;
    let target = timestamp(&url, 776).await + (10000 << 18);
    let (mut follower, fu) = node(store.clone(), 776, 2).await;
    assert_eq!(reset(&fu, &format!(r#"{{"tso":"{target}"}}"#)).0, 200);
    assert!(timestamp(&url, 776).await > target);
    follower.shutdown().await.unwrap();
    let boundary = store
        .read(Read::prefix(
            b"/pd/776/timestamp",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let durable =
        u64::from_be_bytes(boundary.entries[0].value.as_slice().try_into().unwrap()) / 1_000_000;
    n.shutdown().await.unwrap();
    let (mut n, url) = node_at(store, 776, 1, Some(&url)).await;
    assert!(timestamp(&url, 776).await >> 18 > durable);
    assert_eq!(reset(&url, r#"{"tso":"1","force-use-larger":true}"#).0, 200);
    n.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn task_076_differential() {
    let f = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let (mut n, url) = node(store.clone(), 7761, 1).await;
    let rust = observe(&url, 7761).await;
    let before = timestamp(&url, 7761).await;
    let (mut follower, fu) = node(store.clone(), 7761, 2).await;
    assert_eq!(
        reset(&fu, &format!(r#"{{"tso":"{}"}}"#, before + (10000 << 18))).0,
        200
    );
    assert!(timestamp(&url, 7761).await > before + (10000 << 18));
    follower.shutdown().await.unwrap();
    n.shutdown().await.unwrap();
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut go = astersql_pd_compat_tests::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let id = PdClient::connect(go.url.clone())
        .await
        .unwrap()
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let oracle = observe(&go.url, id).await;
    let before = timestamp(&go.url, id).await;
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    crate::grpc_region_lookup_scenarios_test::prepare(&mut client, id).await;
    go.restart_with_args(&binary, &[]).unwrap();
    go.ready().unwrap();
    assert!(timestamp(&go.url, id).await > before);
    let join = format!("--join={}", go.url);
    let mut follower = astersql_pd_compat_tests::harness::Server::start_with_args(
        &binary,
        &["--name=reset-follower", "--initial-cluster=", &join],
    )
    .unwrap();
    follower.ready().unwrap();
    let target = timestamp(&go.url, id).await + (10000 << 18);
    assert_eq!(
        reset(&follower.url, &format!(r#"{{"tso":"{target}"}}"#)).0,
        200
    );
    assert!(timestamp(&go.url, id).await > target);
    follower.stop().unwrap();
    println!("Go: {oracle:?} Rust: {rust:?}");
    assert_eq!(oracle, rust);
    go.stop().unwrap();
}
use astersql_pd_member::{Leadership, Member};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use astersql_pd_tso::{Clock, Oracle, Timestamp, TsoError};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
struct ManualClock(AtomicU64);
impl Clock for ManualClock {
    fn now_millis(&self) -> Result<u64, TsoError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
struct Host {
    oracle: Oracle,
    identity: crate::Identity,
    leader: Arc<Leadership>,
}
#[async_trait::async_trait]
impl crate::PdServices for Host {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    async fn healthy(&self) -> Result<(), String> {
        self.leader.validate().await.map_err(|e| e.to_string())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("unused".into())
    }
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String> {
        self.oracle.allocate(count).await.map_err(|e| e.to_string())
    }
    async fn reset_timestamp(&self, tso: u64, force: bool) -> Result<(), String> {
        self.oracle
            .reset(tso, force, 100)
            .await
            .map_err(|e| e.to_string())
    }
}
async fn saved(store: &dyn MetaStore) -> u64 {
    let v = store
        .read(Read::prefix(
            b"/pd/7762/timestamp",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    u64::from_be_bytes(v.entries[0].value.as_slice().try_into().unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_076_clock_fault_cancel() {
    let f = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(f.store().await);
    let store = Arc::new(RecoveryStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(
        store.clone(),
        cancel.clone(),
        7762,
        1,
        b"reset-clock".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let clock = Arc::new(ManualClock(AtomicU64::new(1000)));
    let oracle = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        10,
    )
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let host = Arc::new(Host {
        oracle,
        leader: leader.clone(),
        identity: crate::Identity {
            cluster_id: 7762,
            member_id: 1,
            name: "reset-clock".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
    });
    let router = crate::router(host.clone(), cancel.clone());
    let shutdown = cancel.clone();
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap()
    });
    let set = |physical: u64, logical: u64, force: bool| {
        reset(
            &url,
            &format!(
                r#"{{"tso":"{}","force-use-larger":{force}}}"#,
                (physical << 18) | logical
            ),
        )
    };
    assert_eq!(set(1000, 0, false).0, 403); // equal logical
    assert_eq!(set(999, 100, false).0, 403); // physical rollback
    assert_eq!(set(1100, 0, false).0, 403); // exclusive gap boundary
    assert_eq!(set(1099, 3, false).0, 200);
    assert_eq!(timestamp(&url, 7762).await, (1099 << 18) | 4);
    let boundary = saved(inner.as_ref()).await;
    assert_eq!(set(1099, 4, true).0, 200);
    assert_eq!(set(999, 0, true).0, 200);
    assert_eq!(saved(inner.as_ref()).await, boundary);
    assert_eq!(timestamp(&url, 7762).await, (1099 << 18) | 5);
    clock.0.store(1, Ordering::SeqCst);
    host.oracle.update().await.unwrap();
    assert_eq!(timestamp(&url, 7762).await, (1099 << 18) | 6);
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(set(1198, 0, false).0, 403);
    assert_eq!(saved(inner.as_ref()).await, boundary);
    assert_eq!(timestamp(&url, 7762).await, (1099 << 18) | 7);
    assert_eq!(set(1198, 0, false).0, 200);
    assert_eq!(timestamp(&url, 7762).await, (1198 << 18) | 1);
    let boundary = saved(inner.as_ref()).await;
    store.mode.store(2, Ordering::SeqCst);
    let u = url.clone();
    let request = tokio::task::spawn_blocking(move || {
        reset(&u, &format!(r#"{{"tso":"{}"}}"#, 1297u64 << 18))
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(request.await.unwrap().0, 500);
    serving.await.unwrap();
    assert_eq!(saved(inner.as_ref()).await, boundary);
    assert!(matches!(
        host.oracle.reset(0, true, 100).await,
        Err(TsoError::Cancelled)
    ));
    leader.resign().await.unwrap();
}

use astersql_pd_storage::{
    Commit, Compare, Lease, LeaseId, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Revision,
    Snapshot,
};

// Only gates real backend IO: failures still come from the real etcd transaction.
pub(crate) struct RecoveryStore {
    pub(crate) inner: Arc<dyn MetaStore>,
    pub(crate) mode: std::sync::atomic::AtomicU8,
    pub(crate) entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaStore for RecoveryStore {
    async fn consensus_cluster_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_cluster_id().await
    }
    async fn consensus_leader_id(&self) -> astersql_pd_storage::Result<Option<u64>> {
        self.inner.consensus_leader_id().await
    }
    async fn read(&self, request: Read) -> astersql_pd_storage::Result<Snapshot> {
        let mut request = request;
        if request
            .start
            .as_ref()
            .is_some_and(|key| key.windows(9).any(|w| w == b"/markers/"))
        {
            match self.mode.load(std::sync::atomic::Ordering::SeqCst) {
                3 => {
                    request.revision = Some(i64::MAX);
                }
                4 => {
                    self.entered.notify_one();
                    std::future::pending::<()>().await;
                }
                _ => {}
            }
        }
        self.inner.read(request).await
    }
    async fn transact(
        &self,
        compares: Vec<Compare>,
        mut writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        let target = writes.iter().any(|write| matches!(write, Mutation::Delete {key} | Mutation::Put {key, ..} if key.ends_with(b"/timestamp")));
        if target {
            match self.mode.swap(0, std::sync::atomic::Ordering::SeqCst) {
                1 => writes.push(Mutation::Put {
                    key: b"/task076/failure".to_vec(),
                    value: vec![],
                    lease: Some(i64::MAX),
                }),
                2 => {
                    self.entered.notify_one();
                    std::future::pending::<()>().await;
                }
                _ => {}
            }
        }
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> astersql_pd_storage::Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> astersql_pd_storage::Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(
        &self,
        prefix: Vec<u8>,
        start_revision: Revision,
    ) -> astersql_pd_storage::Result<Box<dyn MetaWatch>> {
        self.inner.watch(prefix, start_revision).await
    }
    async fn grant(&self, ttl_seconds: i64) -> astersql_pd_storage::Result<Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> astersql_pd_storage::Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> astersql_pd_storage::Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> astersql_pd_storage::Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> astersql_pd_storage::Result<()> {
        self.inner.compact(revision).await
    }
}
