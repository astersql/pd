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
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn peer(
    id: u64,
    start: &[u8],
    end: &[u8],
    version: u64,
    conf: u64,
    term: u64,
    last: u64,
    stores: &[u64],
) -> pdpb::PeerReport {
    pdpb::PeerReport {
        raft_state: Some(raft_serverpb::RaftLocalState {
            last_index: last,
            hard_state: Some(eraftpb::HardState {
                term,
                vote: 0,
                commit: last,
            }),
        }),
        region_state: Some(raft_serverpb::RegionLocalState {
            region: Some(metapb::Region {
                id,
                start_key: start.to_vec(),
                end_key: end.to_vec(),
                region_epoch: Some(metapb::RegionEpoch {
                    version,
                    conf_ver: conf,
                }),
                peers: stores
                    .iter()
                    .map(|store| metapb::Peer {
                        id: id + store,
                        store_id: *store,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        applied_index: last,
        ..Default::default()
    }
}
async fn report_all(url: &str) -> (PdClient<tonic::transport::Channel>, u64, u64) {
    let (mut client, cluster, _) = seed(url).await;
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
    for (store, peers) in [
        (
            1,
            vec![
                peer(700, b"", b"m", 10, 1, 1, 10, &[1, 2, 4, 5, 6]),
                peer(800, b"a", b"", 9, 100, 9, 99, &[1, 4, 5]),
                peer(900, b"m", b"", 8, 1, 1, 10, &[1, 4, 5]),
            ],
        ),
        (2, vec![peer(700, b"", b"m", 10, 1, 2, 9, &[1, 2, 4, 5, 6])]),
        (3, vec![]),
    ] {
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
        .await;
    }
    (client, cluster, step)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_018_native_index_matches_real_go_epoch_and_raft_selection() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    let binary = pinned_go_binary().unwrap();
    let mut go = Server::start(&binary).unwrap();
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let (mut client, go_cluster, go_step) = report_all(&go.url).await;
    let mut expected = BTreeMap::new();
    let mut go_plans = Vec::new();
    for store in [1, 2] {
        let response = heartbeat(&mut client, go_cluster, store, 1, None).await;
        let plan = response
            .recovery_plan
            .expect("Go must enter actual force-leader phase");
        assert_ne!(plan.step, go_step);
        let force = plan
            .force_leader
            .as_ref()
            .expect("lost quorum requires force leader");
        for id in &force.enter_force_leaders {
            expected.insert(*id, store);
        }
        go_plans.push((store, plan.step, force.enter_force_leaders.clone()));
    }
    assert_eq!(expected, BTreeMap::from([(700, 2), (900, 1)]));
    let fixture = fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 1800,
                member_id: 18,
                name: "index".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let (_, _, rust_step) = report_all(&url).await;
    let index = node
        .cluster()
        .unsafe_recovery_index()
        .await
        .unwrap()
        .expect("real report collection must publish a recovery index");
    assert_eq!(
        index
            .regions
            .items()
            .map(|i| i.region().id)
            .collect::<Vec<_>>(),
        vec![700, 900]
    );
    assert_eq!(index.peers.len(), 3);
    assert!(!index.regions.contains(800));
    let stores = astersql_pd_core::StoresInfo::new();
    for id in [1, 2, 3] {
        stores.put_store(
            astersql_pd_server::ClusterReadService::store(node.cluster().as_ref(), id)
                .await
                .unwrap()
                .unwrap(),
        );
    }
    let selected = index
        .regions
        .items()
        .map(|r| {
            (
                r.region().id,
                index
                    .select_leader(r.region().id, &stores)
                    .unwrap()
                    .unwrap()
                    .store_id,
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(selected, expected);
    node.shutdown().await.unwrap();
    go.stop().unwrap();
    println!(
        "Go report step {go_step}, actual force plans {go_plans:?}; Rust report step {rust_step}, canonical recovery index leaders {selected:?}; older overlapping epoch excluded without mutating peer reports"
    );
}
