// Copyright 2026 AsterSQL.

use std::fs::{self, File};
use std::process::{Command, Output};
use std::sync::Arc;

use astersql_pd_backup::{BackupInfo, collect, write_backup};
use astersql_pd_keypath::{CLUSTER_ID_PATH, ClusterPaths};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNode, PdNodeBuilder};
use astersql_pd_storage_kv::{Compare, Consistency, MetaStore, Mutation, Read, open_isolated};
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
    node_with_store(Arc::new(fixture.store().await), id).await
}

async fn node_with_store(store: Arc<dyn MetaStore>, id: u64) -> (PdNode, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
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
    assert_pd_boundaries(
        url,
        backup.cluster_id,
        backup.alloc_id_max,
        backup.alloc_timestamp_max,
    )
    .await;
    restored_pd.shutdown().await.unwrap();
}

async fn assert_pd_boundaries(
    url: String,
    cluster_id: u64,
    alloc_id_max: u64,
    alloc_timestamp_max: u64,
) {
    let mut pd = PdClient::connect(url).await.unwrap();
    let member = pd
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(member.header.unwrap().cluster_id, cluster_id);
    let id = pd
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster_id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    assert!(id > alloc_id_max);
    let mut tso = pd
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster_id,
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
    assert!(physical > alloc_timestamp_max / 1_000_000);
    drop(tso);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_native_recovery_drill() {
    let root = tempfile::tempdir().unwrap();
    let source_path = root.path().join("source");
    let target_path = root.path().join("target");
    let archive = root.path().join("backup.json");
    let tampered = root.path().join("tampered.json");
    let pd_id = 99_u64;
    let alloc = 500_u64;
    let timestamp = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    )
    .unwrap();
    let paths = ClusterPaths::new(pd_id);
    let source = open_isolated(&source_path, 72, 1).await.unwrap();
    source
        .consensus()
        .initialize(std::collections::BTreeSet::from([1]))
        .await
        .unwrap();
    source
        .consensus()
        .wait(Some(std::time::Duration::from_secs(4)))
        .current_leader(1, "source leader")
        .await
        .unwrap();
    let lease = source.grant(60).await.unwrap();
    let committed = source
        .transact(
            vec![],
            vec![
                Mutation::put(CLUSTER_ID_PATH.as_bytes(), &pd_id.to_be_bytes()),
                Mutation::put(paths.alloc_id_path().as_bytes(), &alloc.to_be_bytes()),
                Mutation::put(paths.timestamp_path(0).as_bytes(), &timestamp.to_be_bytes()),
                Mutation::put(
                    paths.cluster_path().as_bytes(),
                    &astersql_pd_storage_endpoint::Cluster {
                        id: pd_id,
                        ..Default::default()
                    }
                    .encode(),
                ),
                Mutation::put(
                    paths.cluster_bootstrap_time_path().as_bytes(),
                    &timestamp.to_be_bytes(),
                ),
                Mutation::put(b"/unrelated", b"preserved"),
                Mutation::Put {
                    key: b"/leader-fence".to_vec(),
                    value: b"old".to_vec(),
                    lease: Some(lease.id),
                },
            ],
        )
        .await
        .unwrap();
    source.backup_to(&archive).await.unwrap();
    assert!(source.backup_to(&archive).await.is_err());

    let mut broken: serde_json::Value =
        serde_json::from_slice(&fs::read(&archive).unwrap()).unwrap();
    broken["image"]["revision"] = serde_json::json!(100);
    fs::write(&tampered, serde_json::to_vec(&broken).unwrap()).unwrap();
    let restore = |archive: &std::path::Path, cluster: &str| {
        Command::new(env!("CARGO_BIN_EXE_pd-recover"))
            .arg("--native-restore")
            .arg(archive)
            .arg(&target_path)
            .args([cluster, "1"])
            .output()
            .unwrap()
    };
    assert_eq!(restore(&tampered, "73").status.code(), Some(1));
    assert!(!target_path.exists());
    assert_eq!(restore(&archive, "72").status.code(), Some(1));
    assert!(!target_path.exists());
    let started = std::time::Instant::now();
    let output = restore(&archive, "73");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(restore(&archive, "73").status.code(), Some(1));

    let target = Arc::new(open_isolated(&target_path, 73, 1).await.unwrap());
    target
        .consensus()
        .wait(Some(std::time::Duration::from_secs(4)))
        .current_leader(1, "restored leader")
        .await
        .unwrap();
    let snapshot = target
        .read(Read::prefix(b"", Consistency::Linearizable))
        .await
        .unwrap();
    eprintln!(
        "native restore to verified read: {} ms, {} durable keys",
        started.elapsed().as_millis(),
        snapshot.entries.len()
    );
    assert_eq!(snapshot.revision, committed.revision + 1);
    assert_eq!(snapshot.entries.len(), 6);
    assert!(
        snapshot
            .entries
            .iter()
            .any(|entry| entry.key == b"/unrelated" && entry.value == b"preserved")
    );
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.key != b"/leader-fence")
    );
    assert_eq!(target.consensus_cluster_id().await.unwrap(), Some(73));
    assert_eq!(source.consensus_cluster_id().await.unwrap(), Some(72));
    assert!(
        source
            .read(Read::prefix(b"/leader-fence", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .len()
            == 1
    );
    let new_lease = target.grant(60).await.unwrap();
    assert!(new_lease.id > lease.id);
    let denied = target
        .transact(
            vec![Compare::Lease(b"/leader-fence".to_vec(), lease.id)],
            vec![Mutation::put(b"/stale", b"bad")],
        )
        .await
        .unwrap();
    assert!(!denied.succeeded);

    let (mut pd, url) = node_with_store(target.clone(), pd_id).await;
    assert_pd_boundaries(url, pd_id, alloc, timestamp).await;
    pd.shutdown().await.unwrap();
    drop(pd);
    Arc::try_unwrap(target).ok().unwrap().close().await.unwrap();
    source.close().await.unwrap();
}
