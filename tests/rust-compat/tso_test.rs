// Copyright 2026 AsterSQL.
use astersql_pd_compat_tests::{
    harness::Server,
    normalize::{differences, tso},
};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use prost::Message;
use serde_json::{Value, json};
use std::path::Path;
async fn sample(server: &Server) -> Value {
    let channel = tonic::transport::Endpoint::from_shared(server.url.clone())
        .unwrap()
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(3))
        .connect()
        .await
        .unwrap();
    let mut client = PdClient::new(channel);
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let cluster = members.header.unwrap().cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let mut output = Vec::new();
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(pdpb::TsoRequest {
        header: header.clone(),
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
    let mut previous = None;
    for (index, count) in [1, 3, 1].into_iter().enumerate() {
        if index > 0 {
            tx.send(pdpb::TsoRequest {
                header: header.clone(),
                count,
                ..Default::default()
            })
            .await
            .unwrap();
        }
        let response = tokio::time::timeout(std::time::Duration::from_secs(3), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(response.count, count);
        assert_eq!(response.header.as_ref().unwrap().cluster_id, cluster);
        assert!(response.header.as_ref().unwrap().error.is_none());
        let timestamp = response.timestamp.as_ref().unwrap();
        let current = (timestamp.physical, timestamp.logical);
        assert!(current.0 > 0 && current.1 >= i64::from(count));
        let logical_delta = if let Some(previous) = previous {
            assert!(current > previous);
            if current.0 == previous.0 {
                current.1 - previous.1
            } else {
                current.1
            }
        } else {
            current.1
        };
        assert_eq!(logical_delta, i64::from(count));
        previous = Some(current);
        // Wall-clock tick boundaries vary across processes. Compare each batch's start
        // origin and logical increment; continuity is checked above and suffix/count/header remain exact.
        let mut normalized = tso(&response.encode_to_vec(), timestamp.physical).unwrap();
        normalized["header"]["cluster_id"] = json!("<cluster>");
        normalized["timestamp"]["logical"] = json!(logical_delta);
        output.push(normalized);
    }
    drop((tx, stream));
    let error = match client
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header,
            count: 0,
            ..Default::default()
        }]))
        .await
    {
        Err(error) => error,
        Ok(response) => response.into_inner().message().await.unwrap_err(),
    };
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert!(error.message().contains("tso count should be positive"));
    json!({"batches":output,"zero_count_code":format!("{:?}",error.code()),"zero_count_message":error.message()})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Go/Rust process binaries; run scripts/run-pd-compat.sh"]
async fn tso_processes() {
    let mode = std::env::var("PD_COMPAT_MODE").expect("use scripts/run-pd-compat.sh");
    let go = std::env::var("PD_COMPAT_GO_BIN").unwrap_or_default();
    let rust = std::env::var("PD_COMPAT_RUST_BIN").unwrap_or_default();
    let mut samples = Vec::new();
    for is_rust in match mode.as_str() {
        "go" => vec![false],
        "rust" => vec![true],
        "dual" => vec![false, true],
        _ => panic!("invalid mode"),
    } {
        let mut server = if is_rust {
            Server::start_rust(Path::new(&rust))
        } else {
            Server::start(Path::new(&go))
        }
        .unwrap();
        server.ready().unwrap();
        samples.push(sample(&server).await);
        server.stop().unwrap();
    }
    if samples.len() == 2 {
        assert_eq!(differences(&samples[0], &samples[1]), Vec::<String>::new());
    }
    println!("TSO: 3 monotonic batches, counts/header/suffix, zero-count status and error equal");
}
