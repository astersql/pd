// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use tonic::transport::Channel;
pub fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub async fn minimum(client: &mut PdClient<Channel>, id: u64) -> pdpb::Timestamp {
    let result = client
        .get_min_ts(pdpb::GetMinTsRequest { header: header(id) })
        .await;
    assert!(
        result.is_ok(),
        "classic GetMinTS must allocate from the shared Oracle: {result:?}"
    );
    let response = result.unwrap().into_inner();
    assert_eq!(
        response.header.unwrap(),
        pdpb::ResponseHeader {
            cluster_id: id,
            ..Default::default()
        }
    );
    response.timestamp.unwrap()
}
pub async fn observe(client: &mut PdClient<Channel>, id: u64) -> Vec<(i32, String)> {
    let mut previous = minimum(client, id).await;
    let mut errors = Vec::new();
    for count in [1, 2, 100, (1 << 18) - 1, 2] {
        // Classic Go ignores the deprecated local-TSO location field.
        let mut stream = client
            .tso(tokio_stream::iter([pdpb::TsoRequest {
                header: header(id),
                count,
                dc_location: "legacy-dc".into(),
                ..Default::default()
            }]))
            .await
            .unwrap()
            .into_inner();
        let response = stream.message().await.unwrap().unwrap();
        assert_eq!(response.count, count);
        assert_eq!(
            response.header.unwrap(),
            pdpb::ResponseHeader {
                cluster_id: id,
                ..Default::default()
            }
        );
        let next = response.timestamp.unwrap();
        assert_eq!(next.suffix_bits, 0);
        assert!(next.logical >= i64::from(count) && next.logical < 1 << 18);
        assert!(
            (next.physical, next.logical - i64::from(count) + 1)
                > (previous.physical, previous.logical)
        );
        assert!(stream.message().await.unwrap().is_none());
        previous = minimum(client, id).await;
        assert!((previous.physical, previous.logical) > (next.physical, next.logical));
    }
    for count in [0, 1 << 18, u32::MAX] {
        let error = match client
            .tso(tokio_stream::iter([pdpb::TsoRequest {
                header: header(id),
                count,
                ..Default::default()
            }]))
            .await
        {
            Ok(response) => response.into_inner().message().await.unwrap_err(),
            Err(error) => error,
        };
        assert_eq!(error.code(), tonic::Code::Unknown);
        let message = if count == 0 {
            "tso count should be positive"
        } else {
            "generate tso maximum number of retries exceeded"
        };
        assert_eq!(
            error.message(),
            format!("[PD:tso:ErrGenerateTimestamp]generate timestamp failed, {message}")
        );
        errors.push((error.code() as i32, error.message().into()));
    }
    let error = client
        .get_min_ts(pdpb::GetMinTsRequest { header: None })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        error.message(),
        format!("mismatch cluster id, need {id} but got 0")
    );
    // Keep one duplex stream open while interleaving unary allocations.
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    // Go sends response headers only after receiving the first request.
    tx.send(pdpb::TsoRequest {
        header: header(id),
        count: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut stream = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let first = stream.message().await.unwrap().unwrap();
    assert_eq!(first.count, 1);
    for count in [2, 3, 1] {
        let before = minimum(client, id).await;
        tx.send(pdpb::TsoRequest {
            header: header(id),
            count,
            ..Default::default()
        })
        .await
        .unwrap();
        let response = stream.message().await.unwrap().unwrap();
        assert_eq!(response.count, count);
        let ts = response.timestamp.unwrap();
        assert!(
            (ts.physical, ts.logical - i64::from(count) + 1) > (before.physical, before.logical)
        );
    }
    // Client cancellation must not poison subsequent allocations.
    drop(stream);
    drop(tx);
    assert!(minimum(client, id).await.physical > 0);
    errors
}

// Streaming TSO checks identity before allocation; unary GetMinTS checks role first.
pub async fn follower(client: &mut PdClient<Channel>, id: u64) -> Vec<(i32, String)> {
    let mut result = Vec::new();
    for count in [0, 1, 1 << 18] {
        let error = match client
            .tso(tokio_stream::iter([pdpb::TsoRequest {
                header: header(id),
                count,
                ..Default::default()
            }]))
            .await
        {
            Ok(response) => response.into_inner().message().await.unwrap_err(),
            Err(error) => error,
        };
        assert_eq!(error.code(), tonic::Code::Unknown);
        assert_eq!(
            error.message(),
            "[PD:tso:ErrGenerateTimestamp]generate timestamp failed, requested pd is not leader of cluster"
        );
        result.push((error.code() as i32, error.message().into()));
    }
    for request_header in [None, header(id)] {
        let error = client
            .get_min_ts(pdpb::GetMinTsRequest {
                header: request_header,
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unavailable);
        assert_eq!(error.message(), "not leader");
        result.push((error.code() as i32, error.message().into()));
    }
    result
}
