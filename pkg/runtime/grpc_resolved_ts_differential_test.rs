// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../server/grpc_region_lookup_scenarios_test.rs"]
mod prepare;
#[path = "../server/grpc_resolved_ts_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go binary and read-only supervisor"]
async fn readonly_report_min_resolved_ts_differential() {
    use astersql_pd_compat_tests::harness::Server;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node =
        crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                crate::NodeConfig {
                    cluster_id: 2601,
                    member_id: 1,
                    name: "split".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    node.ready().await.unwrap();
    let mut rust = PdClient::connect(url).await.unwrap();
    let admission = scenarios::admission(&mut rust, 2601).await;
    prepare::prepare(&mut rust, 2601).await;
    use astersql_pd_server::ClusterReadService;
    let mut store_values = vec![];
    for ts in [42, 7, 0, u64::MAX, 100] {
        scenarios::report(&mut rust, 2601, 1, ts).await.unwrap();
        store_values.push(ts);
        assert_eq!(
            node.cluster()
                .store(1)
                .await
                .unwrap()
                .unwrap()
                .min_resolved_ts(),
            ts
        );
    }
    let expected = scenarios::observe(&mut rust, 2601).await;
    let binary = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap());
    let mut leader = Server::start(&binary).unwrap();
    leader.ready().unwrap();
    let mut client = PdClient::connect(leader.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    assert_eq!(admission, scenarios::admission(&mut client, id).await);
    prepare::prepare(&mut client, id).await;
    for ts in &store_values {
        scenarios::report(&mut client, id, 1, *ts).await.unwrap();
        let (status, body) = leader.get("/pd/api/v1/min-resolved-ts/1").unwrap();
        assert_eq!(status, 200);
        assert_eq!(body["min_resolved_ts"].as_u64(), Some(*ts));
    }
    let actual = scenarios::observe(&mut client, id).await;
    assert_eq!(expected, actual);
    let storage = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let paths = astersql_pd_keypath::ClusterPaths::new(2601);
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let (status, body) = leader.get("/pd/api/v1/min-resolved-ts").unwrap();
            assert_eq!(status, 200);
            if body["min_resolved_ts"].as_u64() == Some(100)
                && storage.load_min_resolved_ts(&paths).await.unwrap() == 100
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    leader.restart_with_args(&binary, &[]).unwrap();
    leader.ready().unwrap();
    let (status, body) = leader.get("/pd/api/v1/min-resolved-ts").unwrap();
    assert_eq!(status, 200);
    assert_eq!(body["min_resolved_ts"].as_u64(), Some(100));
    println!(
        "ReportMinResolvedTS success bytes, exact errors, per-store reports and persisted minimum match Go"
    );
    leader.stop().unwrap();
    node.shutdown().await.unwrap();
}
