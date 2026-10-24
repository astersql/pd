// Copyright 2026 AsterSQL.
//! Read-only-source Go server oracle probe; runs against a disposable host.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
#[tokio::main]
async fn main() {
    let url = std::env::var("PD_ORACLE_URL").unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let header = Some(pdpb::RequestHeader {
        cluster_id: members.header.unwrap().cluster_id,
        ..Default::default()
    });
    let preboot = client
        .report_buckets(tokio_stream::iter([pdpb::ReportBucketsRequest {
            header: header.clone(),
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    println!("not bootstrapped: {preboot:?}");
    assert_eq!(
        preboot.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    let store = metapb::Store {
        id: 1,
        address: "127.0.0.1:29999".into(),
        version: "8.5.0".into(),
        ..Default::default()
    };
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(store),
            region: Some(metapb::Region {
                id: 1,
                region_epoch: Some(metapb::RegionEpoch {
                    conf_ver: 1,
                    version: 1,
                }),
                peers: vec![metapb::Peer {
                    id: 2,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    println!("bootstrap: {:?}", response.header);
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let base = pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(metapb::Region {
            id: 1,
            region_epoch: Some(metapb::RegionEpoch {
                conf_ver: 2,
                version: 2,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        }),
        leader: Some(peer),
        term: 7,
        bytes_written: 1234,
        keys_written: 23,
        bytes_read: 4567,
        keys_read: 89,
        bucket_meta: Some(metapb::BucketMeta {
            version: 7,
            keys: vec![vec![], b"m".to_vec(), vec![]],
        }),
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 10,
            end_timestamp: 20,
        }),
        query_stats: Some(pdpb::QueryStats {
            get: 11,
            put: 13,
            ..Default::default()
        }),
        cpu_stats: Some(pdpb::CpuStats {
            unified_read: 17,
            scheduler: 999,
        }),
        approximate_size: 2097152,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tx.send(base).await.unwrap();
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        client.region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx)),
    )
    .await;
    drop(tx);
    for _ in 0..50 {
        let snapshot = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header.clone(),
                region_id: 1,
                need_buckets: true,
            })
            .await
            .unwrap()
            .into_inner();
        if snapshot.leader.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let request = pdpb::SyncRegionRequest {
        header: header.clone(),
        member: Some(pdpb::Member {
            name: "codec-probe".into(),
            client_urls: vec!["http://127.0.0.1:29998".into()],
            ..Default::default()
        }),
        start_index: 0,
    };
    let channel = tonic::transport::Endpoint::from_shared(url)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut grpc = tonic::client::Grpc::new(channel);
    grpc.ready().await.unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(request).await.unwrap();
    let mut stream = grpc
        .streaming(
            tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx)),
            tonic::codegen::http::uri::PathAndQuery::from_static("/pdpb.PD/SyncRegions"),
            RawCodec,
        )
        .await
        .unwrap()
        .into_inner();
    let mut frames = vec![];
    for _ in 0..2 {
        let raw = tokio::time::timeout(std::time::Duration::from_secs(10), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let frame = pdpb::SyncRegionResponse::decode(raw.as_slice()).unwrap();
        println!("Go sync: {frame:?}");
        frames.push(raw.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }
    std::fs::write(
        "tests/rust-compat/fixtures/state/task-50.json",
        serde_json::to_string_pretty(&frames).unwrap() + "\n",
    )
    .unwrap();
    drop(stream);
    drop(tx);
}
use prost::{Message, bytes::Buf};
#[derive(Default)]
struct RawCodec;
struct RawEncoder;
struct RawDecoder;
impl tonic::codec::Codec for RawCodec {
    type Encode = pdpb::SyncRegionRequest;
    type Decode = Vec<u8>;
    type Encoder = RawEncoder;
    type Decoder = RawDecoder;
    fn encoder(&mut self) -> RawEncoder {
        RawEncoder
    }
    fn decoder(&mut self) -> RawDecoder {
        RawDecoder
    }
}
impl tonic::codec::Encoder for RawEncoder {
    type Item = pdpb::SyncRegionRequest;
    type Error = tonic::Status;
    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut tonic::codec::EncodeBuf<'_>,
    ) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|e| tonic::Status::internal(e.to_string()))
    }
}
impl tonic::codec::Decoder for RawDecoder {
    type Item = Vec<u8>;
    type Error = tonic::Status;
    fn decode(
        &mut self,
        src: &mut tonic::codec::DecodeBuf<'_>,
    ) -> Result<Option<Self::Item>, Self::Error> {
        Ok(Some(src.copy_to_bytes(src.remaining()).to_vec()))
    }
}
