// Copyright 2026 AsterSQL.

use std::{
    collections::BTreeSet, fs, net::SocketAddr, os::unix::process::ExitStatusExt, path::PathBuf,
    process::Command, sync::Arc, time::Duration,
};

use astersql_pd_runtime::{PdNodeBuilder, bootstrap_config};
use astersql_pd_storage_kv::open_isolated;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::native_process_test_support::{Host, observe};

const FIXTURE: &str = "native_single_binary_test::run_native_host_fixture";

fn start_host(root: &std::path::Path, label: &str, address: Option<SocketAddr>) -> Host {
    Host::start(
        root,
        label,
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", FIXTURE, "--nocapture"])
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("PD_NATIVE_HOST_STORE", root.join("store"))
            .env("PD_NATIVE_HOST_READY", root.join(format!("{label}.ready")))
            .env(
                "PD_NATIVE_HOST_ADDRESS",
                address
                    .map(|address| address.to_string())
                    .unwrap_or_default(),
            ),
    )
}

#[tokio::test]
#[ignore = "test subprocess driven by theme10_native_single_binary_embedded_wal"]
async fn run_native_host_fixture() {
    let root = PathBuf::from(std::env::var_os("PD_NATIVE_HOST_STORE").unwrap());
    let ready = PathBuf::from(std::env::var_os("PD_NATIVE_HOST_READY").unwrap());
    let address = std::env::var("PD_NATIVE_HOST_ADDRESS").unwrap();
    let mut usr1 =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()).unwrap();
    let mut term =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .finish(),
    )
    .unwrap();
    let store = Arc::new(open_isolated(&root, 40, 1).await.unwrap());
    if address.is_empty() {
        store
            .consensus()
            .initialize(BTreeSet::from([1]))
            .await
            .unwrap();
    }
    store
        .consensus()
        .wait(Some(Duration::from_secs(5)))
        .current_leader(1, "local voter")
        .await
        .unwrap();
    let listener = TcpListener::bind(if address.is_empty() {
        "127.0.0.1:0"
    } else {
        &address
    })
    .await
    .unwrap();
    let local = listener.local_addr().unwrap();
    let config = bootstrap_config(
        store.as_ref(),
        "native-host".into(),
        format!("http://{local}"),
        "http://127.0.0.1:2380".into(),
    )
    .await
    .unwrap();
    let token = CancellationToken::new();
    let mut pd = PdNodeBuilder::new(tokio::runtime::Handle::current(), token.clone())
        .start(
            store.clone(),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    pd.ready().await.unwrap();
    let publish = ready.with_extension("pending");
    fs::write(&publish, local.to_string()).unwrap();
    fs::rename(publish, &ready).unwrap();
    assert_eq!(usr1.recv().await, Some(()));
    tracing::info!("host received SIGUSR1");
    assert_eq!(term.recv().await, Some(()));
    pd.shutdown().await.unwrap();
    drop(pd);
    assert!(!token.is_cancelled());
    assert_eq!(tokio::spawn(async { 40 }).await.unwrap(), 40);
    Arc::try_unwrap(store).ok().unwrap().close().await.unwrap();
    tracing::info!("host closed native store");
}

#[tokio::test]
async fn theme10_native_single_binary_embedded_wal() {
    // The native_local_cli integration target covers the standalone binary.
    let root = tempfile::tempdir().unwrap();
    let mut first = start_host(root.path(), "first", None);
    let ready = root.path().join("first.ready");
    first.wait_for(|_| ready.is_file()).await;
    let address: SocketAddr = fs::read_to_string(ready).unwrap().parse().unwrap();
    let before = observe(address).await;
    let wal = root.path().join("store/metadata.sqlite-wal");
    assert!(fs::metadata(&wal).unwrap().len() > 0);
    first.assert_no_children();
    first.signal("-USR1");
    first
        .wait_for(|host| host.logs().contains("host received SIGUSR1"))
        .await;
    first.child.kill().unwrap();
    assert_eq!(first.wait().await.signal(), Some(9));
    assert!(wal.is_file());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());

    let mut reopened = start_host(root.path(), "reopen", Some(address));
    reopened
        .wait_for(|_| root.path().join("reopen.ready").is_file())
        .await;
    let after = observe(address).await;
    assert_eq!(after.0, before.0);
    assert!(after.1 > before.1);
    assert!(after.2 > before.2);
    println!(
        "native host WAL replay: cluster {}, ID {} -> {}, TSO {:?} -> {:?}",
        after.0, before.1, after.1, before.2, after.2
    );
    reopened.assert_no_children();
    reopened.signal("-USR1");
    reopened
        .wait_for(|host| host.logs().contains("host received SIGUSR1"))
        .await;
    reopened.signal("-TERM");
    assert!(reopened.wait().await.success(), "{}", reopened.logs());
    assert!(reopened.logs().contains("host closed native store"));
    drop(std::net::TcpListener::bind(address).unwrap());
}
