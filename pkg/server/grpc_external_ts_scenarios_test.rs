// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use prost::Message;
type Client = PdClient<tonic::transport::Channel>;
pub fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub async fn get(client: &mut Client, id: u64) -> pdpb::GetExternalTimestampResponse {
    client
        .get_external_timestamp(pdpb::GetExternalTimestampRequest { header: header(id) })
        .await
        .unwrap()
        .into_inner()
}
pub async fn set(client: &mut Client, id: u64, ts: u64) -> pdpb::SetExternalTimestampResponse {
    client
        .set_external_timestamp(pdpb::SetExternalTimestampRequest {
            header: header(id),
            timestamp: ts,
        })
        .await
        .unwrap()
        .into_inner()
}
fn normalized(mut header: pdpb::ResponseHeader, id: u64) -> pdpb::ResponseHeader {
    assert_eq!(header.cluster_id, id);
    header.cluster_id = 0;
    header
}
pub async fn admission(client: &mut Client, id: u64) -> Vec<Vec<u8>> {
    let mut get = get(client, id).await;
    assert_eq!(get.timestamp, 0);
    assert!(get.header.as_ref().unwrap().error.is_none());
    get.header = Some(normalized(get.header.unwrap(), id));
    let mut set = set(client, id, 1).await;
    let error = set.header.as_ref().unwrap().error.as_ref().unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::InvalidValue as i32);
    assert_eq!(
        error.message,
        "[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first"
    );
    set.header = Some(normalized(set.header.unwrap(), id));
    let err = client
        .get_external_timestamp(pdpb::GetExternalTimestampRequest {
            header: header(id + 1),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        err.message(),
        format!("mismatch cluster id, need {id} but got {}", id + 1)
    );
    let err = client
        .set_external_timestamp(pdpb::SetExternalTimestampRequest {
            header: header(id + 1),
            timestamp: 1,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    vec![get.encode_to_vec(), set.encode_to_vec()]
}
pub async fn observe(client: &mut Client, id: u64) -> Vec<Vec<u8>> {
    let mut results = vec![];
    for ts in [1, 100, 101, 101, 7, 0, u64::MAX, 102] {
        let mut response = set(client, id, ts).await;
        let error = response.header.as_ref().unwrap().error.as_ref();
        if ts == u64::MAX {
            assert_eq!(
                error.unwrap().message,
                "the external timestamp should not be larger than global ts"
            );
        } else if [0, 7].contains(&ts) || (ts == 101 && results.len() == 6) {
            assert_eq!(
                error.unwrap().message,
                "the external timestamp should be larger than current external timestamp"
            );
        } else {
            assert!(error.is_none(), "{response:?}");
        }
        response.header = Some(normalized(response.header.unwrap(), id));
        results.push(response.encode_to_vec());
        let mut read = get(client, id).await;
        read.header = Some(normalized(read.header.unwrap(), id));
        results.push(read.encode_to_vec());
    }
    assert_eq!(get(client, id).await.timestamp, 102);
    results
}
