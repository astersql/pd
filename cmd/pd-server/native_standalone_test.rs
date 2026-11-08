// Copyright 2026 AsterSQL.

use std::{
    collections::BTreeSet, fs, os::unix::process::ExitStatusExt, path::PathBuf, sync::Arc,
    time::Duration,
};

use astersql_pd_storage::MetaStore;
use astersql_pd_storage_kv::open_isolated;

use crate::classic_process::{prepare_shutdown_signal, run_with_backend};
use crate::native_process_test_support::{Host, observe};

const FIXTURE: &str = "native_standalone_test::run_standalone_fixture";

#[test]
#[ignore = "test subprocess driven by theme10_native_cli_test_target"]
fn run_standalone_fixture() {
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .finish(),
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let path = PathBuf::from(std::env::var_os("PD_NATIVE_HOST_STORE").unwrap());
        let address = std::env::var("PD_NATIVE_HOST_ADDRESS").unwrap();
        let fresh = !path.exists();
        let shutdown = prepare_shutdown_signal().unwrap();
        let native = Arc::new(open_isolated(&path, 28, 1).await.unwrap());
        if fresh {
            native
                .consensus()
                .initialize(BTreeSet::from([1]))
                .await
                .unwrap();
        }
        native
            .consensus()
            .wait(Some(Duration::from_secs(5)))
            .current_leader(1, "standalone fixture")
            .await
            .unwrap();
        let config = crate::cli_config::parse(
            [
                "--name=native-standalone-test".to_owned(),
                format!("--client-urls=http://{address}"),
            ]
            .into_iter(),
        )
        .unwrap();
        assert!(!config.config_check);
        let store: Arc<dyn MetaStore> = native.clone();
        let events = run_with_backend(config, shutdown, None, |endpoint| async move {
            assert!(
                endpoint.is_empty(),
                "fixture must not connect to external etcd"
            );
            Ok(store)
        })
        .await
        .unwrap();
        assert_eq!(events.last(), Some(&"storage-released"));
        Arc::try_unwrap(native).ok().unwrap().close().await.unwrap();
        tracing::info!("standalone fixture released native store");
    });
}

#[tokio::test]
async fn theme10_native_cli_test_target() {
    let root = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut first = Host::start(root.path(), "first", Some(address), FIXTURE);
    first
        .wait_for(|host| host.logs().contains("PD ready"))
        .await;
    let before = observe(address).await;
    first.assert_no_children();
    let wal = root.path().join("store/metadata.sqlite-wal");
    assert!(fs::metadata(&wal).unwrap().len() > 0);
    first.child.kill().unwrap();
    assert_eq!(first.wait().await.signal(), Some(9));
    assert!(wal.is_file());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());

    let mut reopened = Host::start(root.path(), "reopen", Some(address), FIXTURE);
    reopened
        .wait_for(|host| host.logs().contains("PD ready"))
        .await;
    let after = observe(address).await;
    assert_eq!(after.0, before.0);
    assert!(after.1 > before.1 && after.2 > before.2);
    reopened.assert_no_children();
    reopened.signal("-TERM");
    assert!(reopened.wait().await.success(), "{}", reopened.logs());
    let log = reopened.logs();
    assert!(log.contains("PD stopped") && log.contains("standalone fixture released native store"));
    drop(std::net::TcpListener::bind(address).unwrap());
    println!(
        "pd-server test target: cluster {}, ID {} -> {}, TSO {:?} -> {:?}, WAL survived SIGKILL",
        after.0, before.1, after.1, before.2, after.2
    );
}
