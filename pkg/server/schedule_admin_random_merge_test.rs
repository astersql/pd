// Copyright 2026 AsterSQL.
use crate::schedule_admin_random_merge::serve;
use astersql_pd_core::KeyRange;
use astersql_pd_schedule_schedulers::{RandomMergeConfig, RandomMergeScheduler};
use axum::{
    body::{Body, to_bytes},
    http::Request,
    response::Response,
};
use serde_json::Value;

async fn assert_response(response: Response) {
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert!(response.headers().get("allow").is_none());
    assert!(response.headers().get("tidb-error-code").is_none());
    assert_eq!(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"not implements"
    );
}

#[tokio::test]
async fn task_147_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_147.json"
    ))
    .unwrap();
    for config in [
        RandomMergeConfig::default(),
        RandomMergeConfig::new(vec![]),
        RandomMergeConfig::from_args(&["a%00", "z+"]).unwrap(),
    ] {
        let scheduler = RandomMergeScheduler::new(config);
        let before = scheduler.encode_config().unwrap();
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
                case["content_type"].as_str().unwrap(),
                "{case}"
            );
            assert!(response.headers().get("allow").is_none());
            assert!(response.headers().get("tidb-error-code").is_none());
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(
                body.as_ref(),
                case["body"].as_str().unwrap().as_bytes(),
                "{case}"
            );
            assert!(
                serde_json::from_slice::<Value>(&body).is_err(),
                "response is plain text"
            );
            assert_eq!(scheduler.encode_config().unwrap(), before, "{case}");
        }
    }
    println!("task147 HTTP: 19 cases x 3 config states = 57 passed");
}

#[tokio::test]
async fn task_147_unread_body_faults() {
    let scheduler = RandomMergeScheduler::new(RandomMergeConfig::from_args(&[]).unwrap());
    let before = scheduler.encode_config().unwrap();
    for method in ["POST", "GET", "DELETE", "HEAD"] {
        // Polling a failing or stalled external stream would violate BaseScheduler.
        let stream = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                panic!("unsupported random_merge endpoint must not poll the request body")
            },
        );
        assert_response(serve(
            &scheduler,
            Request::builder()
                .method(method)
                .uri("/config")
                .body(Body::from_stream(stream))
                .unwrap(),
        ))
        .await;
        assert_eq!(scheduler.encode_config().unwrap(), before);
    }
    println!("task147 unread-body faults: 4 passed");
}

#[tokio::test]
async fn task_147_schedule_after_unsupported_update() {
    use astersql_pd_core::{BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
    use astersql_pd_core_constant::{KeyType, PriorityLevel};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::RegionReplicationContext;
    use astersql_pd_schedule_operator::{OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::RandomMergeContext;
    use astersql_pd_statistics::StatisticsSnapshot;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::{cell::RefCell, sync::Arc};
    for (in_range, previous, target) in [(true, false, 3), (true, true, 1), (false, false, 0)] {
        let mut cluster = BasicCluster::new();
        for id in 1..=3 {
            let mut store = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            cluster.stores.put_store(Arc::new(store));
            let peers: Vec<_> = (1..=3)
                .map(|s| Peer {
                    id: id * 10 + s,
                    store_id: s,
                    ..Default::default()
                })
                .collect();
            let region = Arc::new(RegionInfo::new(
                RegionMeta {
                    id,
                    peers: peers.clone(),
                    key_range: KeyRange {
                        start_key: vec![b'a' + id as u8 - 1],
                        end_key: vec![b'a' + id as u8],
                    },
                    ..Default::default()
                },
                Some(peers[0].clone()),
                vec![],
                vec![],
            ));
            let (o, v, b) = cluster.regions.set_region(region.clone());
            cluster.regions.update_sub_tree(region, o.as_deref(), &v, b);
        }
        let mut conf = ProviderSnapshot::<()>::default();
        conf.key_type = KeyType::RAW;
        conf.replication.max_replicas = 3;
        let endpoint = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(147);
        let mut manager = RuleManager::new(endpoint.clone(), paths.clone());
        manager
            .initialize(3, &[], "", false, &conf, &[])
            .await
            .unwrap();
        let scheduler = RandomMergeScheduler::new(
            RandomMergeConfig::from_args(if in_range { &["b", "c"] } else { &["x", "z"] }).unwrap(),
        );
        let before = scheduler.encode_config().unwrap();
        endpoint
            .save_scheduler_config(&paths, "random-merge-scheduler", &before)
            .await
            .unwrap();
        for after in [false, true] {
            if after {
                assert_response(serve(
                    &scheduler,
                    Request::builder()
                        .method("POST")
                        .uri("/config")
                        .body(Body::from(if in_range {
                            r#"{"ranges":[]}"#
                        } else {
                            r#"{"ranges":[{"start-key":"62","end-key":"63"}]}"#
                        }))
                        .unwrap(),
                ))
                .await;
            }
            for v2 in [false, true] {
                let stores = cluster.stores.get_stores();
                let rep = RegionReplicationContext {
                    conf: &conf,
                    stores: &stores,
                    manager: RefCell::new(&mut manager),
                    now: 1_000_000_000_000,
                };
                let pair = scheduler
                    .schedule(
                        &mut RandomMergeContext {
                            cluster: &cluster,
                            replication: &rep,
                            labels: None,
                            statistics: &StatisticsSnapshot::default(),
                            hot_degree: 3,
                            random_index: &mut |_| 0,
                            shuffle: &mut |_| {},
                            choose_previous: previous,
                            supports_conf_change_v2: v2,
                        },
                        || panic!("same peers require no allocation"),
                    )
                    .await
                    .unwrap();
                assert_eq!(pair.is_some(), target != 0);
                if let Some(pair) = pair {
                    for (i, op) in pair.iter().enumerate() {
                        assert_eq!(op.desc(), "random-merge-scheduler");
                        assert_eq!(op.kind(), OpKind::MERGE);
                        assert_eq!(op.priority_level(), PriorityLevel::LOW);
                        assert_eq!(op.region_id(), if i == 0 { 2 } else { target });
                        assert_eq!(op.len(), 1);
                        let OpStep::MergeRegion(step) = op.step(0).unwrap() else {
                            panic!("merge required")
                        };
                        assert_eq!(step.is_passive, i == 1);
                        assert_eq!(step.from_region.as_ref().unwrap().id, 2);
                        assert_eq!(step.to_region.as_ref().unwrap().id, target);
                        let region = cluster.regions.get_region(op.region_id()).unwrap();
                        assert_eq!(step.conf_ver_changed(&region), 0);
                        let cmd = step.get_cmd(&region, v2);
                        assert_eq!(cmd.is_some(), i == 0);
                        if let Some(cmd) = cmd {
                            assert_eq!(cmd.merge.unwrap().target.as_ref(), step.to_region.as_ref());
                        }
                    }
                }
            }
            assert_eq!(scheduler.encode_config().unwrap(), before);
            assert_eq!(
                endpoint
                    .load_scheduler_config(&paths, "random-merge-scheduler")
                    .await
                    .unwrap(),
                before
            );
        }
    }
    println!(
        "task147 real scheduling: 3 scenarios x before/after x v1/v2 = 12 decisions passed; persisted config unchanged"
    );
}
