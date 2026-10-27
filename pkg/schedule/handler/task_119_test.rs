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
    for id in 2..=4 {
        cluster.regions.set_region(Arc::new(RegionInfo::new(
            RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![id as u8],
                    end_key: vec![id as u8 + 1],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )));
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
use astersql_pd_schedule_placement::PeerRoleType;
use std::time::Instant;
fn context() -> AdminPeerContext<'static> {
    AdminPeerContext {
        rules: &[],
        now_unix_nanos: 0,
        supports_conf_change_v2: false,
        now: Instant::now(),
    }
}
fn step_names(op: &Operator) -> Vec<&'static str> {
    (0..op.len())
        .map(|i| match op.step(i).unwrap() {
            OpStep::TransferLeader(_) => "transfer",
            OpStep::AddLearner(_) => "add",
            OpStep::PromoteLearner(_) => "promote",
            OpStep::RemovePeer(_) => "remove",
            _ => "other",
        })
        .collect()
}
#[tokio::test]
async fn task_119_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_119.json"
    ))
    .unwrap();
    for case in fixture["success"].as_array().unwrap() {
        let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let h = Handler::new(Some(&c));
        let request = match case["action"].as_str().unwrap() {
            "leader" => AdminPeerRequest::TransferLeader(2),
            "region" => AdminPeerRequest::TransferRegion(vec![
                (1, PeerRoleType::Leader),
                (2, PeerRoleType::Voter),
                (4, PeerRoleType::default()),
            ]),
            "peer" => AdminPeerRequest::TransferPeer { from: 3, to: 4 },
            "add" => AdminPeerRequest::AddPeer(4),
            "learner" => AdminPeerRequest::AddLearner(4),
            "remove" => AdminPeerRequest::RemovePeer(3),
            _ => unreachable!(),
        };
        h.add_peer_operator(1, request, context(), || Ok(100))
            .await
            .unwrap();
        let op = h.get_operator(1).unwrap();
        let expected: Vec<_> = case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(step_names(&op), expected, "{case}");
        assert_ne!(op.kind().0 & OpKind::ADMIN.0, 0);
        assert_eq!(
            op.priority_level(),
            astersql_pd_core_constant::PriorityLevel::URGENT
        );
        assert_eq!(op.status(), STARTED);
        for i in 0..op.len() {
            match op.step(i).unwrap() {
                OpStep::TransferLeader(s) => assert_eq!((s.from_store, s.to_store), (1, 2)),
                OpStep::AddLearner(s) => assert_eq!((s.to_store, s.peer_id), (4, 100)),
                OpStep::PromoteLearner(s) => assert_eq!((s.to_store, s.peer_id), (4, 100)),
                OpStep::RemovePeer(s) => assert_eq!(s.from_store, 3),
                _ => panic!("unexpected step"),
            }
        }
        assert!(h.get_waiting_operators().unwrap().is_empty());
        pending.shutdown().await;
    }
}
#[tokio::test]
async fn task_119_validation_and_faults() {
    let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
    let h = Handler::new(Some(&c));
    let cases = [
        (
            99,
            AdminPeerRequest::AddPeer(4),
            AdminPeerError::RegionNotFound(99),
        ),
        (
            1,
            AdminPeerRequest::TransferLeader(4),
            AdminPeerError::NoVoter(4),
        ),
        (
            1,
            AdminPeerRequest::TransferPeer { from: 9, to: 4 },
            AdminPeerError::NoPeer(9),
        ),
        (
            1,
            AdminPeerRequest::AddPeer(2),
            AdminPeerError::ExistingPeer(2),
        ),
        (
            1,
            AdminPeerRequest::AddLearner(2),
            AdminPeerError::ExistingPeer(2),
        ),
        (
            1,
            AdminPeerRequest::AddPeer(9),
            AdminPeerError::StoreNotFound(9),
        ),
        (
            1,
            AdminPeerRequest::RemovePeer(9),
            AdminPeerError::NoPeer(9),
        ),
    ];
    for (id, request, expected) in cases {
        assert_eq!(
            h.add_peer_operator(id, request, context(), || panic!(
                "validation must precede allocation"
            ))
            .await,
            Err(expected)
        );
        assert!(h.get_operators().unwrap().is_empty());
    }
    assert_eq!(
        h.add_peer_operator(1, AdminPeerRequest::AddPeer(4), context(), || Err(
            BuilderError::Allocation("unavailable".into())
        ))
        .await,
        Err(AdminPeerError::Builder(BuilderError::Allocation(
            "unavailable".into()
        )))
    );
    c.get_cluster()
        .stores
        .put_store(Arc::new(StoreInfo::new(StoreMeta {
            id: 4,
            node_state: astersql_pd_core::NodeState::Removed,
            ..Default::default()
        })));
    assert_eq!(
        h.add_peer_operator(1, AdminPeerRequest::AddPeer(4), context(), || Ok(100))
            .await,
        Err(AdminPeerError::StoreRemoved(4))
    );
    let mut clock = context();
    clock.now_unix_nanos = 601_000_000_000;
    assert_eq!(
        h.add_peer_operator(1, AdminPeerRequest::AddLearner(5), clock, || Ok(100))
            .await,
        Err(AdminPeerError::StoreUnhealthy(5))
    );
    assert!(h.get_operators().unwrap().is_empty());
    h.add_peer_operator(1, AdminPeerRequest::TransferLeader(2), context(), || {
        Ok(100)
    })
    .await
    .unwrap();
    let original = h.get_operator(1).unwrap();
    assert!(matches!(
        h.add_peer_operator(1, AdminPeerRequest::TransferLeader(3), context(), || Ok(
            100
        ))
        .await,
        Err(AdminPeerError::Admission(_))
    ));
    assert!(Arc::ptr_eq(&h.get_operator(1).unwrap(), &original));
    assert!(h.get_waiting_operators().unwrap().is_empty());
    for s in ["", "-1", " 1", "1_0", "18446744073709551616", "++1", "+1"] {
        assert_eq!(
            h.pre_check_for_region(s).err(),
            Some(AdminPeerError::InvalidRegionId)
        );
    }
    assert_eq!(
        h.pre_check_for_region("99").err(),
        Some(AdminPeerError::RegionNotFound(99))
    );
    for s in ["1", "01"] {
        assert_eq!(h.pre_check_for_region(s).unwrap().meta().id, 1);
    }
    pending.shutdown().await;
}
#[tokio::test]
async fn task_119_unbootstrapped() {
    let h = Handler::<ProviderSnapshot<()>>::new(None);
    assert_eq!(
        h.pre_check_for_region("invalid").err(),
        Some(AdminPeerError::Handler(HandlerError::NotBootstrapped))
    );
    for r in [
        AdminPeerRequest::TransferLeader(2),
        AdminPeerRequest::TransferRegion(vec![]),
        AdminPeerRequest::TransferPeer { from: 1, to: 4 },
        AdminPeerRequest::AddPeer(4),
        AdminPeerRequest::AddLearner(4),
        AdminPeerRequest::RemovePeer(1),
    ] {
        assert_eq!(
            h.add_peer_operator(1, r, context(), || Ok(100)).await,
            Err(AdminPeerError::Handler(HandlerError::NotBootstrapped))
        );
    }
}
#[tokio::test]
async fn task_119_roles() {
    for (roles, error) in [
        (vec![], BuilderError::NoExpectedVoter),
        (
            vec![(1, PeerRoleType::Learner)],
            BuilderError::NoExpectedVoter,
        ),
        (
            vec![(1, PeerRoleType::Unknown("invalid".into()))],
            BuilderError::NoExpectedVoter,
        ),
        (
            vec![(1, PeerRoleType::Leader), (2, PeerRoleType::Leader)],
            BuilderError::MultipleLeaders,
        ),
    ] {
        let (c, pending) = setup(serde_json::json!({}), Arc::new(MemoryStore::default()));
        let h = Handler::new(Some(&c));
        assert_eq!(
            h.add_peer_operator(
                1,
                AdminPeerRequest::TransferRegion(roles),
                context(),
                || Ok(100)
            )
            .await,
            Err(AdminPeerError::Builder(error))
        );
        assert!(h.get_operators().unwrap().is_empty());
        pending.shutdown().await;
    }
    let (c, pending) = setup(
        serde_json::json!({"rules":true}),
        Arc::new(MemoryStore::default()),
    );
    let h = Handler::new(Some(&c));
    assert_eq!(
        h.add_peer_operator(
            1,
            AdminPeerRequest::TransferRegion(vec![(4, PeerRoleType::default())]),
            context(),
            || Ok(100)
        )
        .await,
        Err(AdminPeerError::MissingRole)
    );
    // Explicit roles reach actual placement fitting: absent rules must fail closed.
    assert_eq!(
        h.add_peer_operator(
            1,
            AdminPeerRequest::TransferRegion(vec![(4, PeerRoleType::Voter)]),
            context(),
            || Ok(100)
        )
        .await,
        Err(AdminPeerError::Builder(BuilderError::NoPlacementRule))
    );
    assert!(h.get_operators().unwrap().is_empty());
    pending.shutdown().await;
}
#[tokio::test]
async fn task_119_preserves_peer_roles_and_placement() {
    for (learner, witness) in [(true, false), (false, true)] {
        let (c, pending) = setup(
            serde_json::json!({"learner":learner,"witness":witness}),
            Arc::new(MemoryStore::default()),
        );
        let h = Handler::new(Some(&c));
        assert_eq!(
            h.add_peer_operator(1, AdminPeerRequest::TransferLeader(3), context(), || Ok(
                100
            ))
            .await
            .err(),
            Some(if learner {
                AdminPeerError::NoVoter(3)
            } else {
                AdminPeerError::Builder(BuilderError::LeaderNotAllowed)
            })
        );
        h.add_peer_operator(
            1,
            AdminPeerRequest::TransferPeer { from: 3, to: 4 },
            context(),
            || Ok(100),
        )
        .await
        .unwrap();
        let op = h.get_operator(1).unwrap();
        assert_eq!(
            step_names(&op),
            if learner {
                vec!["add", "remove"]
            } else {
                vec!["add", "promote", "remove"]
            }
        );
        match op.step(0).unwrap() {
            OpStep::AddLearner(s) => {
                assert_eq!((s.to_store, s.peer_id, s.is_witness), (4, 100, witness))
            }
            _ => unreachable!(),
        }
        pending.shutdown().await;
    }
    // Freeze the applicable rules through the real RuleManager + MemoryStore.
    let backend = Arc::new(MemoryStore::default());
    let (c, pending) = setup(serde_json::json!({"rules":true}), backend.clone());
    let mut manager = astersql_pd_schedule_placement::RuleManager::new(
        Arc::new(StorageEndpoint::new(backend)),
        ClusterPaths::new(119),
    );
    manager
        .initialize(3, &[], "", false, &**c.get_config(), &[])
        .await
        .unwrap();
    let region = c.get_cluster().regions.get_region(1).unwrap();
    let rules = manager.get_rules_for_apply_region(&region);
    assert_eq!(rules.len(), 1);
    let mut input = context();
    input.rules = &rules;
    let h = Handler::new(Some(&c));
    h.add_peer_operator(
        1,
        AdminPeerRequest::TransferRegion(vec![
            (1, PeerRoleType::Leader),
            (2, PeerRoleType::Follower),
            (4, PeerRoleType::Voter),
        ]),
        input,
        || Ok(100),
    )
    .await
    .unwrap();
    assert_eq!(
        step_names(&h.get_operator(1).unwrap()),
        ["add", "promote", "remove"]
    );
    pending.shutdown().await;
}
