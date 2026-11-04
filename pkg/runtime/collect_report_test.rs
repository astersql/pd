// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    node_test::fixture,
    recovery_start_test::{http, seed},
};
use astersql_pd_proto::{
    eraftpb, metapb,
    pdpb::{self, pd_client::PdClient},
    raft_serverpb,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn report(step: u64, version: u64) -> pdpb::StoreReport {
    pdpb::StoreReport {
        step,
        peer_reports: vec![pdpb::PeerReport {
            raft_state: Some(raft_serverpb::RaftLocalState {
                last_index: 80,
                hard_state: Some(eraftpb::HardState {
                    term: 9,
                    vote: 2,
                    commit: 70,
                }),
            }),
            region_state: Some(raft_serverpb::RegionLocalState {
                region: Some(metapb::Region {
                    id: 777,
                    start_key: b"a".to_vec(),
                    end_key: b"z".to_vec(),
                    region_epoch: Some(metapb::RegionEpoch {
                        version,
                        conf_ver: 3,
                    }),
                    peers: vec![metapb::Peer {
                        id: 22,
                        store_id: 2,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            is_force_leader: true,
            has_commit_merge: true,
            applied_index: 75,
        }],
    }
}
pub(crate) async fn heartbeat(
    client: &mut PdClient<tonic::transport::Channel>,
    cluster: u64,
    store: u64,
    capacity: u64,
    report: Option<pdpb::StoreReport>,
) -> pdpb::StoreHeartbeatResponse {
    let response = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            stats: Some(pdpb::StoreStats {
                store_id: store,
                capacity,
                ..Default::default()
            }),
            store_report: report,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        response.header.as_ref().unwrap().error.is_none(),
        "{response:?}"
    );
    response
}
async fn capacity(
    client: &mut PdClient<tonic::transport::Channel>,
    cluster: u64,
    store: u64,
) -> u64 {
    client
        .get_store(pdpb::GetStoreRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            store_id: store,
        })
        .await
        .unwrap()
        .into_inner()
        .stats
        .unwrap()
        .capacity
}
async fn observe(url: &str) -> (Vec<Value>, u64) {
    let (mut client, cluster, _) = seed(url).await;
    let mut observations = Vec::new();
    let original = capacity(&mut client, cluster, 2).await;
    let idle = heartbeat(&mut client, cluster, 2, 999, Some(report(0, 1))).await;
    assert!(idle.recovery_plan.is_none());
    assert_eq!(capacity(&mut client, cluster, 2).await, original);
    assert_eq!(
        http(
            url,
            "admin/unsafe/remove-failed-stores",
            Some(&json!({"auto-detect":true}))
        )
        .0,
        200
    );
    let first = heartbeat(&mut client, cluster, 2, 12345, None).await;
    let step = first.recovery_plan.unwrap().step;
    assert_ne!(step, 0);
    for (store, provided, version) in [
        (2, None, 0),
        (2, Some(step.wrapping_sub(1)), 2),
        (2, Some(step + 1), 3),
        (2, Some(step), 4),
        (2, Some(step), 5),
        (3, Some(step + 1), 6),
        (3, Some(step), 7),
        (1, None, 0),
    ] {
        let before = capacity(&mut client, cluster, store).await;
        let response = heartbeat(
            &mut client,
            cluster,
            store,
            if provided.is_some() {
                900_000 + version
            } else {
                999
            },
            provided.map(|step| report(step, version)),
        )
        .await;
        if let Some(plan) = &response.recovery_plan {
            assert_eq!(plan.step, step);
            assert!(plan.creates.is_empty());
            assert!(plan.tombstones.is_empty());
            assert!(plan.demotes.is_empty());
            assert!(plan.force_leader.is_none());
        }
        let after = capacity(&mut client, cluster, store).await;
        if provided.is_some() {
            assert_eq!(before, after, "recovery reports must bypass stats");
        }
        observations.push(json!({"store":store,"report":provided.is_some(),"version":version,"requests_report":response.recovery_plan.is_some(),"capacity":after}));
    }
    (observations, step)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_017_current_step_reports_match_go_without_statistics_mutation() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    let binary = pinned_go_binary().unwrap();
    let mut go = Server::start(&binary).unwrap();
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let (go_observed, go_step) = observe(&go.url).await;
    let show = http(&go.url, "admin/unsafe/remove-failed-stores/show", None);
    assert_eq!(
        show.1[0]["info"],
        "Unsafe recovery enters collect report stage"
    );
    assert_eq!(
        show.1.as_array().unwrap().last().unwrap()["info"],
        "Collecting reports from alive stores(2/3)"
    );
    let fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 1700,
                member_id: 17,
                name: "reports".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let (rust_observed, rust_step) = observe(&url).await;
    assert_eq!(rust_observed, go_observed);
    let reports = node.cluster().unsafe_recovery_reports().await.unwrap();
    assert_eq!(reports.values().filter(|r| r.is_some()).count(), 2);
    assert!(reports[&1].is_none());
    let accepted = reports[&2].as_ref().unwrap();
    assert_eq!(accepted.step, rust_step);
    assert_eq!(
        accepted.peers[0]
            .region
            .as_ref()
            .unwrap()
            .epoch
            .unwrap()
            .version,
        5
    );
    assert_eq!(accepted.peers[0].raft.as_ref().unwrap().last_index, 80);
    assert_eq!(
        node.cluster()
            .unsafe_recovery_start_state()
            .await
            .unwrap()
            .0,
        astersql_pd_unsaferecovery::Stage::CollectReport
    );
    node.shutdown().await.unwrap();
    go.stop().unwrap();
    println!(
        "Actual Go recovery step {go_step}, Rust step {rust_step}; 8 matching heartbeat observations; duplicate/stale/missing reports keep CollectReport at 2/3 and latest report replaces prior metadata"
    );
}
