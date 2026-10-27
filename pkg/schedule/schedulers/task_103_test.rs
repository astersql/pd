// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, DownPeer, Peer, PeerRole, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_operator::{OpKind, OpStep};
use astersql_pd_schedule_placement::RuleManager;
use std::sync::Arc;
fn region(case: &serde_json::Value) -> Arc<RegionInfo> {
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            role: if id == 2 && case["learner"] == true {
                PeerRole::Learner
            } else if id == 2 && case["demoting"] == true {
                PeerRole::DemotingVoter
            } else {
                PeerRole::Voter
            },
            is_witness: (id == 1 && case["ordinary"] != true)
                || (id == 2 && case["witness"] == true),
        })
        .collect();
    Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            ..Default::default()
        },
        if case["no_leader"] == true {
            None
        } else {
            Some(peers[0].clone())
        },
        if case["down"] == true {
            vec![DownPeer {
                peer: peers[2].clone(),
                down_seconds: 1000,
            }]
        } else {
            vec![]
        },
        if case["pending"] == true {
            vec![peers[1].clone()]
        } else {
            vec![]
        },
    ))
}
async fn cluster(case: &serde_json::Value) -> BasicCluster {
    let c = BasicCluster::new();
    for id in 1..=3 {
        if id > 1 && case["missing_stores"] == true {
            continue;
        }
        let mut s = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        s.set_last_heartbeat_unix_nanos(if id == 2 && case["disconnected"] == true {
            1
        } else {
            1_000_000_000_000
        });
        if id == 2 && case["offline"] == true {
            s.set_store_state(astersql_pd_core::StoreState::Offline, Some(false));
        }
        if id > 1 && case["budget"] == true {
            let limit = astersql_pd_storelimit::StoreRateLimit::new(0.000000001);
            assert!(
                limit
                    .take(
                        1000,
                        astersql_pd_storelimit::Type::TRANSFER_LEADER_IN,
                        PriorityLevel::URGENT
                    )
                    .await
            );
            s.set_store_limit(Some(astersql_pd_core::StoreLimiter::V1(limit)));
        }
        c.stores.put_store(Arc::new(s));
    }
    c
}
#[tokio::test]
async fn task_103_behavior_cases() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_103.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let c = cluster(case).await;
        let conf = ProviderSnapshot::<u64>::default();
        let mut rules = RuleManager::new(
            Arc::new(astersql_pd_storage_domain::StorageEndpoint::new(Arc::new(
                astersql_pd_storage_domain::MemoryStore::default(),
            ))),
            astersql_pd_keypath::ClusterPaths::new(103),
        );
        let r = region(case);
        let mut s = TransferWitnessLeaderScheduler::new();
        s.recv_region_info().send(r.clone()).unwrap();
        let mut pick = |n| if case["last"] == true { n - 1 } else { 0 };
        let mut shuffle = |_: &mut [usize]| {};
        let mut ctx = EvictLeaderContext {
            cluster: &c,
            config: &conf,
            rules: &mut rules,
            now_unix_nanos: 1_000_000_000_000,
            random_index: &mut pick,
            shuffle: &mut shuffle,
        };
        let out = s.schedule(&mut ctx).await;
        assert_eq!(
            out.operators.len(),
            case["ops"].as_u64().unwrap() as usize,
            "{case}, {:?}",
            out.errors
        );
        assert_eq!(out.errors.len(), usize::from(case["ops"] == 0), "{case}");
        if case["no_leader"] == true {
            assert!(matches!(
                out.errors.as_slice(),
                [TransferWitnessLeaderError::Builder(
                    astersql_pd_schedule_operator::BuilderError::NoLeader
                )]
            ));
        } else if case["witness"] == true || case["demoting"] == true {
            assert!(matches!(
                out.errors.as_slice(),
                [TransferWitnessLeaderError::Builder(
                    astersql_pd_schedule_operator::BuilderError::LeaderNotAllowed
                )]
            ));
        } else if case["ops"] == 0 {
            assert!(matches!(
                out.errors.as_slice(),
                [TransferWitnessLeaderError::NoTarget]
            ));
        }
        if let Some(op) = out.operators.first() {
            assert_eq!(
                op.kind(),
                OpKind(OpKind::LEADER.0 | OpKind::WITNESS_LEADER.0)
            );
            assert_eq!(op.priority_level(), PriorityLevel::URGENT);
            assert_eq!(op.desc(), "transfer-witness-leader-scheduler");
            assert_eq!(op.region_id(), 1);
            assert_eq!(op.len(), 1);
            let OpStep::TransferLeader(step) = op.step(0).unwrap() else {
                panic!()
            };
            assert_eq!(step.from_store, 1);
            assert_eq!(step.to_store, case["target"].as_u64().unwrap());
            assert_eq!(
                step.to_stores,
                serde_json::from_value::<Vec<u64>>(case["targets"].clone()).unwrap()
            );
            assert!(!step.is_finish(&r));
            assert_eq!(step.conf_ver_changed(&r), 0);
            let cmd = step.get_cmd(&r, true).transfer_leader.unwrap();
            assert_eq!(cmd.peer.unwrap().store_id, step.to_store);
            assert_eq!(
                cmd.peers
                    .iter()
                    .map(|p| p.as_ref().unwrap().store_id)
                    .collect::<Vec<_>>(),
                step.to_stores
            );
            let missing = BasicCluster::new();
            assert!(
                step.check_in_progress(&missing, &conf, &r, true, 1_000_000_000_000)
                    .is_err()
            );
            assert!(
                step.check_in_progress(&c, &conf, &r, true, 1_000_000_000_000)
                    .is_ok()
            );
            assert!(op.start());
            assert!(op.check(&r).is_some());
            for id in &step.to_stores {
                let ack = RegionInfo::new(
                    r.meta().clone(),
                    r.get_store_peer(*id).cloned(),
                    vec![],
                    vec![],
                );
                assert!(step.is_finish(&ack));
            }
            let ack = RegionInfo::new(
                r.meta().clone(),
                r.get_store_peer(step.to_store).cloned(),
                vec![],
                vec![],
            );
            assert!(op.check(&ack).is_none());
            assert!(op.check_success());
            assert_eq!(r.leader().unwrap().store_id, 1);
        }
        assert!(s.schedule(&mut ctx).await.operators.is_empty());
    }
    println!(
        "task_103: {} source-derived behavior cases passed",
        f["cases"].as_array().unwrap().len()
    );
}
#[tokio::test]
async fn task_103_queue_batch_and_faults() {
    let c = cluster(&serde_json::json!({})).await;
    let conf = ProviderSnapshot::<u64>::default();
    let mut rules = RuleManager::new(
        Arc::new(astersql_pd_storage_domain::StorageEndpoint::new(Arc::new(
            astersql_pd_storage_domain::MemoryStore::default(),
        ))),
        astersql_pd_keypath::ClusterPaths::new(103),
    );
    let mut pick = |_| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut ctx = EvictLeaderContext {
        cluster: &c,
        config: &conf,
        rules: &mut rules,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut pick,
        shuffle: &mut shuffle,
    };
    let mut s = TransferWitnessLeaderScheduler::new();
    assert!(s.is_schedule_allowed());
    let sender = s.recv_region_info();
    let r = region(&serde_json::json!({}));
    for _ in 0..4 {
        sender.send(r.clone()).unwrap();
    }
    assert_eq!(s.schedule(&mut ctx).await.operators.len(), 3);
    assert_eq!(s.schedule(&mut ctx).await.operators.len(), 1);
    assert!(s.schedule(&mut ctx).await.operators.is_empty());
    // A rejected event consumes a batch position; it must not prevent later work.
    sender
        .send(region(&serde_json::json!({"pending":true,"down":true})))
        .unwrap();
    for _ in 0..3 {
        sender.send(r.clone()).unwrap();
    }
    let out = s.schedule(&mut ctx).await;
    assert_eq!(out.errors.len(), 1);
    assert_eq!(out.operators.len(), 2);
    assert!(matches!(
        &out.errors[0],
        TransferWitnessLeaderError::NoTarget
    ));
    assert_eq!(s.schedule(&mut ctx).await.operators.len(), 1);
    // Healthy heartbeat snapshots are queued, but target stores are evaluated at consumption.
    sender.send(r.clone()).unwrap();
    let gone = BasicCluster::new();
    ctx.cluster = &gone;
    assert!(matches!(
        s.schedule(&mut ctx).await.errors.as_slice(),
        [TransferWitnessLeaderError::NoTarget]
    ));
    ctx.cluster = &c;
    assert!(s.schedule(&mut ctx).await.operators.is_empty());
    for _ in 0..10_000 {
        sender.try_send(r.clone()).unwrap();
    }
    assert!(matches!(
        sender.try_send(r.clone()),
        Err(std::sync::mpsc::TrySendError::Full(_))
    ));
    assert_eq!(s.schedule(&mut ctx).await.operators.len(), 3);
    sender.try_send(r).unwrap();
    drop(s);
    assert!(matches!(
        sender.try_send(region(&serde_json::json!({}))),
        Err(std::sync::mpsc::TrySendError::Disconnected(_))
    ));
}
