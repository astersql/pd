// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use tonic::transport::Channel;

// Same wire assertions run against the Rust host and the existing Go binary.
pub async fn observe(client: &mut PdClient<Channel>, id: u64) {
    for header in [
        None,
        Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        }),
        Some(pdpb::RequestHeader {
            cluster_id: id ^ u64::MAX,
            ..Default::default()
        }),
    ] {
        for skip_check in [false, true] {
            let response = client
                .sync_max_ts(pdpb::SyncMaxTsRequest {
                    header: header.clone(),
                    max_ts: Some(pdpb::Timestamp {
                        physical: i64::MAX,
                        logical: i64::MAX,
                        suffix_bits: u32::MAX,
                    }),
                    skip_check,
                })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                response,
                pdpb::SyncMaxTsResponse {
                    header: Some(pdpb::ResponseHeader {
                        cluster_id: id,
                        error: None
                    }),
                    ..Default::default()
                }
            );
        }
        for dc in ["", "global", "unknown\0dc"] {
            let response = client
                .get_dc_location_info(pdpb::GetDcLocationInfoRequest {
                    header: header.clone(),
                    dc_location: dc.into(),
                })
                .await
                .unwrap()
                .into_inner();
            assert_eq!(
                response,
                pdpb::GetDcLocationInfoResponse {
                    header: Some(pdpb::ResponseHeader {
                        cluster_id: id,
                        error: None
                    }),
                    ..Default::default()
                }
            );
        }
    }
    for header in [
        None,
        Some(pdpb::RequestHeader {
            cluster_id: id ^ u64::MAX,
            ..Default::default()
        }),
    ] {
        let result = client
            .watch_gc_safe_point_v2(pdpb::WatchGcSafePointV2Request {
                header: header.clone(),
                ..Default::default()
            })
            .await;
        let status = match result {
            Err(status) => status,
            Ok(response) => response.into_inner().message().await.unwrap_err(),
        };
        assert_eq!(status.code(), tonic::Code::Unimplemented);
        assert_eq!(
            status.message(),
            "WatchGCSafePointV2 is obsolete. Poll GetAllKeyspacesGCStates instead if necessary"
        );
        let result = client
            .watch_gc_states(pdpb::WatchGcStatesRequest {
                header,
                ..Default::default()
            })
            .await;
        let status = match result {
            Err(status) => status,
            Ok(response) => response.into_inner().message().await.unwrap_err(),
        };
        assert_eq!(status.code(), tonic::Code::Unimplemented);
        assert_eq!(status.message(), "WatchGCStates is not supported yet");
    }
    // A deadline belongs to the transport; aborting a caller must not poison the next RPC.
    let mut cancelled = client.clone();
    let call = tokio::spawn(async move {
        cancelled
            .sync_max_ts(pdpb::SyncMaxTsRequest::default())
            .await
    });
    call.abort();
    if let Err(error) = call.await {
        assert!(error.is_cancelled());
    }
    client
        .get_dc_location_info(pdpb::GetDcLocationInfoRequest::default())
        .await
        .unwrap();
}
