// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    collect_report_test::heartbeat,
    node_test::fixture,
    recovery_start_test::{http, seed},
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
    raft_serverpb,
};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_unsaferecovery::{RecoveryPlan, Stage};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Copy, Debug)]
enum Mode {
    Holes,
    Whole,
    Contiguous,
    Overlap,
    UncheckedOverlap,
    NoTiKv,
}
fn region(id: u64, start: &[u8], end: &[u8], version: u64) -> pdpb::PeerReport {
    pdpb::PeerReport {
        region_state: Some(raft_serverpb::RegionLocalState {
            region: Some(metapb::Region {
                id,
                start_key: start.to_vec(),
                end_key: end.to_vec(),
                region_epoch: Some(metapb::RegionEpoch {
                    version,
                    conf_ver: 1,
                }),
                peers: vec![metapb::Peer {
                    id: 100 + id,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}
async fn report_all(
    url: &str,
    mode: Mode,
) -> (
    PdClient<tonic::transport::Channel>,
    u64,
    u64,
    pdpb::StoreHeartbeatResponse,
) {
    let (mut client, cluster, _) = seed(url).await;
    for store in 1..=3 {
        if store == 1 && !matches!(mode, Mode::NoTiKv) {
            continue;
        }
        let response = client
            .put_store(pdpb::PutStoreRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: cluster,
                    ..Default::default()
                }),
                store: Some(metapb::Store {
                    id: store,
                    address: format!("store-{store}:20160"),
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
    }
    assert_eq!(http(url,"admin/unsafe/remove-failed-stores",Some(&json!({"auto-detect":true,"disable-paranoid-check":matches!(mode,Mode::UncheckedOverlap)}))).0,200);
    let step = heartbeat(&mut client, cluster, 1, 1, None)
        .await
        .recovery_plan
        .unwrap()
        .step;
    let mut completed = None;
    for store in 1..=3 {
        let peers = if store != 1 {
            vec![]
        } else {
            match mode {
                Mode::Whole | Mode::NoTiKv => vec![],
                Mode::Contiguous => vec![region(7, b"", b"b", 10), region(9, b"b", b"", 9)],
                Mode::Holes => vec![region(7, b"b", b"d", 10), region(9, b"f", b"h", 9)],
                Mode::Overlap | Mode::UncheckedOverlap => vec![
                    region(7, b"b", b"d", 10),
                    region(9, b"f", b"h", 9),
                    region(8, b"a", b"i", 8),
                ],
            }
        };
        let response = client
            .store_heartbeat(pdpb::StoreHeartbeatRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: cluster,
                    ..Default::default()
                }),
                stats: Some(pdpb::StoreStats {
                    store_id: store,
                    capacity: 999,
                    ..Default::default()
                }),
                store_report: Some(pdpb::StoreReport {
                    step,
                    peer_reports: peers,
                }),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        if store < 3 {
            assert!(response.header.as_ref().unwrap().error.is_none());
        }
        completed = Some(response);
    }
    (client, cluster, step, completed.unwrap())
}
async fn boundary(store: &dyn MetaStore, cluster: u64) -> (Option<u64>, i64, Option<i64>) {
    let read = store
        .read(Read::range(
            ClusterPaths::new(cluster).alloc_id_path().as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    (
        read.entries
            .first()
            .map(|e| u64::from_be_bytes(e.value.as_slice().try_into().unwrap())),
        read.revision,
        read.entries.first().map(|e| e.mod_revision),
    )
}
async fn next_id(client: &mut PdClient<tonic::transport::Channel>, cluster: u64) -> u64 {
    client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_022_native_empty_region_plans_and_real_allocator_effects_match_go() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs022-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(root.clone());
    let config = root.join("pd.toml");
    std::fs::write(&config, "[schedule]\nhalt-scheduling=true\n").unwrap();
    for mode in [
        Mode::Holes,
        Mode::Whole,
        Mode::Contiguous,
        Mode::Overlap,
        Mode::UncheckedOverlap,
        Mode::NoTiKv,
    ] {
        let binary = pinned_go_binary().unwrap();
        let mut go =
            Server::start_with_args(&binary, &[&format!("--config={}", config.display())]).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let (mut go_client, go_cluster, go_step, go_completion) = report_all(&go.url, mode).await;
        assert!(go_completion.header.as_ref().unwrap().error.is_none());
        let show = http(&go.url, "admin/unsafe/remove-failed-stores/show", None);
        let blocked = matches!(mode, Mode::Overlap | Mode::NoTiKv);
        let no_work = matches!(mode, Mode::Contiguous);
        let mut plans = BTreeMap::new();
        let mut go_frames = BTreeMap::new();
        if !no_work {
            plans.insert(3, go_completion.recovery_plan.clone().unwrap());
            for store in [1, 2] {
                let response = heartbeat(&mut go_client, go_cluster, store, 1, None).await;
                plans.insert(store, response.recovery_plan.clone().unwrap());
                go_frames.insert(store, response);
            }
        }
        let go_error = if blocked {
            Some(
                show.1[1]["details"][0]
                    .as_str()
                    .unwrap()
                    .strip_prefix("triggered by error: ")
                    .unwrap()
                    .to_owned(),
            )
        } else {
            None
        };
        let go_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
            &[go.url.clone()],
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        let go_boundary = boundary(&go_store, go_cluster).await;
        let go_next = next_id(&mut go_client, go_cluster).await;
        let fixture = fixture::Fixture::start();
        let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
        StorageEndpoint::new(store.clone()).save_config(&ClusterPaths::new(2200),&json!({"schedule":{"halt-scheduling":"true"},"replication":{"enable-placement-rules":"true"}})).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 2200,
                        member_id: 22,
                        name: "empty".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        assert!(
            node.cluster()
                .restored_config()
                .unwrap()
                .config
                .schedule
                .halt_scheduling
        );
        let (mut rust_client, rust_cluster, rust_step, rust_completion) =
            report_all(&url, mode).await;
        let prepared = node.cluster().unsafe_recovery_plan().await.unwrap();
        assert_eq!(
            crate::recovery_dispatch_test::normalized_frame(
                rust_completion.clone(),
                rust_cluster,
                rust_step
            ),
            crate::recovery_dispatch_test::normalized_frame(
                go_completion.clone(),
                go_cluster,
                go_step
            ),
            "{mode:?}: complete completion frame"
        );
        if !no_work {
            assert_eq!(
                rust_completion.recovery_plan.as_ref().unwrap().step,
                rust_step + 1
            );
            for store in [1, 2] {
                let response = heartbeat(&mut rust_client, rust_cluster, store, 1, None).await;
                assert_eq!(response.recovery_plan.as_ref().unwrap().step, rust_step + 1);
                assert_eq!(plans[&store].step, go_step + 1);
                assert_eq!(
                    crate::recovery_dispatch_test::normalized_frame(
                        response,
                        rust_cluster,
                        rust_step
                    ),
                    crate::recovery_dispatch_test::normalized_frame(
                        go_frames[&store].clone(),
                        go_cluster,
                        go_step
                    ),
                    "{mode:?}: complete store {store} dispatch frame"
                );
            }
        }
        if blocked {
            assert!(prepared.is_none());
            assert!(
                rust_completion.header.as_ref().unwrap().error.is_none(),
                "controller errors enter bounded exit instead of an RPC failure"
            );
            let show = http(&url, "admin/unsafe/remove-failed-stores/show", None);
            let reason = show
                .1
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|v| {
                    v.get("details")
                        .and_then(serde_json::Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .filter_map(serde_json::Value::as_str)
                .find_map(|s| s.strip_prefix("triggered by error: "))
                .expect("generator rejection must enter ExitForceLeader");
            assert_eq!(Some(reason), go_error.as_deref());
            assert_eq!(
                node.cluster()
                    .unsafe_recovery_start_state()
                    .await
                    .unwrap()
                    .0,
                Stage::ExitForceLeader
            );
            assert!(plans.values().all(|p| p.creates.is_empty()));
        } else if no_work {
            assert!(prepared.is_none());
            assert!(rust_completion.header.as_ref().unwrap().error.is_none());
            assert!(go_completion.recovery_plan.is_none());
        } else {
            let prepared = prepared.unwrap();
            assert_eq!(prepared.stage, Stage::CreateEmptyRegion);
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
                assert_eq!(wire.creates, go.creates, "{mode:?}");
                assert_eq!(wire.tombstones, go.tombstones);
                assert_eq!(wire.demotes, go.demotes);
                assert_eq!(wire.force_leader, go.force_leader);
            }
            assert!(
                prepared.batch.plans[&1]
                    .creates
                    .iter()
                    .all(|r| r.peers[0].store_id == 1)
            );
            let ids = prepared.batch.plans[&1]
                .creates
                .iter()
                .map(|r| r.id)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(prepared.batch.newly_created_regions, ids);
        }
        let rust_boundary = boundary(store.as_ref(), rust_cluster).await;
        let rust_next = next_id(&mut rust_client, rust_cluster).await;
        assert_eq!(rust_boundary.0, go_boundary.0, "{mode:?}");
        assert_eq!(
            rust_next, go_next,
            "actual ID positions must not be normalized"
        );
        let expected = match mode {
            Mode::Holes | Mode::Overlap | Mode::UncheckedOverlap => 7,
            Mode::Whole => 3,
            Mode::Contiguous | Mode::NoTiKv => 1,
        };
        assert_eq!(rust_next, expected);
        node.shutdown().await.unwrap();
        go.stop().unwrap();
        println!(
            "{mode:?}: actual Go/Rust allocator (boundary, header revision, key mod_revision) {go_boundary:?}/{rust_boundary:?}, next ID {go_next}/{rust_next}, collect step {go_step}/{rust_step}; complete dispatch frames, generator error exit, exact ID payloads and allocator side effects match"
        );
    }
}
