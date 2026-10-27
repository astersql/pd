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
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_kv::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let mut cluster = BasicCluster::new();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if id == 3 && config["learner"].as_bool().unwrap_or(false) {
                astersql_pd_core::PeerRole::Learner
            } else {
                astersql_pd_core::PeerRole::Voter
            },
            is_witness: id == 3 && config["witness"].as_bool().unwrap_or(false),
            ..Default::default()
        })
        .collect();
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            epoch: Some(astersql_pd_core::RegionEpoch {
                conf_ver: 1,
                version: 2,
            }),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let mut region = (*region).clone();
    region.stats_mut().approximate_size = 96;
    let region = altered(&region, config["source"].as_str().unwrap_or(""));
    let (origin, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, origin.as_deref(), &overlaps, changed);
    for id in 2..=3 {
        let mut meta = cluster.regions.get_region(1).unwrap().meta().clone();
        meta.id = id;
        meta.key_range.start_key = vec![b'a' + id as u8 - 1];
        meta.key_range.end_key = if id == 3 {
            vec![]
        } else {
            vec![b'a' + id as u8]
        };
        meta.epoch = Some(astersql_pd_core::RegionEpoch {
            conf_ver: id,
            version: id + 1,
        });
        for peer in &mut meta.peers {
            peer.id += id * 10;
        }
        let region = RegionInfo::new(meta.clone(), Some(meta.peers[0].clone()), vec![], vec![]);
        cluster.regions.set_region(altered(
            &region,
            if id == 2 {
                config["target"].as_str().unwrap_or("")
            } else {
                ""
            },
        ));
    }
    let cluster = Arc::new(cluster);
    for id in 1..=5 {
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
    let rules = config["rules"].as_bool().unwrap_or(false);
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.scheduler_max_waiting_operator = 10;
            config.replication.enable_placement_rules = rules;
            config.replication.max_replicas = 3;
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

use astersql_pd_schedule_operator::*;
use astersql_pd_schedule_placement::RuleManager;
use std::time::Instant;
fn manager() -> RuleManager {
    RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        ClusterPaths::new(120),
    )
}
fn context(manager: &mut RuleManager) -> AdminRangeContext<'_> {
    AdminRangeContext {
        manager,
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        now: Instant::now(),
    }
}
#[tokio::test]
async fn task_120_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_120.json"
    ))
    .unwrap();
    for case in fixture["split"].as_array().unwrap() {
        let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let h = Handler::new(Some(&c));
        let keys: Vec<String> = case["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        h.add_split_region_operator(1, case["policy"].as_str().unwrap(), &keys, Instant::now())
            .unwrap();
        let op = h.get_operator(1).unwrap();
        assert_eq!(
            op.region_epoch(),
            c.get_cluster()
                .regions
                .get_region(1)
                .unwrap()
                .meta()
                .epoch
                .as_ref()
        );
        assert_eq!(op.kind(), OpKind::ADMIN);
        assert_eq!(
            op.priority_level(),
            astersql_pd_core_constant::PriorityLevel::URGENT
        );
        assert_eq!(op.status(), STARTED);
        assert_eq!(op.len(), 1);
        let OpStep::SplitRegion(s) = op.step(0).unwrap() else {
            panic!()
        };
        assert_eq!(s.policy.as_raw(), case["raw"].as_i64().unwrap() as i32);
        assert_eq!(
            serde_json::to_value(&s.split_keys).unwrap(),
            case["decoded"]
        );
        assert_eq!((&s.start_key, &s.end_key), (&b"a".to_vec(), &b"b".to_vec()));
        assert!(
            !s.get_cmd(&c.get_cluster().regions.get_region(1).unwrap(), true)
                .is_empty()
        );
        pending.shutdown().await;
    }
    for (from, to) in [(1, 2), (2, 1), (2, 3), (3, 2)] {
        let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let h = Handler::new(Some(&c));
        h.add_merge_region_operator(from, to, context(&mut manager()), || {
            panic!("matching peers need no allocation")
        })
        .await
        .unwrap();
        assert_eq!(h.get_operators().unwrap().len(), 2);
        for (id, passive) in [(from, false), (to, true)] {
            let op = h.get_operator(id).unwrap();
            assert_eq!(op.len(), 1);
            assert_eq!(op.kind(), OpKind::ADMIN);
            assert_eq!(op.status(), STARTED);
            assert_eq!(
                op.priority_level(),
                astersql_pd_core_constant::PriorityLevel::URGENT
            );
            let region = c.get_cluster().regions.get_region(id).unwrap();
            assert_eq!(op.region_epoch(), region.meta().epoch.as_ref());
            let OpStep::MergeRegion(s) = op.step(0).unwrap() else {
                panic!()
            };
            assert_eq!(s.is_passive, passive);
            assert_eq!(
                s.from_region.as_ref(),
                Some(c.get_cluster().regions.get_region(from).unwrap().meta())
            );
            assert_eq!(
                s.to_region.as_ref(),
                Some(c.get_cluster().regions.get_region(to).unwrap().meta())
            );
            assert_eq!(s.get_cmd(&region, true).is_some(), !passive);
            assert_eq!(
                op.additional_info().get(RELATED_MERGE_REGION),
                Some(if passive { from } else { to }.to_string())
            );
        }
        pending.shutdown().await;
    }
    println!("task120 behavior: 5 split + 4 directed merge cases passed");
}
fn altered(r: &RegionInfo, mode: &str) -> Arc<RegionInfo> {
    let mut meta = r.meta().clone();
    let mut down = vec![];
    let mut pending = vec![];
    match mode {
        "pending" => pending.push(meta.peers[1].clone()),
        "down" => down.push(astersql_pd_core::DownPeer {
            peer: meta.peers[1].clone(),
            down_seconds: 0,
        }),
        "under" => {
            meta.peers.pop();
        }
        "over" => meta.peers.push(Peer {
            id: 100,
            store_id: 4,
            ..Default::default()
        }),
        "learner" => meta.peers[2].role = astersql_pd_core::PeerRole::Learner,
        "joint" => meta.peers[2].role = astersql_pd_core::PeerRole::IncomingVoter,
        "mismatch" => meta.peers[2].store_id = 4,
        "first" => meta.key_range.start_key.clear(),
        _ => {}
    }
    Arc::new(RegionInfo::new(
        meta.clone(),
        if mode == "leaderless" {
            None
        } else {
            Some(meta.peers[0].clone())
        },
        down,
        pending,
    ))
}
#[tokio::test]
async fn task_120_validation_and_atomicity() {
    let (c, pending) = setup(
        serde_json::json!({"source":"first"}),
        Arc::new(MemoryStore::default()),
    );
    let h = Handler::new(Some(&c));
    for (from, to, error) in [
        (99, 2, AdminRangeError::RegionNotFound(99)),
        (1, 99, AdminRangeError::RegionNotFound(99)),
        (1, 3, AdminRangeError::NotAdjacent),
        (3, 1, AdminRangeError::NotAdjacent),
        (1, 1, AdminRangeError::NotAdjacent),
    ] {
        assert_eq!(
            h.add_merge_region_operator(from, to, context(&mut manager()), || panic!())
                .await,
            Err(error)
        );
        assert!(h.get_operators().unwrap().is_empty());
    }
    for (policy, keys, error) in [
        (
            "scan ",
            vec![],
            AdminRangeError::UnsupportedPolicy("scan ".into()),
        ),
        ("", vec![], AdminRangeError::UnsupportedPolicy("".into())),
        ("2", vec![], AdminRangeError::UnsupportedPolicy("2".into())),
        (
            "USEKEY",
            vec!["0".into()],
            AdminRangeError::InvalidHexKey("0".into()),
        ),
        (
            "USEKEY",
            vec!["GG".into()],
            AdminRangeError::InvalidHexKey("GG".into()),
        ),
        (
            "USEKEY",
            vec!["é".into()],
            AdminRangeError::InvalidHexKey("é".into()),
        ),
        (
            "USEKEY",
            vec!["00".into(), "0x".into()],
            AdminRangeError::InvalidHexKey("0x".into()),
        ),
    ] {
        assert_eq!(
            h.add_split_region_operator(1, policy, &keys, Instant::now()),
            Err(error)
        );
        assert!(h.get_operators().unwrap().is_empty());
    }
    assert_eq!(
        h.add_split_region_operator(99, "invalid", &[], Instant::now()),
        Err(AdminRangeError::RegionNotFound(99))
    );
    h.add_split_region_operator(2, "SCAN", &[], Instant::now())
        .unwrap();
    let old = h.get_operator(2).unwrap();
    assert_eq!(
        h.add_merge_region_operator(1, 2, context(&mut manager()), || panic!())
            .await,
        Err(AdminRangeError::Admission(AdmissionError::Rejected(
            CancelReason::AlreadyExist
        )))
    );
    assert!(h.get_operator(1).is_err());
    assert!(Arc::ptr_eq(&old, &h.get_operator(2).unwrap()));
    assert_eq!(
        h.add_split_region_operator(2, "SCAN", &[], Instant::now()),
        Err(AdminRangeError::Admission(AdmissionError::Rejected(
            CancelReason::AlreadyExist
        )))
    );
    assert!(Arc::ptr_eq(&old, &h.get_operator(2).unwrap()));
    assert!(h.get_waiting_operators().unwrap().is_empty());
    pending.shutdown().await;
}
#[tokio::test]
async fn task_120_health_joint_and_allocation_fault() {
    for field in ["source", "target"] {
        for mode in ["down", "pending", "under", "over", "learner", "joint"] {
            let (c, pending) = setup(
                serde_json::json!({field:mode}),
                Arc::new(MemoryStore::default()),
            );
            let h = Handler::new(Some(&c));
            let expected = if mode == "joint" {
                AdminRangeError::Builder(BuilderError::OriginJoint)
            } else {
                AdminRangeError::AbnormalPeer(if field == "source" { 1 } else { 2 })
            };
            assert_eq!(
                h.add_merge_region_operator(1, 2, context(&mut manager()), || panic!())
                    .await,
                Err(expected),
                "{field} {mode}"
            );
            assert!(h.get_operators().unwrap().is_empty());
            pending.shutdown().await;
        }
    }
    let (c, pending) = setup(
        serde_json::json!({"source":"joint"}),
        Arc::new(MemoryStore::default()),
    );
    let h = Handler::new(Some(&c));
    assert_eq!(
        h.add_split_region_operator(1, "SCAN", &[], Instant::now()),
        Err(AdminRangeError::Builder(BuilderError::OriginJoint))
    );
    assert!(h.get_operators().unwrap().is_empty());
    pending.shutdown().await;
    let (c, pending) = setup(
        serde_json::json!({"target":"mismatch"}),
        Arc::new(MemoryStore::default()),
    );
    let h = Handler::new(Some(&c));
    assert_eq!(
        h.add_merge_region_operator(1, 2, context(&mut manager()), || Err(
            BuilderError::Allocation("unavailable".into())
        ))
        .await,
        Err(AdminRangeError::Builder(BuilderError::Allocation(
            "unavailable".into()
        )))
    );
    assert!(h.get_operators().unwrap().is_empty());
    h.add_merge_region_operator(1, 2, context(&mut manager()), || Ok(100))
        .await
        .unwrap();
    let op = h.get_operator(1).unwrap();
    let names: Vec<_> = (0..op.len())
        .map(|i| match op.step(i).unwrap() {
            OpStep::AddLearner(s) => {
                assert_eq!((s.to_store, s.peer_id), (4, 100));
                "add"
            }
            OpStep::PromoteLearner(s) => {
                assert_eq!((s.to_store, s.peer_id), (4, 100));
                "promote"
            }
            OpStep::RemovePeer(s) => {
                assert_eq!(s.from_store, 3);
                "remove"
            }
            OpStep::MergeRegion(_) => "merge",
            s => panic!("unexpected {s:?}"),
        })
        .collect();
    assert_eq!(names, ["add", "promote", "remove", "merge"]);
    assert_eq!(h.get_operators().unwrap().len(), 2);
    pending.shutdown().await;
}
#[tokio::test]
async fn task_120_placement_and_epoch() {
    let (c, pending) = setup(
        serde_json::json!({"rules":true}),
        Arc::new(MemoryStore::default()),
    );
    let h = Handler::new(Some(&c));
    let mut rules = manager();
    assert_eq!(
        h.add_merge_region_operator(1, 2, context(&mut rules), || panic!())
            .await,
        Err(AdminRangeError::AbnormalPeer(1))
    );
    assert!(h.get_operators().unwrap().is_empty());
    rules
        .initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    h.add_merge_region_operator(1, 2, context(&mut rules), || panic!())
        .await
        .unwrap();
    assert_eq!(h.get_operators().unwrap().len(), 2);
    h.remove_operator(1, Instant::now()).unwrap();
    assert!(h.get_operators().unwrap().is_empty());
    // The canonical admission boundary rechecks both epochs, rejecting the entire pair.
    let region = c.get_cluster().regions.get_region(1).unwrap();
    let target = c.get_cluster().regions.get_region(2).unwrap();
    let mut stale = target.meta().clone();
    stale.epoch.as_mut().unwrap().version += 1;
    let stale = RegionInfo::new(stale, target.leader().cloned(), vec![], vec![]);
    let ops = RangeOperatorFactory::new(
        "stale",
        BuilderContext {
            stores: &c.get_cluster().stores,
            config: &**c.get_config(),
            rules: &[],
            now_unix_nanos: 0,
        },
        &region,
        false,
    )
    .create_merge(&stale, OpKind::ADMIN, || panic!())
    .await
    .unwrap()
    .map(Arc::new);
    assert_eq!(
        c.get_operator_controller()
            .add_operators(c.get_cluster(), &ops)
            .err(),
        Some(AdmissionError::Rejected(CancelReason::EpochNotMatch))
    );
    assert!(h.get_operators().unwrap().is_empty());
    pending.shutdown().await;
}
#[tokio::test]
async fn task_120_split_preserves_source_validation_boundary() {
    for mode in ["down", "pending", "under", "leaderless"] {
        let (c, pending) = setup(
            serde_json::json!({"source":mode}),
            Arc::new(MemoryStore::default()),
        );
        let region = c.get_cluster().regions.get_region(1).unwrap();
        let h = Handler::new(Some(&c));
        h.add_split_region_operator(1, "SCAN", &[], Instant::now())
            .unwrap();
        assert!(Arc::ptr_eq(
            &region,
            &c.get_cluster().regions.get_region(1).unwrap()
        ));
        assert_eq!(h.get_operator(1).unwrap().status(), STARTED);
        pending.shutdown().await;
    }
}
#[tokio::test]
async fn task_120_unbootstrapped() {
    let h = Handler::<ProviderSnapshot<()>>::new(None);
    assert_eq!(
        h.add_split_region_operator(99, "invalid", &["bad".into()], Instant::now()),
        Err(AdminRangeError::Handler(HandlerError::NotBootstrapped))
    );
    assert_eq!(
        h.add_merge_region_operator(99, 100, context(&mut manager()), || panic!())
            .await,
        Err(AdminRangeError::Handler(HandlerError::NotBootstrapped))
    );
}
