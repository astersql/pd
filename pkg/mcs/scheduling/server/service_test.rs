// Copyright 2026 AsterSQL.
use super::{Host, Service};
use astersql_pd_mcs_server::BaseServer;
use astersql_pd_proto::schedulingpb::{
    self as p, scheduling_client::SchedulingClient, scheduling_server::SchedulingServer,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
struct Inactive;
impl Host for Inactive {
    fn cluster_id(&self) -> u64 {
        91
    }
    fn cluster(&self) -> Option<Arc<dyn astersql_pd_server::PdServices>> {
        None
    }
    fn force_load_stores(&self) {
        panic!("an inactive term must not request store reload");
    }
}
fn check(header: Option<p::ResponseHeader>) {
    let header = header.unwrap();
    assert_eq!(header.cluster_id, 91);
    let error = header.error.unwrap();
    assert_eq!(error.r#type, p::ErrorType::NotBootstrapped as i32);
    assert_eq!(error.message, "cluster is not initialized");
}
#[tokio::test]
async fn mcs_139_inactive_term_keeps_seven_methods_and_stream_terminal_semantics() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let service = Service::new(Arc::new(Inactive), cancel.clone());
    let mut host = BaseServer::start(
        tokio::runtime::Handle::current(),
        &cancel,
        listener,
        move |_| Ok(tonic::service::Routes::new(SchedulingServer::new(service)).into_axum_router()),
        vec![],
    )
    .await
    .unwrap();
    let mut client = SchedulingClient::connect(url).await.unwrap();
    check(
        client
            .store_heartbeat(p::StoreHeartbeatRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header,
    );
    check(
        client
            .get_operator(p::GetOperatorRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header,
    );
    check(
        client
            .split_regions(p::SplitRegionsRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header,
    );
    check(
        client
            .scatter_regions(p::ScatterRegionsRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header,
    );
    check(
        client
            .ask_batch_split(p::AskBatchSplitRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header,
    );
    let mut regions = client
        .region_heartbeat(futures_util::stream::iter([
            p::RegionHeartbeatRequest::default(),
        ]))
        .await
        .unwrap()
        .into_inner();
    check(regions.message().await.unwrap().unwrap().header);
    // Go returns after sending this inactive header, so this path has normal EOF
    // even though a bare active heartbeat receive EOF is wrapped as Unknown.
    assert!(regions.message().await.unwrap().is_none());
    let mut buckets = client
        .region_buckets(futures_util::stream::iter([
            p::RegionBucketsRequest::default(),
        ]))
        .await
        .unwrap()
        .into_inner();
    check(buckets.message().await.unwrap().unwrap().header);
    assert!(buckets.message().await.unwrap().is_none());
    host.shutdown().await.unwrap();
}
