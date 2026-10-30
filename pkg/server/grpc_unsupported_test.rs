// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::pd_client::PdClient;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_037_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 3600,
                member_id: 1,
                name: "tso".into(),
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
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let read = || Read::prefix(b"/pd/3600/timestamp", Consistency::Linearizable);
    let before = store.read(read()).await.unwrap().entries;
    super::grpc_unsupported_scenarios_test::observe(&mut client, 3600).await;
    assert_eq!(before, store.read(read()).await.unwrap().entries);
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    verify_pending(&url).await;
    node.shutdown().await.unwrap();
}

#[test]
fn task_037_descriptor_ownership() {
    use prost::Message;
    let descriptor =
        prost_types::FileDescriptorSet::decode(astersql_pd_proto::FILE_DESCRIPTOR_SET).unwrap();
    let service = descriptor
        .file
        .iter()
        .flat_map(|f| &f.service)
        .find(|s| s.name() == "PD")
        .unwrap();
    let methods: std::collections::BTreeSet<_> = service.method.iter().map(|m| m.name()).collect();
    let owned: std::collections::BTreeSet<_> = OWNERS.iter().map(|(name, _)| *name).collect();
    assert_eq!(methods, owned);
    assert_eq!(OWNERS.len(), methods.len());
    let transport = include_str!("sync_transport.rs");
    for (name, owner) in OWNERS.iter().filter(|(_, owner)| *owner <= 37) {
        let chars: Vec<char> = name.chars().collect();
        let mut snake = String::new();
        for (i, c) in chars.iter().enumerate() {
            if c.is_uppercase()
                && i > 0
                && (chars[i - 1].is_lowercase()
                    || chars.get(i + 1).is_some_and(|next| next.is_lowercase()))
            {
                snake.push('_');
            }
            snake.extend(c.to_lowercase());
        }
        assert!(
            transport.contains(&format!("async fn {snake}(")),
            "{name} owner {owner} must be explicitly wired"
        );
    }

    let inventory = include_str!("../../docs/rust/server-api/wire-inventory.md");
    for (name, owner) in OWNERS {
        let row = inventory
            .lines()
            .find(|row| row.starts_with(&format!("| {name} |")))
            .unwrap();
        assert_eq!(
            row.split('|')
                .nth(5)
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap(),
            *owner
        );
    }
    assert_eq!(
        crate::grpc_unsupported::header(0).unwrap().error.unwrap(),
        astersql_pd_proto::pdpb::Error {
            r#type: astersql_pd_proto::pdpb::ErrorType::NotBootstrapped as i32,
            message: "cluster id is not ready".into(),
        }
    );
}

async fn verify_pending(url: &str) {
    // These fail-closed responses are migration gaps, not Go compatibility claims.
    use astersql_pd_proto::pdpb;
    let mut grpc = tonic::client::Grpc::new(
        tonic::transport::Endpoint::from_shared(url.to_owned())
            .unwrap()
            .connect()
            .await
            .unwrap(),
    );
    for (method, owner) in OWNERS
        .iter()
        .filter(|(_, owner)| *owner > 37 && *owner != 74)
    {
        assert!([74, 78].contains(owner));
        if ["WatchGCSafePointV2", "WatchGCStates"].contains(method) {
            continue;
        }
        grpc.ready().await.unwrap();
        let path = tonic::codegen::http::uri::PathAndQuery::from_maybe_shared(format!(
            "/pdpb.PD/{method}"
        ))
        .unwrap();
        let codec =
            tonic_prost::ProstCodec::<pdpb::GetMembersRequest, pdpb::GetMembersResponse>::default();
        let status = grpc
            .server_streaming(
                tonic::Request::new(pdpb::GetMembersRequest::default()),
                path,
                codec,
            )
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::Unimplemented, "{method}");
        assert_eq!(status.message(), "Not yet implemented", "{method}");
    }
}

// Every pinned PD method has exactly one task owner.
const OWNERS: &[(&str, u32)] = &[
    ("GetClusterInfo", 19),
    ("GetMembers", 19),
    ("Tso", 36),
    ("Bootstrap", 20),
    ("IsBootstrapped", 20),
    ("AllocID", 21),
    ("IsSnapshotRecovering", 74),
    ("GetStore", 22),
    ("PutStore", 22),
    ("GetAllStores", 22),
    ("StoreHeartbeat", 8),
    ("RegionHeartbeat", 9),
    ("GetRegion", 23),
    ("GetPrevRegion", 23),
    ("GetRegionByID", 23),
    ("QueryRegion", 25),
    ("ScanRegions", 24),
    ("BatchScanRegions", 24),
    ("AskSplit", 26),
    ("ReportSplit", 27),
    ("AskBatchSplit", 26),
    ("ReportBatchSplit", 27),
    ("GetClusterConfig", 28),
    ("PutClusterConfig", 28),
    ("ScatterRegion", 29),
    ("GetGCSafePoint", 78),
    ("UpdateGCSafePoint", 78),
    ("UpdateServiceGCSafePoint", 78),
    ("GetGCSafePointV2", 78),
    ("WatchGCSafePointV2", 78),
    ("UpdateGCSafePointV2", 78),
    ("UpdateServiceSafePointV2", 78),
    ("GetAllGCSafePointV2", 78),
    ("AdvanceGCSafePoint", 78),
    ("AdvanceTxnSafePoint", 78),
    ("SetGCBarrier", 78),
    ("DeleteGCBarrier", 78),
    ("SetGlobalGCBarrier", 78),
    ("DeleteGlobalGCBarrier", 78),
    ("GetGCState", 78),
    ("GetAllKeyspacesGCStates", 78),
    ("WatchGCStates", 78),
    ("SyncRegions", 11),
    ("GetOperator", 29),
    ("SyncMaxTS", 37),
    ("SplitRegions", 30),
    ("SplitAndScatterRegions", 31),
    ("GetDCLocationInfo", 37),
    ("StoreGlobalConfig", 32),
    ("LoadGlobalConfig", 32),
    ("WatchGlobalConfig", 33),
    ("ReportBuckets", 10),
    ("ReportMinResolvedTS", 34),
    ("SetExternalTimestamp", 35),
    ("GetExternalTimestamp", 35),
    ("GetMinTS", 36),
];
