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
async fn task_121_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_121.json"
    ))
    .unwrap();
    for case in fixture.as_array().unwrap() {
        let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let mut m = manager();
        m.initialize(3, &[], "", false, &**c.get_config(), &[])
            .await
            .unwrap();
        let hot = std::collections::BTreeSet::new();
        let cancel = CancellationToken::new();
        let input = &case["input"];
        let mut id = 100;
        let result = handle_operator_creation(
            Some(&mut c),
            input,
            CreationContext {
                range: context(&mut m),
                hot: &hot,
                affinity: None,
                cancellation: &cancel,
            },
            || {
                id += 1;
                Ok(id)
            },
        )
        .await;
        assert_eq!(
            result.status,
            case["status"].as_u64().unwrap() as u16,
            "{case}: {:?}",
            result.error
        );
        if let Some(error) = case["error"].as_str() {
            assert_eq!(result.error.as_ref().unwrap().to_string(), error, "{case}");
        }
        if let Some(desc) = case["desc"].as_str() {
            let op = c.get_operator_controller().get_operator(1).unwrap();
            assert_eq!(op.desc(), desc);
            assert!(op.kind().0 & OpKind::ADMIN.0 != 0);
            assert_eq!(op.status(), STARTED);
            assert_eq!(
                op.priority_level(),
                astersql_pd_core_constant::PriorityLevel::URGENT
            );
            for i in 0..op.len() {
                match op.step(i).unwrap() {
                    OpStep::TransferLeader(s) => assert_eq!((s.from_store, s.to_store), (1, 2)),
                    OpStep::AddLearner(s) => assert_eq!((s.to_store, s.peer_id), (4, 101)),
                    OpStep::PromoteLearner(s) => assert_eq!((s.to_store, s.peer_id), (4, 101)),
                    OpStep::RemovePeer(s) => assert_eq!(
                        s.from_store,
                        if desc == "admin-move-region" { 1 } else { 3 }
                    ),
                    OpStep::SplitRegion(s) => {
                        assert_eq!(s.policy, astersql_pd_schedule_hbstream::SplitPolicy::Scan)
                    }
                    OpStep::MergeRegion(s) => assert_eq!(s.to_region.as_ref().unwrap().id, 2),
                    _ => panic!("unexpected step"),
                }
            }
        }
        if case["body"].is_object() {
            assert_eq!(result.body, case["body"]);
        }
        if result.status == 400 {
            assert!(c.get_operator_controller().operators().is_empty());
        }
        pending.shutdown().await;
    }
}

#[test]
fn task_121_roles_and_numeric_boundaries() {
    use astersql_pd_schedule_placement::PeerRoleType;
    for (roles, expected) in [
        (
            serde_json::json!(["leader", "learner"]),
            vec![(2, PeerRoleType::Leader), (3, PeerRoleType::Learner)],
        ),
        (
            serde_json::json!(["leader"]),
            vec![
                (2, PeerRoleType::Unknown("".into())),
                (3, PeerRoleType::Unknown("".into())),
            ],
        ),
        (
            serde_json::json!([7, "follower"]),
            vec![
                (2, PeerRoleType::Unknown("".into())),
                (3, PeerRoleType::Follower),
            ],
        ),
        (
            serde_json::json!(null),
            vec![
                (2, PeerRoleType::Unknown("".into())),
                (3, PeerRoleType::Unknown("".into())),
            ],
        ),
        (
            serde_json::json!(["other", "voter"]),
            vec![
                (2, PeerRoleType::Unknown("other".into())),
                (3, PeerRoleType::Voter),
            ],
        ),
    ] {
        match OperatorCreation::parse(&serde_json::json!({"name":"transfer-region","region_id":1,"to_store_ids":[2,3],"peer_roles":roles})).unwrap() {
            OperatorCreation::Peer {region, request:AdminPeerRequest::TransferRegion(actual)} => {assert_eq!(region,1);assert_eq!(actual,expected);},
            _=>panic!("wrong dispatch"),
        }
    }
    match OperatorCreation::parse(&serde_json::json!({"name":"transfer-region","region_id":1,"to_store_ids":[2,2],"peer_roles":["leader",7]})).unwrap() {
        OperatorCreation::Peer {request:AdminPeerRequest::TransferRegion(actual),..}=>assert_eq!(actual,vec![(2,PeerRoleType::Leader)]),
        _=>panic!(),
    }
    for input in [
        serde_json::json!({"name":"add-peer","region_id":1,"store_id":0.5}),
        serde_json::json!({"name":"transfer-region","region_id":1,"to_store_ids":[-1]}),
        serde_json::json!({"name":"scatter-regions","region_ids":[1.25]}),
        serde_json::json!({"name":"scatter-regions","region_ids":[],"retry_limit":1e100}),
    ] {
        assert!(matches!(
            OperatorCreation::parse(&input),
            Err(CreationError::Input(_))
        ));
    }
    for (value, expected) in [(0_u64, 0_u64), (9007199254740993, 9007199254740992)] {
        match OperatorCreation::parse(
            &serde_json::json!({"name":"scatter-region","region_id":value,"group":3}),
        )
        .unwrap()
        {
            OperatorCreation::Scatter { region, group } => {
                assert_eq!(region, expected);
                assert_eq!(group, "");
            }
            _ => panic!(),
        }
    }
    for (value, retries) in [
        (serde_json::json!("bad"), 5),
        (serde_json::json!(2.9), 2),
        (serde_json::json!(-1), -1),
    ] {
        match OperatorCreation::parse(
            &serde_json::json!({"name":"scatter-regions","region_ids":[],"retry_limit":value}),
        )
        .unwrap()
        {
            OperatorCreation::ScatterRegions {
                retries: actual, ..
            } => assert_eq!(actual, retries),
            _ => panic!(),
        }
    }
}

#[tokio::test]
async fn task_121_faults_and_response_shapes() {
    let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let mut m = manager();
    m.initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    let hot = std::collections::BTreeSet::from([1]);
    let cancel = CancellationToken::new();
    for (input, status, body) in [
        (
            serde_json::json!({"name":"add-peer","region_id":1,"store_id":4}),
            500,
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"name":"scatter-region","region_id":1}),
            500,
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[1],"retry_limit":0}),
            200,
            serde_json::json!({"processed-percentage":0,"error":""}),
        ),
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[999],"start_key":"61","end_key":"6"}),
            200,
            serde_json::json!({"processed-percentage":0,"error":"encoding/hex: odd length hex string"}),
        ),
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[],"start_key":"ggg","end_key":"63"}),
            200,
            serde_json::json!({"processed-percentage":0,"error":"encoding/hex: invalid byte: U+0067 'g'"}),
        ),
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[],"start_key":"61","end_key":"中"}),
            200,
            serde_json::json!({"processed-percentage":0,"error":"encoding/hex: invalid byte: U+00E4 'ä'"}),
        ),
    ] {
        let response = handle_operator_creation(
            Some(&mut c),
            &input,
            CreationContext {
                range: context(&mut m),
                hot: &hot,
                affinity: None,
                cancellation: &cancel,
            },
            || Err(BuilderError::Allocation("I/O unavailable".into())),
        )
        .await;
        assert_eq!(response.status, status);
        assert_eq!(response.body, body);
        if input["name"] == "add-peer" {
            assert!(matches!(
                response.error,
                Some(CreationError::Peer(AdminPeerError::Builder(
                    BuilderError::Allocation(_)
                )))
            ));
        }
        assert!(c.get_operator_controller().operators().is_empty());
    }
    // Recovery enters the same allocation path and admits the actual target.
    let result = handle_operator_creation(
        Some(&mut c),
        &serde_json::json!({"name":"add-peer","region_id":1,"store_id":4}),
        CreationContext {
            range: context(&mut m),
            hot: &hot,
            affinity: None,
            cancellation: &cancel,
        },
        || Ok(501),
    )
    .await;
    assert_eq!(result.status, 200);
    assert!(
        matches!(c.get_operator_controller().get_operator(1).unwrap().step(0),Some(OpStep::AddLearner(s)) if s.to_store==4 && s.peer_id==501)
    );
    for (input, status) in [
        (serde_json::json!({}), 400),
        (
            serde_json::json!({"name":"split-region","region_id":1,"policy":"SCAN"}),
            500,
        ),
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[]}),
            200,
        ),
    ] {
        let result = handle_operator_creation::<ProviderSnapshot<()>, _>(
            None,
            &input,
            CreationContext {
                range: context(&mut m),
                hot: &hot,
                affinity: None,
                cancellation: &cancel,
            },
            || panic!(),
        )
        .await;
        assert_eq!(result.status, status);
        if status == 500 {
            assert!(matches!(
                result.error,
                Some(CreationError::Handler(HandlerError::NotBootstrapped))
            ));
        }
        if status == 200 {
            assert!(
                result.body["error"]
                    .as_str()
                    .unwrap()
                    .contains("not bootstrapped")
            );
        }
    }
    pending.shutdown().await;
}

#[tokio::test]
async fn task_121_scatter_accounting_and_real_commands() {
    let (mut c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let mut m = manager();
    m.initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    for id in [4, 5] {
        c.get_cluster()
            .stores
            .put_store(Arc::new(StoreInfo::new(StoreMeta {
                id,
                node_state: astersql_pd_core::NodeState::Removed,
                ..Default::default()
            })));
    }
    let hot = std::collections::BTreeSet::new();
    let cancel = CancellationToken::new();
    for (input, body) in [
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[1,999],"retry_limit":0}),
            serde_json::json!({"processed-percentage":50,"error":""}),
        ),
        (
            serde_json::json!({"name":"scatter-regions","region_ids":[999],"start_key":"62","end_key":"63","retry_limit":0}),
            serde_json::json!({"processed-percentage":100,"error":""}),
        ),
        (
            serde_json::json!({"name":"scatter-region","region_id":1,"group":"single"}),
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"name":"scatter-region","region_id":1,"group":"single"}),
            serde_json::Value::Null,
        ),
        (
            serde_json::json!({"name":"scatter-region","region_id":1,"group":"single"}),
            serde_json::Value::Null,
        ),
    ] {
        let result = handle_operator_creation(
            Some(&mut c),
            &input,
            CreationContext {
                range: context(&mut m),
                hot: &hot,
                affinity: None,
                cancellation: &cancel,
            },
            || Ok(600),
        )
        .await;
        assert_eq!(result.status, 200, "{:?}", result.error);
        assert_eq!(result.body, body);
    }
    let op = c.get_operator_controller().get_operator(1).unwrap();
    assert_eq!(op.desc(), "scatter-region");
    assert_eq!(
        op.priority_level(),
        astersql_pd_core_constant::PriorityLevel::HIGH
    );
    assert!(
        matches!(op.step(0),Some(OpStep::TransferLeader(s)) if s.from_store==1 && s.to_store==2)
    );
    pending.shutdown().await;
    let (mut c, pending) = setup(
        serde_json::json!({"source":"under"}),
        Arc::new(MemoryStore::default()),
    );
    let result = handle_operator_creation(
        Some(&mut c),
        &serde_json::json!({"name":"scatter-region","region_id":1}),
        CreationContext {
            range: context(&mut m),
            hot: &hot,
            affinity: None,
            cancellation: &cancel,
        },
        || panic!(),
    )
    .await;
    assert!(matches!(
        result.error,
        Some(CreationError::ScatterFailure(
            astersql_pd_schedule_scatter::ScatterBatchFailure::NotReplicated
        ))
    ));
    assert!(
        c.get_checker_controller()
            .lock()
            .unwrap()
            .pending_processed_regions()
            .contains(&1)
    );
    pending.shutdown().await;
}
