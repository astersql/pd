// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "grpc_report_split_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_027_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2700,
                member_id: 1,
                name: "report".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    for batch in [false, true] {
        let h = if batch {
            client
                .report_batch_split(pdpb::ReportBatchSplitRequest {
                    header: scenarios::header(2700),
                    ..Default::default()
                })
                .await
                .map(|r| r.into_inner().header)
        } else {
            client
                .report_split(pdpb::ReportSplitRequest {
                    header: scenarios::header(2700),
                    ..Default::default()
                })
                .await
                .map(|r| r.into_inner().header)
        };
        assert!(
            h.is_ok(),
            "Report split must serve protocol responses: {h:?}"
        );
        assert_eq!(
            h.unwrap().unwrap().error.unwrap().r#type,
            pdpb::ErrorType::NotBootstrapped as i32
        );
    }
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2700).await;
    let before = store
        .read(Read::prefix(b"/pd/2700/raft/r/", Consistency::Linearizable))
        .await
        .unwrap()
        .entries;
    let original = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: scenarios::header(2700),
            region_id: 10,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    scenarios::observe(&mut client, 2700).await;
    assert_eq!(
        before,
        store
            .read(Read::prefix(b"/pd/2700/raft/r/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    assert_eq!(
        original,
        client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: scenarios::header(2700),
                region_id: 10,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
    );
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    cancel.cancel();
    assert!(
        client
            .report_split(pdpb::ReportSplitRequest {
                header: scenarios::header(2700),
                left: Some(metapb::Region::default()),
                right: Some(metapb::Region::default())
            })
            .await
            .is_err()
    );
    node.shutdown().await.unwrap();
}
