// Copyright 2026 AsterSQL.

use std::fs::{self, File};
use std::process::{Command, Output};
use std::sync::Arc;

use astersql_pd_backup::{BackupInfo, collect, write_backup};
use astersql_pd_keypath::{CLUSTER_ID_PATH, ClusterPaths};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNode, PdNodeBuilder};
use etcd_client::Client;
use tokio_util::sync::CancellationToken;

#[allow(dead_code)]
#[path = "../../pkg/storage/kv/etcd_fixture_test.rs"]
mod fixture;

fn command(endpoint: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pd-recover"))
        .args(["-endpoints", endpoint])
        .args(args)
        .output()
        .unwrap()
}

async fn connect(fixture: &fixture::Fixture) -> Client {
    Client::connect([fixture.endpoint.as_str()], None)
        .await
        .unwrap()
}

async fn value(client: &mut Client, key: &str) -> Option<Vec<u8>> {
    client
        .get(key, None)
        .await
        .unwrap()
        .kvs()
        .first()
        .map(|kv| kv.value().to_vec())
}

async fn node(fixture: &fixture::Fixture, id: u64) -> (PdNode, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: id,
                member_id: 1,
                name: "recover".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    (node, url)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ops_086_preflight_invalid_inputs_have_no_side_effects() {
    let fixture = fixture::Fixture::start();
    let mut client = connect(&fixture).await;
    for args in [
        &["-cluster-id", "99"][..],
        &[
            "-cluster-id",
            "99",
            "-alloc-id",
            "1",
            "-cert",
            "missing.pem",
        ][..],
        &["-cluster-id", "invalid", "-alloc-id", "1"][..],
    ] {
        assert_eq!(command(&fixture.endpoint, args).status.code(), Some(2));
        assert_eq!(value(&mut client, CLUSTER_ID_PATH).await, None);
    }
    assert!(command(&fixture.endpoint, &["-h"]).status.success());
    assert!(command(&fixture.endpoint, &["-V"]).status.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ops_088_old_member_respects_identity_and_allocator_floor() {
    let fixture = fixture::Fixture::start();
    let mut client = connect(&fixture).await;
    let paths = ClusterPaths::new(99);
    assert_eq!(
        command(&fixture.endpoint, &["-from-old-member"])
            .status
            .code(),
        Some(1)
    );
    assert!(
        command(&fixture.endpoint, &["-cluster-id=99", "-alloc-id=500"])
            .status
            .success()
    );
    let store = paths.store_path(10);
    client.put(store.as_str(), b"member", None).await.unwrap();
    client.put("/unrelated", b"preserved", None).await.unwrap();
    assert!(
        command(&fixture.endpoint, &["-from-old-member"])
            .status
            .success()
    );
    assert_eq!(value(&mut client, &store).await, None);
    assert_eq!(
        value(&mut client, "/unrelated").await,
        Some(b"preserved".to_vec())
    );
    assert_eq!(
        value(&mut client, &paths.alloc_id_path()).await,
        Some(100_000_500_u64.to_be_bytes().to_vec())
    );
    assert_eq!(
        command(&fixture.endpoint, &["-cluster-id=100", "-alloc-id=500"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        value(&mut client, CLUSTER_ID_PATH).await,
        Some(99_u64.to_be_bytes().to_vec())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ops_092_roundtrip_backup_recover_and_restart() {
    let source = fixture::Fixture::start();
    let mut source_client = connect(&source).await;
    assert!(
        command(&source.endpoint, &["-cluster-id=99", "-alloc-id=500"])
            .status
            .success()
    );
    let paths = ClusterPaths::new(99);
    let (mut source_pd, source_url) = node(&source, 99).await;
    let config = Command::new("curl")
        .args(["-fsS", "--max-time", "4"])
        .arg(format!("{source_url}/pd/api/v1/config"))
        .output()
        .unwrap();
    assert!(
        config.status.success(),
        "{}",
        String::from_utf8_lossy(&config.stderr)
    );
    let backup = collect(
        &mut source_client,
        serde_json::from_slice(&config.stdout).unwrap(),
    )
    .await
    .unwrap();
    assert!(backup.alloc_timestamp_max > 0);
    source_pd.shutdown().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("backup.json");
    write_backup(File::create(&file).unwrap(), &backup).unwrap();
    let backup: BackupInfo = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();

    let mut target = fixture::Fixture::start();
    let id = format!("-cluster-id={}", backup.cluster_id);
    let alloc = format!("-alloc-id={}", backup.alloc_id_max);
    let timestamp = format!("-alloc-timestamp={}", backup.alloc_timestamp_max);
    let restored = command(&target.endpoint, &[&id, &alloc, &timestamp]);
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(
        command(&target.endpoint, &[&id, &alloc, &timestamp])
            .status
            .code(),
        Some(1)
    );
    target.command("stop", "stopped");
    target.command("start", "restarted");
    let mut target_client = connect(&target).await;
    assert_eq!(
        value(&mut target_client, &paths.alloc_id_path()).await,
        Some(backup.alloc_id_max.to_be_bytes().to_vec())
    );
    assert_eq!(
        value(&mut target_client, &paths.timestamp_path(0)).await,
        Some(backup.alloc_timestamp_max.to_be_bytes().to_vec())
    );
    let (mut restored_pd, url) = node(&target, backup.cluster_id).await;
    let mut pd = PdClient::connect(url).await.unwrap();
    let member = pd
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(member.header.unwrap().cluster_id, backup.cluster_id);
    let id = pd
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: backup.cluster_id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    assert!(id > backup.alloc_id_max);
    let mut tso = pd
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: backup.cluster_id,
                ..Default::default()
            }),
            count: 1,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    let response = tso.message().await.unwrap().unwrap();
    assert!(response.header.unwrap().error.is_none());
    let physical = u64::try_from(response.timestamp.unwrap().physical).unwrap();
    assert!(physical > backup.alloc_timestamp_max / 1_000_000);
    drop(tso);
    restored_pd.shutdown().await.unwrap();
}
