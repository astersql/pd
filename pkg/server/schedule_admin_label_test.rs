// Copyright 2026 AsterSQL.
use crate::schedule_admin_label::serve;
use astersql_pd_core::KeyRange;
use astersql_pd_schedule_schedulers::{LabelConfig, LabelScheduler};
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
async fn task_146_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_146.json"
    ))
    .unwrap();
    for config in [
        LabelConfig::default(),
        LabelConfig::new(vec![]),
        LabelConfig::from_args(&["a%00", "z+"]).unwrap(),
    ] {
        let scheduler = LabelScheduler::new(config);
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
    println!("task146 HTTP: 19 cases x 3 config states = 57 passed");
}

#[tokio::test]
async fn task_146_unread_body_faults() {
    let scheduler = LabelScheduler::new(LabelConfig::from_args(&[]).unwrap());
    let before = scheduler.encode_config().unwrap();
    for method in ["POST", "GET", "DELETE", "HEAD"] {
        // Polling a failing or stalled external stream would violate BaseScheduler.
        let stream = futures_util::stream::poll_fn(
            |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                panic!("unsupported label endpoint must not poll the request body")
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
    println!("task146 unread-body faults: 4 passed");
}

#[tokio::test]
async fn task_146_schedule_after_unsupported_update() {
    use astersql_pd_core::{
        BasicCluster, Peer, RegionInfo, RegionMeta, StoreInfo, StoreLabel, StoreMeta,
    };
    use astersql_pd_core_constant::PriorityLevel;
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::{ProviderSnapshot, REJECT_LEADER};
    use astersql_pd_schedule_operator::{OpKind, OpStep};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::EvictLeaderContext;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::sync::Arc;
    for (reject, in_range, expected) in [(true, true, 1), (true, false, 0), (false, true, 0)] {
        let mut cluster = BasicCluster::new();
        let label = StoreLabel {
            key: "noleader".into(),
            value: "true".into(),
        };
        for id in 1..=3 {
            let mut store = StoreInfo::new(StoreMeta {
                id,
                labels: if id == 1 { vec![label.clone()] } else { vec![] },
                ..Default::default()
            });
            store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
            cluster.stores.put_store(Arc::new(store));
        }
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
                key_range: KeyRange {
                    start_key: b"b".to_vec(),
                    end_key: b"c".to_vec(),
                },
                peers: peers.clone(),
                ..Default::default()
            },
            Some(peers[0].clone()),
            vec![],
            vec![],
        ));
        let (o, v, b) = cluster.regions.set_region(region.clone());
        cluster
            .regions
            .update_sub_tree(region.clone(), o.as_deref(), &v, b);
        let mut config = ProviderSnapshot::<()>::default();
        if reject {
            config
                .label_properties
                .insert(REJECT_LEADER.into(), vec![label]);
        }
        let endpoint = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(146);
        let mut rules = RuleManager::new(endpoint.clone(), paths.clone());
        rules
            .initialize(3, &[], "", false, &config, &[])
            .await
            .unwrap();
        let scheduler = LabelScheduler::new(
            LabelConfig::from_args(if in_range { &["a", "z"] } else { &["x", "z"] }).unwrap(),
        );
        let before = scheduler.encode_config().unwrap();
        // Persist the real baseline to detect accidental writes from transport.
        endpoint
            .save_scheduler_config(&paths, "label-scheduler", &before)
            .await
            .unwrap();
        for after in [false, true] {
            if after {
                // Attempt to remove the active range or make an inactive range active.
                let body = if in_range {
                    r#"{"ranges":[]}"#
                } else {
                    r#"{"ranges":[{"start-key":"61","end-key":"7a"}]}"#
                };
                assert_response(serve(
                    &scheduler,
                    Request::builder()
                        .method("POST")
                        .uri("/config")
                        .body(Body::from(body))
                        .unwrap(),
                ))
                .await;
            }
            let result = scheduler
                .schedule(&mut EvictLeaderContext {
                    cluster: &cluster,
                    config: &config,
                    rules: &mut rules,
                    now_unix_nanos: 1_000_000_000_000,
                    random_index: &mut |_| 0,
                    shuffle: &mut |_| {},
                })
                .await;
            assert!(result.errors.is_empty());
            assert_eq!(
                result.operators.len(),
                expected,
                "reject={reject} in_range={in_range} after={after}"
            );
            for op in result.operators {
                assert_eq!(op.desc(), "label-reject-leader");
                assert_eq!(op.region_id(), 1);
                assert_eq!(op.kind(), OpKind::LEADER);
                assert_eq!(op.priority_level(), PriorityLevel::MEDIUM);
                assert_eq!(op.len(), 1);
                let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                    panic!("expected transfer")
                };
                assert_eq!((step.from_store, step.to_store), (1, 2));
                assert_eq!(step.conf_ver_changed(&region), 0);
                for v2 in [false, true] {
                    assert_eq!(
                        step.get_cmd(&region, v2)
                            .transfer_leader
                            .unwrap()
                            .peer
                            .unwrap()
                            .store_id,
                        2
                    );
                }
            }
            assert_eq!(scheduler.encode_config().unwrap(), before);
            assert_eq!(
                endpoint
                    .load_scheduler_config(&paths, "label-scheduler")
                    .await
                    .unwrap(),
                before
            );
        }
    }
    println!(
        "task146 real scheduling: 3 scenarios x before/after = 6 decisions passed; saved config unchanged"
    );
}
