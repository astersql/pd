// Copyright 2026 AsterSQL.
use crate::schedule_admin_balance_region::serve;
use astersql_pd_core::KeyRange;
use astersql_pd_schedule_schedulers::{BalanceRegionConfig, BalanceRegionScheduler};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::Value;

#[tokio::test]
async fn task_138_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_138.json"
    ))
    .unwrap();
    let mut scheduler = BalanceRegionScheduler::default();
    scheduler.config = BalanceRegionConfig::new(vec![KeyRange {
        start_key: b"c".to_vec(),
        end_key: b"d".to_vec(),
    }]);
    let original = serde_json::to_value(&scheduler.config).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let response = serve(
            &scheduler,
            Request::builder()
                .method(case["method"].as_str().unwrap())
                .uri(case["path"].as_str().unwrap())
                .body(Body::from(case["input"].as_str().unwrap().to_owned()))
                .unwrap(),
        );
        assert_eq!(
            response.status().as_u16(),
            case["status"].as_u64().unwrap() as u16,
            "{case}"
        );
        assert_eq!(
            response.headers()["content-type"],
            case["content_type"].as_str().unwrap()
        );
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .as_ref(),
            case["body"].as_str().unwrap().as_bytes()
        );
        assert_eq!(serde_json::to_value(&scheduler.config).unwrap(), original);
        assert_eq!(scheduler.name, "balance-region-scheduler");
    }
    println!("task138 HTTP cases=15 passed; custom region range unchanged");
}

#[tokio::test]
async fn task_138_body_io_failure() {
    let mut scheduler = BalanceRegionScheduler::default();
    scheduler.config = BalanceRegionConfig::new(vec![KeyRange {
        start_key: b"x".to_vec(),
        end_key: b"z".to_vec(),
    }]);
    let original = serde_json::to_value(&scheduler.config).unwrap();
    // A failing body must remain unread, matching BaseScheduler.ServeHTTP.
    let chunks = futures_util::stream::poll_fn(
        |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
            panic!("unsupported endpoint polled a failing external body")
        },
    );
    let response = serve(
        &scheduler,
        Request::builder()
            .method("POST")
            .uri("/config")
            .body(Body::from_stream(chunks))
            .unwrap(),
    );
    assert_eq!(response.status(), 200);
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"not implements"
    );
    assert_eq!(serde_json::to_value(&scheduler.config).unwrap(), original);
    println!("task138 body fault cases=1 passed; no I/O or configuration update");
}

#[tokio::test]
async fn task_138_schedule_after_unsupported_update() {
    use astersql_pd_core::{
        BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreStats,
    };
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::{OpInfluence, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::RegionBalanceContext;
    use astersql_pd_statistics::StatisticsSnapshot;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::sync::Arc;
    let mut core = BasicCluster::new();
    for (id, size) in [(1, 288), (2, 96), (3, 96), (4, 0)] {
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_region_size(size);
        s.set_region_count(10);
        s.set_last_heartbeat_unix_nanos(1_000_000_000_000);
        s.update_raw_stats(StoreStats {
            capacity: 6 * (1u64 << 40),
            used_size: size as u64 * (1 << 20),
            available: 6 * (1u64 << 40) - size as u64 * (1 << 20),
            ..Default::default()
        });
        core.stores.put_store(Arc::new(s));
    }
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id: id + 10,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let mut region = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[1].clone()),
        vec![],
        vec![],
    );
    region.stats_mut().approximate_size = 96;
    let region = Arc::new(region);
    let (o, v, b) = core.regions.set_region(region.clone());
    core.regions.update_sub_tree(region, o.as_deref(), &v, b);
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.tolerant_size_ratio = 1.0;
    config.schedule.region_score_formula_version = "v1".into();
    config.replication.max_replicas = 3;
    config.replication.enable_placement_rules = false;
    let backend = Arc::new(MemoryStore::default());
    let endpoint = StorageEndpoint::new(backend.clone());
    let paths = ClusterPaths::new(138);
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(backend.clone())),
        paths.clone(),
    );
    rules
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap();
    let labels = astersql_pd_schedule_labeler::RegionLabeler::load(
        StorageEndpoint::new(backend.clone()),
        paths.clone(),
        Default::default(),
    )
    .await
    .unwrap();
    let influence = OpInfluence::new();
    let before = influence.snapshot();
    let statistics = StatisticsSnapshot::default();
    let key_ranges = astersql_pd_schedule_keyrange::Manager::new();
    let mut random = |_: usize| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut allocate = || Ok(1000);
    let mut ctx = RegionBalanceContext {
        cluster: &core,
        config: &config,
        rules: &mut rules,
        affinity: AffinityFilter {
            manager: None,
            labels: &labels,
            now: 1000,
        },
        statistics: &statistics,
        hot_degree: 1,
        key_ranges: &key_ranges,
        influence: &influence,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut random,
        shuffle: &mut shuffle,
        allocate: &mut allocate,
        supports_conf_change_v2: false,
        allow_empty_regions: false,
    };
    let mut scheduler = BalanceRegionScheduler::default();
    for after_request in [false, true] {
        if after_request {
            let response = serve(
                &scheduler,
                Request::builder()
                    .method("POST")
                    .uri("/config")
                    .body(Body::from(
                        r#"{"ranges":[{"start-key":"63","end-key":"64"}]}"#,
                    ))
                    .unwrap(),
            );
            assert_eq!(response.status(), 200);
        }
        let result = scheduler.schedule(&mut ctx, false).await;
        assert!(result.errors.is_empty());
        assert_eq!(result.operators.len(), 1);
        let op = &result.operators[0];
        assert_eq!(op.region_id(), 1);
        assert!(
            matches!(op.step(0),Some(OpStep::AddLearner(s)) if s.to_store==4 && s.peer_id==1000)
        );
        assert!(matches!(op.step(op.len()-1),Some(OpStep::RemovePeer(s)) if s.from_store==1));
        assert_eq!(influence.snapshot(), before);
        assert!(
            endpoint
                .load_scheduler_config(&paths, &scheduler.name)
                .await
                .unwrap()
                .is_empty()
        );
    }
    println!(
        "task138 real schedule cases=2 passed; unchanged region1 move 1->4, no persisted config"
    );
}
