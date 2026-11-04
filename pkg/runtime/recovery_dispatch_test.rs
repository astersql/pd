// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    collect_report_test::heartbeat,
    node_test::fixture,
    recovery_start_test::{http, seed},
};
use astersql_pd_proto::{metapb, pdpb, raft_serverpb};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn normalized_show(mut value: Value) -> Value {
    for event in value.as_array_mut().unwrap() {
        // Wall-clock timestamps and Go map iteration order are explicitly nondeterministic.
        if let Some(time) = event.get_mut("time") {
            assert_eq!(time.as_str().unwrap().len(), 23);
            *time = json!("wall-clock");
        }
        if let Some(details) = event.get_mut("details").and_then(Value::as_array_mut) {
            for detail in details {
                let text = detail.as_str().unwrap();
                if text.starts_with("Stores that have ") {
                    let (label, ids) = text.split_once(": ").unwrap();
                    let mut ids = ids.split(", ").collect::<Vec<_>>();
                    ids.sort();
                    *detail = json!(format!("{label}: {}", ids.join(", ")));
                }
            }
        }
    }
    value
}
fn normalized_frame(
    mut response: pdpb::StoreHeartbeatResponse,
    cluster: u64,
    base: u64,
) -> pdpb::StoreHeartbeatResponse {
    assert_eq!(response.header.as_ref().unwrap().cluster_id, cluster);
    assert!(response.header.as_ref().unwrap().error.is_none());
    response.header.as_mut().unwrap().cluster_id = 0; // separately verified run identity
    if let Some(plan) = &mut response.recovery_plan {
        assert!(plan.step >= base);
        plan.step -= base;
    }
    response
}
fn apply(peers: &mut Vec<pdpb::PeerReport>, plan: &pdpb::RecoveryPlan) {
    peers.retain(|p| {
        !plan
            .tombstones
            .contains(&p.region_state.as_ref().unwrap().region.as_ref().unwrap().id)
    });
    for region in &plan.creates {
        peers.push(pdpb::PeerReport {
            region_state: Some(raft_serverpb::RegionLocalState {
                region: Some(region.clone()),
                ..Default::default()
            }),
            ..Default::default()
        });
    }
    if let Some(force) = &plan.force_leader {
        for report in peers.iter_mut() {
            let region = report
                .region_state
                .as_ref()
                .unwrap()
                .region
                .as_ref()
                .unwrap();
            if force.enter_force_leaders.contains(&region.id) {
                report.is_force_leader = true;
            }
        }
    }
    for demote in &plan.demotes {
        for report in peers.iter_mut() {
            let region = report
                .region_state
                .as_mut()
                .unwrap()
                .region
                .as_mut()
                .unwrap();
            if region.id == demote.region_id {
                region
                    .peers
                    .retain(|p| !demote.failed_voters.iter().any(|f| f.id == p.id));
                region.region_epoch.as_mut().unwrap().conf_ver += 1;
            }
        }
    }
    if plan.force_leader.is_none()
        && plan.demotes.is_empty()
        && plan.creates.is_empty()
        && plan.tombstones.is_empty()
    {
        for report in peers {
            report.is_force_leader = false;
        }
    }
}
async fn observe(url: &str, lost_quorum: bool) -> (Vec<pdpb::StoreHeartbeatResponse>, Vec<Value>) {
    let (mut client, cluster, seed) = seed(url).await;
    let cached = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            region_id: 100,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        cached.region.is_some(),
        "bootstrap seeds a real cached region"
    );
    assert_eq!(
        http(url, "admin/unsafe/remove-failed-stores/show", None).1,
        json!([{"info":"No on-going recovery."}])
    );
    assert_eq!(
        http(
            url,
            "admin/unsafe/remove-failed-stores",
            Some(&json!({"auto-detect":true,"plan-execution-timeout":1}))
        )
        .0,
        200
    );
    let first = heartbeat(&mut client, cluster, 1, 101, None).await;
    let base = first.recovery_plan.as_ref().unwrap().step;
    let mut frames = vec![normalized_frame(first, cluster, base)];
    for store in [2, 3] {
        frames.push(normalized_frame(
            heartbeat(&mut client, cluster, store, 101, None).await,
            cluster,
            base,
        ));
    }
    let mut shows = vec![normalized_show(
        http(url, "admin/unsafe/remove-failed-stores/show", None).1,
    )];
    let mut region = seed;
    region.region_epoch = Some(metapb::RegionEpoch {
        version: 1,
        conf_ver: 1,
    });
    if lost_quorum {
        region
            .peers
            .extend([4, 5].into_iter().map(|store_id| metapb::Peer {
                id: store_id + 10,
                store_id,
                ..Default::default()
            }));
    }
    let mut peers: BTreeMap<u64, Vec<pdpb::PeerReport>> = [
        (
            1,
            vec![pdpb::PeerReport {
                region_state: Some(raft_serverpb::RegionLocalState {
                    region: Some(region),
                    ..Default::default()
                }),
                ..Default::default()
            }],
        ),
        (2, vec![]),
        (3, vec![]),
    ]
    .into();
    let mut step = base;
    for round in 0..6 {
        let mut plans = BTreeMap::new();
        for store in 1..=3 {
            let response = heartbeat(
                &mut client,
                cluster,
                store,
                999,
                Some(pdpb::StoreReport {
                    step,
                    peer_reports: peers[&store].clone(),
                }),
            )
            .await;
            assert_eq!(
                response.state, 0,
                "recovery bypasses the normal heartbeat state mutation"
            );
            if let Some(plan) = &response.recovery_plan {
                plans.insert(store, plan.clone());
            }
            frames.push(normalized_frame(response, cluster, base));
        }
        shows.push(normalized_show(
            http(url, "admin/unsafe/remove-failed-stores/show", None).1,
        ));
        if !lost_quorum || round == 3 {
            assert_eq!(
                shows.last().unwrap().as_array().unwrap().last().unwrap()["info"],
                "Unsafe recovery Finished"
            );
            let stable =
                normalized_show(http(url, "admin/unsafe/remove-failed-stores/show", None).1);
            assert_eq!(shows.last(), Some(&stable));
            let cached = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster,
                        ..Default::default()
                    }),
                    region_id: 100,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                cached.region.is_none(),
                lost_quorum,
                "only executed recovery phases invalidate the canonical region cache"
            );
            break;
        }
        for store in 1..=2 {
            let response = heartbeat(&mut client, cluster, store, 101, None).await;
            plans.insert(
                store,
                response
                    .recovery_plan
                    .clone()
                    .expect("new phase must dispatch"),
            );
            frames.push(normalized_frame(response, cluster, base));
        }
        assert_eq!(plans.len(), 3);
        assert!(plans.values().all(|p| p.step == step + 1));
        let next = step + 1;
        let duplicate = heartbeat(&mut client, cluster, 1, 101, None).await;
        assert!(duplicate.recovery_plan.is_none());
        frames.push(normalized_frame(duplicate, cluster, base));
        let stale = heartbeat(
            &mut client,
            cluster,
            1,
            999,
            Some(pdpb::StoreReport {
                step,
                peer_reports: peers[&1].clone(),
            }),
        )
        .await;
        assert!(stale.recovery_plan.is_none());
        frames.push(normalized_frame(stale, cluster, base));
        if round == 0 {
            tokio::time::sleep(Duration::from_millis(1100)).await;
            let retry = heartbeat(&mut client, cluster, 1, 101, None).await;
            assert_eq!(retry.recovery_plan.as_ref(), plans.get(&1));
            frames.push(normalized_frame(retry, cluster, base));
        }
        println!(
            "lost={lost_quorum},round={round},actual collect={base},phase={next},plans={plans:?}"
        );
        for (store, plan) in plans {
            apply(peers.get_mut(&store).unwrap(), &plan);
        }
        step = next;
        assert!(round < 5, "must finish bounded protocol rounds");
    }
    println!(
        "lost={lost_quorum},cluster={cluster},collect={base},frames={frames:?},show={shows:?}"
    );
    (frames, shows)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_023_native_multi_round_dispatch_retry_show_and_schedule_resume_match_go() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    for lost in [false, true] {
        let mut go = Server::start(&pinned_go_binary().unwrap()).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let expected = observe(&go.url, lost).await;
        let fixture = fixture::Fixture::start();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    Arc::new(fixture.store().await),
                    NodeConfig {
                        cluster_id: 2300,
                        member_id: 23,
                        name: "dispatch".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let observed = observe(&url, lost).await;
        assert_eq!(
            observed.0, expected.0,
            "lost={lost}: complete heartbeat frames; verified identity and step offsets recorded separately"
        );
        assert_eq!(
            observed.1, expected.1,
            "lost={lost}: complete Show history and status"
        );
        let (stage, halt, _, _) = node.cluster().unsafe_recovery_start_state().await.unwrap();
        assert_eq!(stage, astersql_pd_unsaferecovery::Stage::Finished);
        assert!(!halt);
        node.shutdown().await.unwrap();
        go.stop().unwrap();
    }
}
