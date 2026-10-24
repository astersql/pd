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
    for (name, id, version, closed, bad_header) in [
        ("missing", 99, 1, false, false),
        ("accepted", 1, 3, false, false),
        ("stale", 1, 2, false, false),
        ("duplicate", 1, 3, false, false),
        ("eof", 1, 4, true, false),
        ("wrong cluster", 1, 5, false, true),
    ] {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tx.send(pdpb::ReportBucketsRequest {
            header: if bad_header { None } else { header.clone() },
            buckets: Some(metapb::Buckets {
                region_id: id,
                version,
                keys: vec![vec![], b"m".to_vec(), vec![]],
                period_in_ms: 2000,
                stats: Some(metapb::BucketStats {
                    read_bytes: vec![200000, 400000],
                    read_keys: vec![4, 8],
                    read_qps: vec![6, 12],
                    write_bytes: vec![800000, 1600000],
                    write_keys: vec![10, 20],
                    write_qps: vec![12, 24],
                }),
            }),
            ..Default::default()
        })
        .await
        .unwrap();
        let hold = if closed {
            drop(tx);
            None
        } else {
            Some(tx)
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            client.report_buckets(tokio_stream::wrappers::ReceiverStream::new(rx)),
        )
        .await;
        println!("{name}: {result:?}");
        if closed || bad_header {
            let error = result.unwrap().unwrap_err();
            assert_eq!(
                error.code(),
                if bad_header {
                    tonic::Code::FailedPrecondition
                } else {
                    tonic::Code::Unknown
                }
            );
            if closed {
                assert_eq!(error.message(), "EOF");
            }
        } else {
            assert!(result.is_err());
        }
        drop(hold);
        let snapshot = client
            .get_region_by_id(pdpb::GetRegionByIdRequest {
                header: header.clone(),
                region_id: 1,
                need_buckets: true,
            })
            .await
            .unwrap()
            .into_inner();
        println!("snapshot (store bucket feature defaults off): {snapshot:?}");
        let output = std::process::Command::new("python3").args(["-c", "import sys,urllib.request; print(urllib.request.urlopen(sys.argv[1]).read().decode())", &format!("{url}/pd/api/v1/hotspot/buckets")]).output().unwrap();
        println!("hot: {}", String::from_utf8_lossy(&output.stdout));
        assert!(output.status.success());
    }
}
