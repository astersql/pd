// Copyright 2026 AsterSQL.
use crate::schedule_admin_hot_region::serve;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_schedulers::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use std::sync::Arc;
async fn call(
    s: &mut HotRegionConfig,
    e: &StorageEndpoint,
    method: &str,
    path: &str,
    body: impl Into<Body>,
) -> (u16, String, String) {
    let r = serve(
        s,
        e,
        &ClusterPaths::new(144),
        Request::builder()
            .method(method)
            .uri(path)
            .body(body.into())
            .unwrap(),
    )
    .await;
    assert!(r.headers().get("allow").is_none());
    let status = r.status().as_u16();
    let ct = r
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (
        status,
        String::from_utf8(to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap(),
        ct,
    )
}
fn normalized(mut v: Value) -> Value {
    match &mut v {
        Value::Number(n) => return json!(n.as_f64().unwrap()),
        Value::Array(a) => {
            for x in a {
                *x = normalized(x.take());
            }
        }
        Value::Object(o) => {
            for x in o.values_mut() {
                *x = normalized(x.take());
            }
        }
        _ => {}
    }
    v
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_144.json"
    ))
    .unwrap()
}
#[tokio::test]
async fn task_144_behavior_cases() {
    let f = fixture();
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    for c in f["cases"].as_array().unwrap() {
        let mut s = HotRegionConfig::default();
        let before = s.encode().unwrap();
        let (status, body, ct) = call(
            &mut s,
            &e,
            c["method"].as_str().unwrap(),
            c["path"].as_str().unwrap(),
            c["input"].as_str().unwrap().to_owned(),
        )
        .await;
        assert_eq!(
            status,
            c["status"].as_u64().unwrap() as u16,
            "{}: {body}",
            c["name"]
        );
        if c["json_body"] == true {
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap(),
                c["body"],
                "{}",
                c["name"]
            );
            assert_eq!(ct, "application/json; charset=UTF-8");
        } else {
            assert_eq!(body, c["body"].as_str().unwrap(), "{}", c["name"]);
            if status == 200 || status == 400 {
                assert_eq!(ct, "text/plain; charset=UTF-8");
            }
        }
        if status != 200 {
            assert_eq!(s.encode().unwrap(), before, "rejection must be atomic");
        } else if body == "Config is updated." {
            let input: Value = serde_json::from_str(c["input"].as_str().unwrap()).unwrap();
            let stored: Value = serde_json::from_slice(&s.encode().unwrap()).unwrap();
            for (k, v) in input.as_object().unwrap() {
                if stored.get(k).is_some() && !v.is_null() {
                    assert_eq!(
                        normalized(stored[k].clone()),
                        normalized(v.clone()),
                        "updated field {k}"
                    );
                }
            }
        }
    }
    // Full wire snapshot, default capabilities and two version fallback modes.
    let mut s = HotRegionConfig::default();
    for (query, cpu, read, write) in [
        (false, false, json!(["byte", "key"]), json!(["key", "byte"])),
        (
            true,
            false,
            json!(["query", "byte"]),
            json!(["query", "byte"]),
        ),
        (true, true, json!(["cpu", "byte"]), json!(["query", "byte"])),
    ] {
        s.set_support(query, cpu);
        let mut expected = f["defaults"].clone();
        expected["read-priorities"] = read;
        expected["write-leader-priorities"] = write;
        let (status, body, ct) = call(&mut s, &e, "GET", "/list", "").await;
        assert_eq!(status, 200);
        assert_eq!(ct, "application/json; charset=UTF-8");
        assert_eq!(
            normalized(serde_json::from_str::<Value>(&body).unwrap()),
            normalized(expected)
        );
    }
    for c in f["list_cases"].as_array().unwrap() {
        let mut state = HotRegionConfig::default();
        state.set_support(c["query"].as_bool().unwrap(), c["cpu"].as_bool().unwrap());
        assert_eq!(
            call(&mut state, &e, "POST", "/config", c["patch"].to_string())
                .await
                .0,
            200
        );
        let (status, body, ct) = call(&mut state, &e, "GET", "/list", "").await;
        assert_eq!(status, 200);
        assert_eq!(ct, "application/json; charset=UTF-8");
        assert_eq!(
            normalized(serde_json::from_str(&body).unwrap()),
            normalized(c["expected"].clone())
        );
    }
    println!(
        "task144 HTTP cases={} + 3 capability + 6 updated complete snapshots",
        f["cases"].as_array().unwrap().len()
    );
}

use astersql_pd_storage_kv::{
    Commit, Compare, Lease, MetaWatch, Mutation, RawTxnOp, RawTxnResponse, Read, Snapshot,
    StoreError,
};
/// Only the I/O boundary is faulted; normal operations use the canonical backend.
#[derive(Default)]
struct FaultStore {
    store: MemoryStore,
    fail: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl astersql_pd_storage_kv::MetaStore for FaultStore {
    async fn read(&self, r: Read) -> astersql_pd_storage_kv::Result<Snapshot> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.read(r).await
    }
    async fn transact(
        &self,
        c: Vec<Compare>,
        w: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(StoreError::Unavailable);
        }
        self.store.transact(c, w).await
    }
    async fn transact_branches(
        &self,
        c: Vec<Compare>,
        t: Vec<Mutation>,
        e: Vec<Mutation>,
    ) -> astersql_pd_storage_kv::Result<Commit> {
        self.store.transact_branches(c, t, e).await
    }
    async fn transact_raw(
        &self,
        c: Vec<Compare>,
        t: Vec<RawTxnOp>,
        e: Vec<RawTxnOp>,
    ) -> astersql_pd_storage_kv::Result<RawTxnResponse> {
        self.store.transact_raw(c, t, e).await
    }
    async fn watch(
        &self,
        p: Vec<u8>,
        r: i64,
    ) -> astersql_pd_storage_kv::Result<Box<dyn MetaWatch>> {
        self.store.watch(p, r).await
    }
    async fn grant(&self, t: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.grant(t).await
    }
    async fn keep_alive(&self, l: i64) -> astersql_pd_storage_kv::Result<Lease> {
        self.store.keep_alive(l).await
    }
    async fn revoke(&self, l: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.revoke(l).await
    }
    async fn ttl(&self, l: i64) -> astersql_pd_storage_kv::Result<i64> {
        self.store.ttl(l).await
    }
    async fn compact(&self, r: i64) -> astersql_pd_storage_kv::Result<()> {
        self.store.compact(r).await
    }
}

#[tokio::test]
async fn task_144_faults_and_body_errors() {
    use std::sync::atomic::Ordering;
    let backend = Arc::new(FaultStore::default());
    let e = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(144);
    let mut s = HotRegionConfig::default();
    assert_eq!(
        call(&mut s, &e, "POST", "/config", r#"{"pending-weight":2}"#)
            .await
            .1,
        "Config is updated."
    );
    let saved = e
        .load_scheduler_config(&paths, HOT_REGION_CONFIG_NAME)
        .await
        .unwrap();
    backend.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        call(&mut s, &e, "POST", "/config", r#"{"pending-weight":3}"#)
            .await
            .1,
        "Config is updated."
    );
    assert_eq!(s.params().pending_weight, 3.);
    assert_eq!(
        call(&mut s, &e, "POST", "/config", r#"{"pending-weight":3}"#)
            .await
            .1,
        "Config is the same with origin, so do nothing."
    );
    let (_, body, _) = call(&mut s, &e, "GET", "/list", "").await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["pending-weight"],
        3.
    );
    assert!(s.reload(&e, &paths).await.is_err());
    assert_eq!(s.params().pending_weight, 3.);
    backend.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        e.load_scheduler_config(&paths, HOT_REGION_CONFIG_NAME)
            .await
            .unwrap(),
        saved
    );
    s.reload(&e, &paths).await.unwrap();
    assert_eq!(s.params().pending_weight, 2.);
    assert_eq!(
        call(&mut s, &e, "POST", "/config", r#"{"pending-weight":4}"#)
            .await
            .0,
        200
    );
    let mut restored = HotRegionConfig::default();
    restored.reload(&e, &paths).await.unwrap();
    assert_eq!(restored.params(), s.params());
    for prefix in [r#"{"pending-weight":5}"#, r#"{"pending-weight":"#] {
        let before = s.encode().unwrap();
        let stream = futures_util::stream::iter(vec![
            Ok(axum::body::Bytes::from_static(prefix.as_bytes())),
            Err(std::io::Error::other("task144 body unavailable")),
        ]);
        let (status, body, ct) =
            call(&mut s, &e, "POST", "/config", Body::from_stream(stream)).await;
        assert_eq!(status, 500);
        assert_eq!(
            serde_json::from_str::<String>(&body).unwrap(),
            "task144 body unavailable"
        );
        assert_eq!(ct, "application/json; charset=UTF-8");
        assert_eq!(s.encode().unwrap(), before);
    }
    println!(
        "task144 10 storage/publication/reload assertions + 2 actual body I/O failures passed"
    );
}

use astersql_pd_core::{Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoresInfo};
use astersql_pd_statistics::*;
fn detail(id: u64, loads: Loads) -> StoreLoadDetail {
    StoreLoadDetail {
        store_summary_info: StoreSummaryInfo {
            store_info: StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            }),
            ..Default::default()
        },
        hot_peers: vec![],
        load_pred: StoreLoadPred {
            current: StoreLoad {
                loads,
                ..Default::default()
            },
            future: StoreLoad {
                loads,
                ..Default::default()
            },
            stddev: StoreLoad {
                loads: [1.; 4],
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
fn peer(store_id: u64, region_id: u64, loads: Loads) -> HotPeerStat {
    HotPeerStat {
        store_id,
        region_id,
        loads: loads.to_vec(),
        ..Default::default()
    }
}
fn region(id: u64, stores: &[u64], leader: u64) -> RegionInfo {
    let peers: Vec<_> = stores
        .iter()
        .map(|&s| Peer {
            id: id * 10 + s,
            store_id: s,
            ..Default::default()
        })
        .collect();
    let lead = peers.iter().find(|p| p.store_id == leader).cloned();
    RegionInfo::new(
        RegionMeta {
            id,
            peers,
            ..Default::default()
        },
        lead,
        vec![],
        vec![],
    )
}
fn stores() -> StoresInfo {
    let s = StoresInfo::new();
    for id in 1..=4 {
        s.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    s
}

#[tokio::test]
async fn task_144_updated_scheduling() {
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_operator::OpKind;
    use astersql_pd_schedule_operator::OpStep;
    let e = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut state = HotRegionConfig::default();
    let stores = stores();
    let provider = ProviderSnapshot::<()>::default();
    let mut allocate = || Ok(100);
    let mut ctx = HotSolveContext {
        stores: &stores,
        config: &provider,
        rules: None,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        allocate: &mut allocate,
    };
    let src = detail(1, [1000., 1000., 0., 0.]);
    let dst = detail(2, [500., 500., 0., 0.]);
    let p = peer(1, 10, [200., 200., 0., 0.]);
    let leader = region(10, &[1, 2, 3], 1);
    let mut candidates: [Vec<HotSolveCandidate<'_>>; 4] = std::array::from_fn(|_| vec![]);
    candidates[HotResource::WriteLeader as usize]
        .push(HotSolveCandidate::new(&src, &dst, &p, &leader));
    let mut count = 0;
    for version in ["v1", "v2"] {
        for forbid in ["none", "write", "read"] {
            let patch=json!({"rank-formula-version":version,"forbid-rw-type":forbid,"read-priorities":["byte","key"],"write-leader-priorities":["byte","key"],"min-hot-byte-rate":1,"min-hot-key-rate":1}).to_string();
            assert_eq!(call(&mut state, &e, "POST", "/config", patch).await.0, 200);
            let mut scheduler = HotScheduler::new(
                state.params().clone(),
                true,
                true,
                std::array::from_fn(|_| HotRankInputs {
                    step: StoreLoad {
                        loads: [10.; 4],
                        hot_peer_count: 1.,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .unwrap();
            let out = scheduler
                .dispatch(HotResource::WriteLeader, &candidates, &mut ctx)
                .await;
            if forbid == "write" {
                assert_eq!(out.diagnostic, Some(HotScheduleDiagnostic::Forbidden));
                assert!(out.offers.is_empty());
            } else {
                assert_eq!(out.diagnostic, None);
                assert!(out.errors.is_empty());
                assert_eq!(out.offers.len(), 1);
                let op = &out.offers[0].1.operators[0];
                assert_eq!(op.region_id(), 10);
                assert_ne!(op.kind().0 & OpKind::LEADER.0, 0);
                assert_eq!(op.priority_level(), PriorityLevel::HIGH);
                assert!(
                    matches!(op.step(0),Some(OpStep::TransferLeader(x)) if x.from_store==1 && x.to_store==2)
                );
            }
            count += 1;
        }
    }
    println!("task144 {count} HTTP-updated v1/v2 real scheduling cases passed");
}
