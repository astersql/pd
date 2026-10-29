// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use prost::Message;
pub type Client = PdClient<tonic::transport::Channel>;
pub fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub async fn report(
    client: &mut Client,
    id: u64,
    store_id: u64,
    ts: u64,
) -> Result<pdpb::ReportMinResolvedTsResponse, tonic::Status> {
    client
        .report_min_resolved_ts(pdpb::ReportMinResolvedTsRequest {
            header: header(id),
            store_id,
            min_resolved_ts: ts,
        })
        .await
        .map(|r| r.into_inner())
}
pub async fn observe(client: &mut Client, id: u64) -> Vec<Vec<u8>> {
    let mut results = vec![];
    for ts in [42, 42, 7, 0, u64::MAX, 100] {
        let mut response = report(client, id, 1, ts).await.unwrap();
        assert!(response.header.as_ref().unwrap().error.is_none());
        assert_eq!(response.header.as_ref().unwrap().cluster_id, id);
        response.header.as_mut().unwrap().cluster_id = 0;
        results.push(response.encode_to_vec());
    }
    for store_id in [0, 999] {
        let error = report(client, id, store_id, 42).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unknown);
        assert_eq!(
            error.message(),
            format!("[PD:core:ErrStoreNotFound]store {store_id} not found")
        );
        results.push(format!("{:?}:{}", error.code(), error.message()).into_bytes());
    }
    for (store_id, state, node_state) in [(8, 2, 3), (9, 1, 2)] {
        let reply = client
            .put_store(pdpb::PutStoreRequest {
                header: header(id),
                store: Some(metapb::Store {
                    id: store_id,
                    address: format!("store{store_id}"),
                    version: "8.5.0".into(),
                    state,
                    node_state,
                    last_heartbeat: i64::MAX,
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(reply.header.unwrap().error.is_none());
    }
    let removed = report(client, id, 8, 42).await.unwrap_err();
    assert_eq!(removed.code(), tonic::Code::Unknown);
    assert_eq!(
        removed.message(),
        "[PD:core:ErrStoreRemoved]store 8 has been removed"
    );
    results.push(removed.message().as_bytes().to_vec());
    assert!(
        report(client, id, 9, 55)
            .await
            .unwrap()
            .header
            .unwrap()
            .error
            .is_none()
    );
    results
}

pub async fn admission(client: &mut Client, id: u64) -> Vec<Vec<u8>> {
    let mut reply = report(client, id, 999, 42).await.unwrap();
    let h = reply.header.as_mut().unwrap();
    assert_eq!(
        h.error.as_ref().unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    assert_eq!(
        h.error.as_ref().unwrap().message,
        "cluster is not bootstrapped"
    );
    h.cluster_id = 0;
    let invalid = report(client, id + 1, 999, 42).await.unwrap_err();
    assert_eq!(invalid.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        invalid.message(),
        format!("mismatch cluster id, need {id} but got {}", id + 1)
    );
    vec![
        reply.encode_to_vec(),
        format!("{:?}", invalid.code()).into_bytes(),
    ]
}
