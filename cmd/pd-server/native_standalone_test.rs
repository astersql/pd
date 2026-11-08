// Copyright 2026 AsterSQL.
#![cfg(unix)]

use std::{
    fs,
    net::{SocketAddr, TcpListener},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::Command,
    time::Duration,
};

use astersql_pd_proto::meta_storagepb::{self as pb, meta_storage_client::MetaStorageClient};

mod native_process_test_support;
use native_process_test_support::{Host, observe};

const KEY: &[u8] = b"task18/local-wal";
const VALUE: &[u8] = b"acknowledged\0before-sigkill";

fn reserve_address() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

fn pd_server() -> Command {
    let executable = std::env::var_os("PD_NATIVE_RELEASE_BIN")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_pd-server").into());
    let mut command = Command::new(executable);
    command.env_clear().env("PATH", "/nonexistent");
    command
}

fn local(root: &Path, address: SocketAddr, cluster: u64) -> Command {
    let mut command = pd_server();
    command
        .arg("--name=native-local-test")
        .arg(format!("--client-urls=http://{address}"))
        .arg(format!(
            "--local-native-data-dir={}",
            root.join("store").display()
        ))
        .arg(format!("--local-native-cluster-id={cluster}"));
    command
}

async fn assert_rejected(root: &Path, command: &mut Command, message: &str) {
    let mut child = Host::start(root, "rejected", command);
    let status = child.wait().await;
    let logs = child.logs();
    assert_eq!(status.code(), Some(1), "{logs}");
    assert!(logs.contains(message), "{logs}");
}

fn metadata(address: SocketAddr) -> MetaStorageClient<tonic::transport::Channel> {
    let endpoint = tonic::transport::Endpoint::from_shared(format!("http://{address}"))
        .unwrap()
        .timeout(Duration::from_secs(3));
    MetaStorageClient::new(endpoint.connect_lazy())
}

async fn assert_committed_metadata(address: SocketAddr, revision: i64) {
    let reply = metadata(address)
        .get(pb::GetRequest {
            key: KEY.to_vec(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let header = reply.header.unwrap();
    assert!(header.error.is_none() && header.revision >= revision);
    assert_eq!(reply.kvs.len(), 1);
    assert_eq!(reply.kvs[0].key, KEY);
    assert_eq!(reply.kvs[0].value, VALUE);
}

#[tokio::test]
async fn task18_local_native_release_wal() {
    let root = tempfile::tempdir().unwrap();
    let address = reserve_address();
    let mut first = Host::start(root.path(), "first", &mut local(root.path(), address, 18));
    first
        .wait_for(|host| host.logs().contains("PD ready"))
        .await;
    let before = observe(address).await;
    let commit = metadata(address)
        .put(pb::PutRequest {
            key: KEY.to_vec(),
            value: VALUE.to_vec(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap();
    assert!(commit.error.is_none() && commit.revision > 0);
    assert_committed_metadata(address, commit.revision).await;
    first.assert_no_children();

    assert_rejected(
        root.path(),
        &mut local(root.path(), reserve_address(), 18),
        "startup or runtime failed",
    )
    .await;
    assert_committed_metadata(address, commit.revision).await;

    let wal = root.path().join("store/metadata.sqlite-wal");
    let bytes = fs::metadata(&wal).unwrap().len();
    assert!(bytes > 0);
    first.child.kill().unwrap();
    assert_eq!(first.wait().await.signal(), Some(9));
    assert!(wal.is_file());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
    assert_rejected(
        root.path(),
        &mut local(root.path(), address, 19),
        "startup or runtime failed",
    )
    .await;

    let mut reopened = Host::start(root.path(), "reopen", &mut local(root.path(), address, 18));
    reopened
        .wait_for(|host| host.logs().contains("PD ready"))
        .await;
    let after = observe(address).await;
    assert_eq!(after.0, before.0);
    assert!(after.1 > before.1 && after.2 > before.2);
    assert_committed_metadata(address, commit.revision).await;
    reopened.assert_no_children();
    reopened.signal("-TERM");
    assert!(reopened.wait().await.success(), "{}", reopened.logs());
    let logs = reopened.logs();
    let stopped = logs.find("PD stopped").unwrap();
    assert!(
        logs[stopped..].contains("local native store closed"),
        "{logs}"
    );
    drop(TcpListener::bind(address).unwrap());
    let store = astersql_pd_storage_kv::open_isolated(&root.path().join("store"), 18, 1)
        .await
        .unwrap();
    store.close().await.unwrap();
    println!(
        "local standalone: GetMembers cluster {}, AllocID {} -> {}, Tso {:?} -> {:?}, HTTP health true; MetaStorage bytes retained at revision {}, WAL {} bytes; lock and listener released",
        after.0, before.1, after.1, before.2, after.2, commit.revision, bytes,
    );

    if let Some(oracle) = std::env::var_os("PD_NATIVE_GO_ORACLE") {
        let oracle_address = reserve_address();
        let client = format!("http://{oracle_address}");
        let peer = format!("http://{}", reserve_address());
        let mut command = Command::new(oracle);
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .arg("--name=native-local-test")
            .arg(format!("--data-dir={}", root.path().join("go").display()))
            .arg(format!("--client-urls={client}"))
            .arg(format!("--advertise-client-urls={client}"))
            .arg(format!("--peer-urls={peer}"))
            .arg(format!("--advertise-peer-urls={peer}"))
            .arg(format!("--initial-cluster=native-local-test={peer}"));
        let mut go = Host::start(root.path(), "oracle", &mut command);
        go.wait_for(|host| host.logs().contains("PD leader is ready to serve"))
            .await;
        let reference = observe(oracle_address).await;
        assert_eq!(reference.1, before.1);
        go.signal("-TERM");
        assert!(go.wait().await.success(), "{}", go.logs());
        println!(
            "Go oracle: GetMembers cluster {}, AllocID {}, Tso {:?}, HTTP health true",
            reference.0, reference.1, reference.2,
        );
    }
}

#[tokio::test]
async fn task18_local_native_rejects_unsafe_combinations() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let address = reserve_address();
    assert_rejected(root, &mut pd_server(), "--etcd-endpoints").await;
    assert_rejected(
        root,
        pd_server().arg("--local-native-cluster-id=18"),
        "requires both",
    )
    .await;
    assert_rejected(root, &mut local(root, address, 0), "nonzero integer").await;

    for extra in [
        "--etcd-endpoints=http://127.0.0.1:1",
        "--join=http://127.0.0.1:1",
        "--data-dir=unsupported",
    ] {
        assert_rejected(
            root,
            local(root, address, 18).arg(extra),
            "without etcd or join",
        )
        .await;
    }
    for extra in [
        "--client-urls=http://0.0.0.0:2379",
        "--advertise-client-urls=http://localhost:2379",
        "--peer-urls=http://192.0.2.1:2380",
        "--advertise-peer-urls=http://192.0.2.1:2380",
    ] {
        assert_rejected(
            root,
            local(root, address, 18).arg(extra),
            "literal loopback",
        )
        .await;
    }
    assert_rejected(
        root,
        local(root, address, 18).env("PD_SERVICE_MODE", "api"),
        "requires classic PD",
    )
    .await;
    let dir = root.join("store");
    assert!(!dir.exists());
    let mut config_check = Host::start(
        root,
        "config-check",
        local(root, address, 18).arg("--config-check"),
    );
    assert!(
        config_check.wait().await.success(),
        "{}",
        config_check.logs()
    );
    assert!(!dir.exists());

    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("source-marker"), b"source").unwrap();
    assert_rejected(
        root,
        &mut local(root, address, 18),
        "startup or runtime failed",
    )
    .await;
    assert_eq!(fs::read(dir.join("source-marker")).unwrap(), b"source");
    assert_eq!(fs::read_dir(dir).unwrap().count(), 1);
    println!("local flags, non-loopback endpoints, API mode and foreign directory rejected");
}
