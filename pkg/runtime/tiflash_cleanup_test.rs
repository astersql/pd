// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    collect_report_test::heartbeat,
    node_test::fixture,
    recovery_start_test::{http, seed},
};
use astersql_pd_proto::{
    eraftpb, metapb,
    pdpb::{self, pd_client::PdClient},
    raft_serverpb,
};
use astersql_pd_unsaferecovery::{RecoveryPlan, Stage};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
async fn report_all(
    url: &str,
    flash_fresher: bool,
) -> (
    PdClient<tonic::transport::Channel>,
    u64,
    u64,
    pdpb::StoreHeartbeatResponse,
) {
    let (mut client, cluster, _) = seed(url).await;
    let response = client
        .put_store(pdpb::PutStoreRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 3,
                address: "store-3:20160".into(),
                labels: vec![metapb::StoreLabel {
                    key: "engine".into(),
                    value: "tiflash".into(),
                }],
                node_state: metapb::NodeState::Serving as i32,
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    assert_eq!(
        http(
            url,
            "admin/unsafe/remove-failed-stores",
            Some(&json!({"auto-detect":true}))
        )
        .0,
        200
    );
    let step = heartbeat(&mut client, cluster, 1, 1, None)
        .await
        .recovery_plan
        .unwrap()
        .step;
    let mut completion = None;
    for store in [1, 2, 3] {
        let peers = if store == 2 {
            vec![]
        } else {
            let last = if (store == 3) == flash_fresher {
                12
            } else {
                10
            };
            vec![pdpb::PeerReport {
                raft_state: Some(raft_serverpb::RaftLocalState {
                    last_index: last,
                    hard_state: Some(eraftpb::HardState {
                        term: 1,
                        vote: 0,
                        commit: 10,
                    }),
                }),
                region_state: Some(raft_serverpb::RegionLocalState {
                    region: Some(metapb::Region {
                        id: 7,
                        region_epoch: Some(metapb::RegionEpoch {
                            version: if store == 1 { 10 } else { 9 },
                            conf_ver: 1,
                        }),
                        peers: vec![
                            metapb::Peer {
                                id: 11,
                                store_id: 1,
                                ..Default::default()
                            },
                            metapb::Peer {
                                id: 13,
                                store_id: 3,
                                role: metapb::PeerRole::Learner as i32,
                                ..Default::default()
                            },
                            metapb::Peer {
                                id: 14,
                                store_id: 4,
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                applied_index: 10,
                ..Default::default()
            }]
        };
        completion = Some(
            heartbeat(
                &mut client,
                cluster,
                store,
                999,
                Some(pdpb::StoreReport {
                    step,
                    peer_reports: peers,
                }),
            )
            .await,
        );
    }
    (client, cluster, step, completion.unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_021_native_cleanup_only_targets_selected_tiflash_and_preserves_tikv() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    for flash_fresher in [true, false] {
        let binary = pinned_go_binary().unwrap();
        let mut go = Server::start(&binary).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let (mut client, cluster, step, completion) = report_all(&go.url, flash_fresher).await;
        let mut plans = BTreeMap::from([(3, completion.recovery_plan.unwrap())]);
        for store in [1, 2] {
            plans.insert(
                store,
                heartbeat(&mut client, cluster, store, 1, None)
                    .await
                    .recovery_plan
                    .unwrap(),
            );
        }
        let show = http(&go.url, "admin/unsafe/remove-failed-stores/show", None);
        assert_eq!(
            show.1[1]["info"],
            if flash_fresher {
                "Unsafe recovery enters tombstone TiFlash learner stage"
            } else {
                "Unsafe recovery enters force leader stage"
            }
        );
        assert!(plans.values().all(|p| p.step == step.wrapping_add(1)));
        let fixture = fixture::Fixture::start();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    Arc::new(fixture.store().await),
                    NodeConfig {
                        cluster_id: 2100,
                        member_id: 21,
                        name: "tiflash".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let (_, _, rust_step, _) = report_all(&url, flash_fresher).await;
        let prepared = node
            .cluster()
            .unsafe_recovery_plan()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            prepared.stage,
            if flash_fresher {
                Stage::TombstoneTiFlashLearner
            } else {
                Stage::ForceLeader
            }
        );
        for store in [1, 2, 3] {
            let domain = prepared
                .batch
                .plans
                .get(&store)
                .cloned()
                .unwrap_or_else(RecoveryPlan::default);
            assert_eq!(domain.step, 0);
            let wire = astersql_pd_server::recovery_report_adapter::plan_to_proto(domain);
            let go = &plans[&store];
            assert_eq!(wire.tombstones, go.tombstones);
            assert_eq!(wire.force_leader, go.force_leader);
            assert_eq!(wire.demotes, go.demotes);
            assert_eq!(wire.creates, go.creates);
        }
        if flash_fresher {
            assert_eq!(
                prepared.batch.plans.keys().copied().collect::<Vec<_>>(),
                vec![3]
            );
            assert_eq!(prepared.batch.plans[&3].tombstones, vec![7]);
        } else {
            assert!(
                prepared
                    .batch
                    .plans
                    .values()
                    .all(|p| p.tombstones.is_empty())
            );
        }
        let reports = node.cluster().unsafe_recovery_reports().await.unwrap();
        assert!(
            reports.values().all(Option::is_none),
            "published phase requires fresh confirmations"
        );
        node.shutdown().await.unwrap();
        go.stop().unwrap();
        println!(
            "flash_fresher={flash_fresher}: actual Go collect/phase {step}/{}, Rust collect {rust_step}; three payloads match, TiKV peer retained",
            plans[&3].step
        );
    }
}
