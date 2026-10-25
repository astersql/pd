// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{BasicCluster, KeyRange, Peer, RegionEpoch, RegionInfo, RegionMeta};
use astersql_pd_schedule_operator::ControllerAdmission;
use std::{sync::Arc, time::Duration};
fn peers(id: u64) -> Vec<Peer> {
    (1..=3)
        .map(|store_id| Peer {
            id: id * 10 + store_id,
            store_id,
            ..Default::default()
        })
        .collect()
}
fn put(c: &mut BasicCluster, id: u64, version: u64) {
    c.regions.set_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            peers: peers(id),
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 1,
            }),
            key_range: KeyRange {
                start_key: id.to_be_bytes().to_vec(),
                end_key: (id + 1).to_be_bytes().to_vec(),
            },
            ..Default::default()
        },
        Some(peers(id)[0].clone()),
        vec![],
        vec![],
    )));
}
#[test]
fn task_054_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_054.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let mut c = BasicCluster::new();
        put(&mut c, 100, 6);
        put(&mut c, 101, 6);
        let mut ctl = SplitScatterRetryController::default();
        ctl.record_batch(100, 6, &[101], Duration::ZERO, true);
        let ops = ControllerAdmission::new(10);
        let ids = ctl
            .top_pending(
                &c.regions,
                &ops,
                Duration::from_secs(case["now"].as_u64().unwrap()),
                case["limit"].as_u64().unwrap() as usize,
            )
            .into_iter()
            .map(|p| p.region_id)
            .collect::<Vec<_>>();
        let expected = case["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, expected, "{}", case["name"]);
    }
    println!("task_054: 5 frozen top-pending/TTL cases");
}

fn recorded() -> SplitScatterRetryController {
    let mut ctl = SplitScatterRetryController::default();
    ctl.record_batch(100, 6, &[101], Duration::ZERO, true);
    ctl
}
fn operation(region: &RegionInfo) -> astersql_pd_schedule_operator::Operator {
    use astersql_pd_schedule_operator::*;
    Operator::new(
        INTERNAL_SPLIT_SCATTER_DESC,
        "",
        region.meta().id,
        region.meta().epoch,
        OpKind::SPLIT_SCATTER | OpKind::LEADER,
        1,
        vec![
            TransferLeader {
                from_store: 1,
                to_store: 2,
                ..Default::default()
            }
            .into(),
        ],
    )
}
#[test]
fn task_054_missing_backoff_expiration_and_stale_identity() {
    let mut c = BasicCluster::new();
    let ops = ControllerAdmission::new(10);
    let mut ctl = recorded();
    assert!(
        ctl.top_pending(&c.regions, &ops, Duration::ZERO, 2)
            .is_empty()
    );
    assert_eq!(ctl.metrics().missing, 2);
    assert_eq!(ctl.retry_at(101), Some(Duration::from_secs(10)));
    ctl.top_pending(&c.regions, &ops, Duration::from_secs(9), 2);
    assert_eq!(ctl.metrics().missing, 2);
    put(&mut c, 100, 6);
    put(&mut c, 101, 6);
    assert!(
        ctl.top_pending(&c.regions, &ops, Duration::from_secs(9), 2)
            .is_empty()
    );
    assert_eq!(
        ctl.top_pending(&c.regions, &ops, Duration::from_secs(10), 1)
            .len(),
        1
    );
    assert_eq!(ctl.cleanup_expired(Duration::from_secs(1800)), 0);
    assert_eq!(
        (
            ctl.metrics().expired_attempted,
            ctl.metrics().expired_unattempted
        ),
        (1, 1)
    );
    assert_eq!(ctl.metrics().pending, 0);
    ctl.record_batch(100, 6, &[101], Duration::ZERO, true);
    let stale = ctl.pending(101).unwrap();
    ctl.record_batch(100, 6, &[101], Duration::from_secs(1), true);
    assert!(!ctl.delay(&stale, Duration::from_secs(100)));
    assert!(!ctl.delete(&stale));
    assert_eq!(ctl.retry_at(101), Some(Duration::ZERO));
    // Missing source delays a ready child; older result does not back off.
    let mut missing = recorded();
    let mut only_child = BasicCluster::new();
    put(&mut only_child, 101, 6);
    missing.top_pending(&only_child.regions, &ops, Duration::ZERO, 2);
    assert_eq!(missing.metrics().missing, 2);
    let mut old = recorded();
    put(&mut c, 100, 5);
    put(&mut c, 101, 5);
    assert!(
        old.top_pending(&c.regions, &ops, Duration::ZERO, 2)
            .is_empty()
    );
    assert_eq!(old.retry_at(101), Some(Duration::ZERO));
    put(&mut c, 100, 7);
    put(&mut c, 101, 7);
    old.top_pending(&c.regions, &ops, Duration::ZERO, 2);
    assert_eq!(old.metrics().pending, 0);
    println!(
        "task_054: missing-result/source, exact 10s retry, TTL attempted split, replacement identity, old/stale versions"
    );
}
#[test]
fn task_054_order_capacity_and_running_operator() {
    let ops = ControllerAdmission::new(10);
    let mut ctl = recorded();
    let mut c = BasicCluster::new();
    for id in [100, 101, 200, 201] {
        put(&mut c, id, 6);
    }
    ctl.record_batch(200, 6, &[201], Duration::from_secs(1), true);
    // Earlier expiry wins even though its ID/group sort later.
    let mut order = SplitScatterRetryController::default();
    order.record_batch(200, 6, &[201], Duration::ZERO, true);
    order.record_batch(100, 6, &[101], Duration::from_secs(1), true);
    assert_eq!(
        order.top_pending(&c.regions, &ops, Duration::from_secs(2), 1)[0].region_id,
        200
    );
    ctl.record_batch(200, 6, &[201], Duration::ZERO, true);
    assert_eq!(
        ctl.top_pending(&c.regions, &ops, Duration::ZERO, 3)
            .iter()
            .map(|p| p.region_id)
            .collect::<Vec<_>>(),
        vec![100, 101, 200]
    );
    ops.add_operators(
        &c,
        &[Arc::new(operation(&c.regions.get_region(100).unwrap()))],
    )
    .unwrap();
    assert_eq!(
        ctl.top_pending(&c.regions, &ops, Duration::ZERO, 1)[0].region_id,
        101
    );
    assert_eq!(ctl.retry_at(100), Some(Duration::from_secs(10)));
    let mut full = SplitScatterRetryController::default();
    let children = (2..=4096).collect::<Vec<_>>();
    full.record_batch(1, 1, &children, Duration::ZERO, true);
    assert_eq!(full.metrics().pending, 4096);
    full.record_batch(5000, 1, &[5001], Duration::ZERO, true);
    assert_eq!(full.metrics().dropped, 2);
    assert!(full.pending(5000).is_none());
    full.record_batch(1, 2, &[2], Duration::from_secs(1), true);
    assert_eq!(full.metrics().pending, 4096);
    full.record_batch(5000, 1, &[5001], Duration::from_secs(1800), true);
    assert_eq!(full.metrics().pending, 4);
    assert_eq!(full.metrics().expired_unattempted, 4094);
    full.clear();
    assert_eq!(full.metrics().pending, 0);
    assert_eq!(full.next_dispatch_at(), Duration::ZERO);
    full.record_batch(1, 1, &[], Duration::ZERO, true);
    full.record_batch(1, 1, &[2], Duration::ZERO, false);
    assert_eq!(full.metrics().pending, 0);
}

#[tokio::test]
async fn task_054_dispatch_outcomes() {
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::RegionReplicationContext;
    use astersql_pd_schedule_operator::{AdmissionError, CancelReason, OpKind};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::cell::RefCell;
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 3;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(54),
    );
    manager
        .initialize(3, &[], "", false, &conf, &[])
        .await
        .unwrap();
    let rep = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut c = BasicCluster::new();
    put(&mut c, 100, 6);
    put(&mut c, 101, 6);
    for (case, expected) in [
        ("hot", (2, 0, 0)),
        ("cpu", (0, 2, 0)),
        ("other", (0, 0, 2)),
        ("planning", (0, 0, 2)),
        ("none", (0, 0, 0)),
        ("operator", (0, 0, 0)),
        ("admission", (0, 0, 0)),
    ] {
        let mut ctl = recorded();
        let ops = ControllerAdmission::new(10);
        if case == "admission" {
            let old = operation(&c.regions.get_region(100).unwrap());
            // A non-internal running op reaches the real admission rejection path.
            let old = astersql_pd_schedule_operator::Operator::new(
                "other",
                "",
                100,
                old.region_epoch().copied(),
                OpKind::LEADER,
                1,
                vec![
                    astersql_pd_schedule_operator::TransferLeader {
                        from_store: 1,
                        to_store: 2,
                        ..Default::default()
                    }
                    .into(),
                ],
            );
            ops.add_operators(&c, &[Arc::new(old)]).unwrap();
        }
        let errors = ctl
            .dispatch(
                &c,
                &ops,
                &rep,
                None,
                Duration::ZERO,
                10,
                |region, group, _hint, _ops| {
                    assert_eq!(group, "split-scatter-100-101");
                    match case {
                        "hot" => Err(SplitScatterDispatchError::RetryLater(
                            SplitScatterRetryReason::HotRegion,
                        )),
                        "cpu" => Err(SplitScatterDispatchError::RetryLater(
                            SplitScatterRetryReason::BalancedReadCpu,
                        )),
                        "other" => Err(SplitScatterDispatchError::RetryLater(
                            SplitScatterRetryReason::Other("blocked".into()),
                        )),
                        "planning" => Err(SplitScatterDispatchError::Planning(
                            "injected I/O failure".into(),
                        )),
                        "none" => Ok(None),
                        _ => Ok(Some(operation(region))),
                    }
                },
            )
            .await;
        assert_eq!(
            (
                ctl.metrics().hot,
                ctl.metrics().balanced_read_cpu,
                ctl.metrics().scatter_failed
            ),
            expected,
            "{case}"
        );
        if ["hot", "cpu", "other", "planning"].contains(&case) {
            assert_eq!(errors.len(), 2);
            assert_eq!(ctl.metrics().pending, 2);
            assert_eq!(ctl.retry_at(101), Some(Duration::from_secs(10)));
            ctl.dispatch(
                &c,
                &ops,
                &rep,
                None,
                Duration::from_secs(9),
                10,
                |_, _, _, _| panic!("backoff bypassed"),
            )
            .await;
            assert_eq!(ctl.metrics().scatter_failed, expected.2);
        } else if case == "admission" {
            assert_eq!(
                errors,
                vec![(
                    100,
                    SplitScatterDispatchError::Admission(AdmissionError::Rejected(
                        CancelReason::AlreadyExist
                    ))
                )]
            );
            assert_eq!(ctl.metrics().add_operator_failed, 1);
            assert_eq!(ctl.metrics().pending, 1);
        } else {
            assert!(errors.is_empty());
            assert_eq!(ctl.metrics().pending, 0);
            if case == "operator" {
                assert_eq!(ops.operator_count(OpKind::SPLIT_SCATTER), 2);
                assert_eq!(
                    ops.get_operator(101)
                        .unwrap()
                        .additional_info()
                        .get("batch-group"),
                    Some("split-scatter-100-101".into())
                );
            } else {
                assert_eq!(ctl.metrics().no_operator_needed, 2);
            }
        }
    }
    // Actual replication guard, not a supplied boolean.
    let mut conf_bad = ProviderSnapshot::<()>::default();
    conf_bad.replication.max_replicas = 4;
    conf_bad.replication.enable_placement_rules = false;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(54),
    );
    let rep_bad = RegionReplicationContext {
        conf: &conf_bad,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut ctl = recorded();
    let ops = ControllerAdmission::new(10);
    ctl.dispatch(
        &c,
        &ops,
        &rep_bad,
        None,
        Duration::ZERO,
        10,
        |_, _, _, _| panic!("unreplicated planned"),
    )
    .await;
    assert_eq!(ctl.metrics().not_replicated, 2);
    assert_eq!(ctl.metrics().pending, 2);
    // Disabled and global capacity gates run after cleanup, before deadline gates.
    let mut ctl = recorded();
    let ops = ControllerAdmission::new(10);
    ops.add_operators(
        &c,
        &[Arc::new(operation(&c.regions.get_region(100).unwrap()))],
    )
    .unwrap();
    ctl.dispatch(
        &c,
        &ops,
        &rep,
        None,
        Duration::ZERO,
        1,
        |_, _, _, _| panic!(),
    )
    .await;
    assert_eq!(ctl.next_dispatch_at(), Duration::from_secs(10));
    assert_eq!(ctl.metrics().schedule_limit, 1);
    ctl.dispatch(
        &c,
        &ops,
        &rep,
        None,
        Duration::from_secs(1),
        0,
        |_, _, _, _| panic!(),
    )
    .await;
    assert_eq!(ctl.metrics().pending, 0);
    assert_eq!(ctl.metrics().disabled, 1);
    assert_eq!(ctl.next_dispatch_at(), Duration::ZERO);
    for limit in [0, 1] {
        let mut ctl = recorded();
        ctl.dispatch(
            &c,
            &ops,
            &rep,
            None,
            Duration::from_secs(1800),
            limit,
            |_, _, _, _| panic!(),
        )
        .await;
        assert_eq!(ctl.metrics().expired_unattempted, 2);
        assert_eq!(ctl.metrics().disabled, 0);
        assert_eq!(ctl.metrics().schedule_limit, 0);
    }
    let mut empty = recorded();
    let absent = BasicCluster::new();
    empty
        .dispatch(
            &absent,
            &ops,
            &rep,
            None,
            Duration::ZERO,
            10,
            |_, _, _, _| panic!(),
        )
        .await;
    assert_eq!(empty.next_dispatch_at(), Duration::from_secs(10));
    empty
        .dispatch(
            &c,
            &ops,
            &rep,
            None,
            Duration::from_secs(9),
            10,
            |_, _, _, _| panic!(),
        )
        .await;
    empty.clear();
    assert_eq!(empty.metrics().pending, 0);
    println!(
        "task_054: 7 dispatch outcomes plus replication, disabled, global limit, cleanup ordering, cancellation and empty-pass backoff"
    );
}

#[tokio::test]
async fn task_054_real_labels_store_budget_and_sequential_admission() {
    use astersql_pd_core::{StoreInfo, StoreMeta};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::RegionReplicationContext;
    use astersql_pd_schedule_labeler::{LabelRule, RegionLabeler};
    use astersql_pd_schedule_operator::{AddPeer, OpKind, Operator};
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::cell::RefCell;
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 3;
    conf.schedule.default_store_limit.add_peer = 0.000001;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(54),
    );
    let rep = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut c = BasicCluster::new();
    put(&mut c, 100, 6);
    put(&mut c, 101, 6);
    let mut labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(54),
        0,
    )
    .await
    .unwrap();
    let rule=LabelRule::from_json(br#"{"id":"deny","labels":[{"key":"schedule","value":"deny"}],"rule_type":"key-range","data":[{"start_key":"","end_key":""}]}"#,0).unwrap();
    labels.set_validated_label_rule(rule).await.unwrap();
    let ops = ControllerAdmission::new(10);
    let mut ctl = recorded();
    ctl.dispatch(
        &c,
        &ops,
        &rep,
        Some(&labels),
        Duration::ZERO,
        10,
        |_, _, _, _| panic!("denied region planned"),
    )
    .await;
    assert_eq!(ctl.metrics().schedule_disabled, 2);
    assert_eq!(ctl.retry_at(101), Some(Duration::from_secs(10)));
    for id in [100, 101] {
        let mut region = (*c.regions.get_region(id).unwrap()).clone();
        region.stats_mut().approximate_size = 96;
        region.stats_mut().approximate_keys = 100;
        c.regions.set_region(Arc::new(region));
    }
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 8,
        ..Default::default()
    })));
    let mut ctl = recorded();
    let errors = ctl
        .dispatch(&c, &ops, &rep, None, Duration::ZERO, 10, |r, _, _, ops| {
            let running = ops.operator_count(OpKind::SPLIT_SCATTER);
            assert_eq!(running, if r.meta().id == 100 { 0 } else { 1 });
            Ok(Some(Operator::new(
                INTERNAL_SPLIT_SCATTER_DESC,
                "",
                r.meta().id,
                r.meta().epoch,
                OpKind::SPLIT_SCATTER | OpKind::REGION,
                96,
                vec![
                    AddPeer {
                        to_store: 8,
                        peer_id: 8,
                        ..Default::default()
                    }
                    .into(),
                ],
            )))
        })
        .await;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, 101);
    assert_eq!(ctl.metrics().store_limit, 1);
    assert_eq!(ctl.metrics().operator_created, 1);
    assert_eq!(ctl.metrics().pending, 1);
    assert_eq!(ctl.retry_at(101), Some(Duration::from_secs(10)));
    assert_eq!(ops.operator_count(OpKind::SPLIT_SCATTER), 1);
    // Exactly the available global capacity is selected; the sibling remains unattempted.
    let mut ctl = recorded();
    let ops = ControllerAdmission::new(10);
    ctl.dispatch(&c, &ops, &rep, None, Duration::ZERO, 1, |r, _, _, _| {
        Ok(Some(operation(r)))
    })
    .await;
    assert_eq!(ops.operator_count(OpKind::SPLIT_SCATTER), 1);
    assert_eq!(ctl.metrics().pending, 1);
    ctl.cleanup_expired(Duration::from_secs(1800));
    assert_eq!(ctl.metrics().expired_unattempted, 1);
    println!(
        "task_054: real label deny, store-token rejection, sequential running delta and capacity-limited attempt accounting"
    );
}

#[tokio::test]
async fn task_054_heartbeat_and_range_group() {
    use astersql_pd_codec::{encode_bytes, generate_table_key};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::RegionReplicationContext;
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::cell::RefCell;
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.max_replicas = 3;
    conf.replication.enable_placement_rules = false;
    let mut manager = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(54),
    );
    let rep = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut manager),
        now: 0,
    };
    let mut c = BasicCluster::new();
    put(&mut c, 100, 5);
    put(&mut c, 101, 6);
    let mut ctl = recorded();
    let ops = ControllerAdmission::new(10);
    ctl.dispatch(&c, &ops, &rep, None, Duration::ZERO, 10, |_, _, _, _| {
        panic!("source not observed")
    })
    .await;
    assert_eq!(ctl.metrics().pending, 2);
    assert_eq!(ctl.next_dispatch_at(), Duration::from_secs(10));
    put(&mut c, 100, 7); // source changed again; child remains eligible at version 6
    let child = RegionInfo::new(
        RegionMeta {
            id: 101,
            peers: peers(101),
            epoch: Some(RegionEpoch {
                version: 6,
                conf_ver: 1,
            }),
            key_range: KeyRange {
                start_key: encode_bytes(&generate_table_key(42)),
                end_key: encode_bytes(&generate_table_key(43)),
            },
            ..Default::default()
        },
        Some(peers(101)[0].clone()),
        vec![],
        vec![],
    );
    c.regions.set_region(Arc::new(child));
    ctl.dispatch(
        &c,
        &ops,
        &rep,
        None,
        Duration::from_secs(9),
        10,
        |_, _, _, _| panic!("global backoff"),
    )
    .await;
    ctl.dispatch(
        &c,
        &ops,
        &rep,
        None,
        Duration::from_secs(10),
        10,
        |r, group, hint, _| {
            assert_eq!(r.meta().id, 101);
            assert_eq!(group, "split-scatter-table-42");
            assert!(!hint.start_key.is_empty());
            let op = operation(r);
            op.additional_info().set("group", group);
            Ok(Some(op))
        },
    )
    .await;
    assert_eq!(ctl.metrics().pending, 0);
    let op = ops.get_operator(101).unwrap();
    assert_eq!(
        op.additional_info().get("group"),
        Some("split-scatter-table-42".into())
    );
    assert_eq!(
        op.additional_info().get("batch-group"),
        Some("split-scatter-100-101".into())
    );
}
