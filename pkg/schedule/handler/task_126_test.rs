// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule::{Coordinator, CoordinatorInputs};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_domain::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let mut cluster = BasicCluster::new();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            epoch: Some(astersql_pd_core::RegionEpoch {
                conf_ver: 2,
                version: 3,
            }),
            peers: peers.clone(),
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"z".to_vec(),
            },
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let mut region = (*region).clone();
    region.stats_mut().approximate_size = 96;
    let region = Arc::new(region);
    let (origin, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, origin.as_deref(), &overlaps, changed);
    let cluster = Arc::new(cluster);
    for id in 1..=3 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.scheduler_max_waiting_operator = 10;
            config.schedule.switch_witness_interval = TtlDuration::from_nanos(10);
            config
        }),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(114),
            JobTime(0),
        ),
        scheduler_configuration: config,
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}
use astersql_pd_schedule_filter::RegionReplicationContext;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_schedule_splitter::SplitRetryPhase;
use std::cell::RefCell;
fn rules() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(126),
    )
}
fn observed(starts: &[&str]) -> astersql_pd_core::RegionsInfo {
    let mut regions = astersql_pd_core::RegionsInfo::new();
    let mut boundaries = vec!["a"];
    boundaries.extend_from_slice(starts);
    boundaries.push("z");
    for (i, pair) in boundaries.windows(2).enumerate() {
        regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id: i as u64 + 2,
                key_range: KeyRange {
                    start_key: pair[0].as_bytes().to_vec(),
                    end_key: pair[1].as_bytes().to_vec(),
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
    }
    regions
}
#[tokio::test]
async fn task_126_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_126.json"
    ))
    .unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let h = Handler::new(Some(&c));
        let keys: Vec<_> = row["hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mut request = h
            .split_regions(&keys, row["retry"].as_i64().unwrap() as i32)
            .unwrap();
        let mut manager = rules();
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = false;
        conf.replication.max_replicas = 3;
        let stores = c.get_cluster().stores.get_stores();
        let context = RegionReplicationContext {
            conf: &conf,
            stores: &stores,
            manager: RefCell::new(&mut manager),
            now: 0,
        };
        if request.phase() != SplitRetryPhase::Done {
            assert!(request.response().is_none());
            let submissions = h
                .start_split_attempt(&mut request, 0, &context)
                .unwrap()
                .unwrap();
            assert!(submissions.rejected.is_empty());
            let interior = !keys.is_empty() && keys.iter().all(|k| !k.is_empty() && *k != "61");
            assert_eq!(submissions.admitted.len(), usize::from(interior));
            for admitted in &submissions.admitted {
                let op = &admitted.admitted[0];
                assert!(op.kind().0 & astersql_pd_schedule_operator::OpKind::SPLIT.0 != 0);
                assert_eq!(
                    op.region_epoch().copied(),
                    Some(astersql_pd_core::RegionEpoch {
                        conf_ver: 2,
                        version: 3
                    })
                );
                let Some(astersql_pd_schedule_operator::OpStep::SplitRegion(step)) = op.step(0)
                else {
                    panic!("split command")
                };
                let expected: Vec<Vec<u8>> =
                    serde_json::from_value(row["decoded"].clone()).unwrap();
                assert_eq!(step.split_keys, expected);
                assert_eq!(step.start_key, b"a");
                assert_eq!(step.end_key, b"z");
                assert_eq!(
                    step.policy,
                    astersql_pd_schedule_hbstream::SplitPolicy::UseKey
                );
            }
            assert!(request.response().is_none(), "admission is not completion");
            let starts: Vec<_> = row["observe"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let regions = observed(&starts);
            request.advance(99, false, &regions);
            assert!(request.response().is_none());
            request.advance(100, false, &regions);
            request.advance(60_000, false, &regions);
            if let SplitRetryPhase::Backoff { until_ms } = request.phase() {
                request.advance(until_ms, false, &regions);
            }
        }
        let response = request.response().expect("settled");
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            row["response"],
            "{}",
            row["id"]
        );
        assert!(
            h.start_split_attempt(&mut request, 100_000, &context)
                .unwrap()
                .is_none()
        );
        pending.shutdown().await;
    }
    println!("task_126: 9 source-derived response cases passed");
}
#[tokio::test]
async fn task_126_hex_and_bootstrap_errors() {
    let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    for (input, message) in [
        ("0", "encoding/hex: odd length hex string"),
        ("gg", "encoding/hex: invalid byte: U+0067 'g'"),
        ("6g0", "encoding/hex: invalid byte: U+0067 'g'"),
    ] {
        let error = h.split_regions(&["6d", input], 1).err().unwrap();
        assert!(matches!(error, SplitAdminError::Hex(_)));
        assert_eq!(error.to_string(), message);
    }
    assert!(
        h.get_operators().unwrap().is_empty(),
        "decode is atomic before admission"
    );
    let missing = Handler::<ProviderSnapshot<()>>::new(None);
    for keys in [vec![], vec!["gg"], vec!["6d"]] {
        assert!(matches!(
            missing.split_regions(&keys, 1),
            Err(SplitAdminError::Handler(HandlerError::NotBootstrapped))
        ));
    }
    pending.shutdown().await;
}
#[tokio::test]
async fn task_126_retry_remaining_keys() {
    let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    let mut manager = rules();
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 3;
    let stores = c.get_cluster().stores.get_stores();
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut request = h.split_regions(&["6d", "74", "74"], 1).unwrap();
    let first = h
        .start_split_attempt(&mut request, 0, &context)
        .unwrap()
        .unwrap();
    assert_eq!(first.admitted.len(), 1);
    assert!(Arc::ptr_eq(
        &first.admitted[0].admitted[0],
        &h.get_operator(1).unwrap()
    ));
    let partial = observed(&["m"]);
    request.advance(100, false, &partial);
    request.advance(60_000, false, &partial);
    assert_eq!(
        request.phase(),
        SplitRetryPhase::Backoff { until_ms: 60_100 }
    );
    assert!(request.response().is_none());
    request.advance(60_099, false, &partial);
    assert!(
        h.start_split_attempt(&mut request, 60_099, &context)
            .unwrap()
            .is_none()
    );
    request.advance(60_100, false, &partial);
    h.remove_operators(std::time::Instant::now()).unwrap();
    let second = h
        .start_split_attempt(&mut request, 60_100, &context)
        .unwrap()
        .unwrap();
    assert_eq!(second.admitted.len(), 1);
    let Some(astersql_pd_schedule_operator::OpStep::SplitRegion(step)) =
        second.admitted[0].admitted[0].step(0)
    else {
        panic!("split")
    };
    assert_eq!(step.split_keys, vec![b"t".to_vec(), b"t".to_vec()]);
    let full = observed(&["m", "t"]);
    request.advance(60_200, false, &full);
    request.advance(120_100, false, &full);
    assert_eq!(
        serde_json::to_value(request.response().unwrap()).unwrap(),
        serde_json::json!({"processed-percentage":100,"regions-id":[3,4]})
    );
    assert!(
        h.start_split_attempt(&mut request, 200_000, &context)
            .unwrap()
            .is_none()
    );
    pending.shutdown().await;
}
#[tokio::test]
async fn task_126_cancellation_keeps_observed_progress_and_retries() {
    for scan_before_cancel in [false, true] {
        let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let h = Handler::new(Some(&c));
        let mut manager = rules();
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules = false;
        conf.replication.max_replicas = 3;
        let stores = c.get_cluster().stores.get_stores();
        let context = RegionReplicationContext {
            conf: &conf,
            stores: &stores,
            manager: RefCell::new(&mut manager),
            now: 0,
        };
        let mut request = h.split_regions(&["6d", "74"], 1).unwrap();
        h.start_split_attempt(&mut request, 0, &context)
            .unwrap()
            .unwrap();
        let partial = observed(&["m"]);
        if scan_before_cancel {
            request.advance(100, false, &partial);
        }
        request.advance(150, true, &partial);
        assert_eq!(request.phase(), SplitRetryPhase::Backoff { until_ms: 250 });
        assert!(request.response().is_none());
        request.advance(249, true, &partial);
        assert!(request.response().is_none());
        request.advance(250, true, &partial);
        assert_eq!(request.phase(), SplitRetryPhase::Ready);
        h.remove_operators(std::time::Instant::now()).unwrap();
        assert_eq!(
            h.start_split_attempt(&mut request, 250, &context)
                .unwrap()
                .unwrap()
                .admitted
                .len(),
            1
        );
        request.advance(250, true, &observed(&["m", "t"]));
        assert_eq!(request.phase(), SplitRetryPhase::Backoff { until_ms: 450 });
        request.advance(449, true, &partial);
        assert!(request.response().is_none());
        request.advance(450, true, &partial);
        assert_eq!(
            serde_json::to_value(request.response().unwrap()).unwrap(),
            if scan_before_cancel {
                serde_json::json!({"processed-percentage":50,"regions-id":[3]})
            } else {
                serde_json::json!({"processed-percentage":0,"regions-id":[]})
            }
        );
        pending.shutdown().await;
    }
}
#[tokio::test]
async fn task_126_admission_fault_and_replication_suspect() {
    let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    let mut manager = rules();
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 3;
    let stores = c.get_cluster().stores.get_stores();
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut first = h.split_regions(&["6d"], 0).unwrap();
    h.start_split_attempt(&mut first, 0, &context)
        .unwrap()
        .unwrap();
    let owner = h.get_operator(1).unwrap();
    let mut rejected = h.split_regions(&["74"], 0).unwrap();
    let submissions = h
        .start_split_attempt(&mut rejected, 0, &context)
        .unwrap()
        .unwrap();
    assert!(submissions.admitted.is_empty());
    assert!(matches!(
        submissions.rejected[&1],
        astersql_pd_schedule_splitter::SplitExecutionError::Admission(
            astersql_pd_schedule_operator::AdmissionError::Rejected(
                astersql_pd_schedule_operator::CancelReason::AlreadyExist
            )
        )
    ));
    assert!(Arc::ptr_eq(&owner, &h.get_operator(1).unwrap()));
    let full = observed(&["t"]);
    rejected.advance(100, false, &full);
    rejected.advance(200, false, &full);
    assert_eq!(
        serde_json::to_value(rejected.response().unwrap()).unwrap(),
        serde_json::json!({"processed-percentage":0,"regions-id":[]})
    );
    drop(context);
    conf.replication.max_replicas = 4;
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &stores,
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut under = h.split_regions(&["6d"], 0).unwrap();
    let submissions = h
        .start_split_attempt(&mut under, 0, &context)
        .unwrap()
        .unwrap();
    assert!(submissions.admitted.is_empty());
    assert_eq!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions(),
        vec![1]
    );
    under.advance(100, false, &full);
    under.advance(200, false, &full);
    assert_eq!(under.response().unwrap().processed_percentage, 0);
    pending.shutdown().await;
}
