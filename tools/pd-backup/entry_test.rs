// Copyright 2026 AsterSQL.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
    sync::Arc,
};

use astersql_pd_backup::BackupInfo;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_runtime::{PdNodeBuilder, bootstrap_config};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use tokio_util::sync::CancellationToken;

#[allow(dead_code)]
#[path = "../../pkg/storage/kv/etcd_fixture_test.rs"]
mod fixture;

fn backup(url: &str, etcd: &str, output: &Path, tls: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pd-backup"))
        .args(["-pd", url, "--etcd-endpoints", etcd, "-file"])
        .arg(output)
        .args(tls)
        .output()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ops_091_entry_plaintext() {
    scenario(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ops_091_entry_mtls() {
    scenario(true).await;
}

async fn scenario(mtls: bool) {
    let dir = tempfile::tempdir().unwrap();
    let mut listener_config = astersql_pd_server_config::Config::default();
    let mut tls = vec![];
    let fixture = if mtls {
        assert!(
            Command::new("python3")
                .args(["../../scripts/service-auth-oracle.py", "--certificates"])
                .arg(dir.path())
                .status()
                .unwrap()
                .success()
        );
        let path = |name: &str| dir.path().join(name).display().to_string();
        listener_config.security.tls.ca_path = path("ca.pem");
        listener_config.security.tls.cert_path = path("server.pem");
        listener_config.security.tls.key_path = path("server.key");
        tls = vec![
            format!("-cacert={}", path("ca.pem")),
            format!("-cert={}", path("allowed.pem")),
            format!("-key={}", path("allowed.key")),
        ];
        fixture::Fixture::start_with_tls(dir.path())
    } else {
        fixture::Fixture::start()
    };
    let store = Arc::new(if mtls {
        let credentials = tonic::transport::ClientTlsConfig::new()
            .ca_certificate(tonic::transport::Certificate::from_pem(
                fs::read(dir.path().join("ca.pem")).unwrap(),
            ))
            .identity(tonic::transport::Identity::from_pem(
                fs::read(dir.path().join("allowed.pem")).unwrap(),
                fs::read(dir.path().join("allowed.key")).unwrap(),
            ));
        EtcdMetaStore::connect_with_transport_factory(
            std::slice::from_ref(&fixture.endpoint),
            std::time::Duration::from_secs(3),
            |endpoints| {
                Ok(
                    tonic::transport::Endpoint::from_shared(endpoints[0].clone())
                        .unwrap()
                        .tls_config(credentials.clone())
                        .unwrap()
                        .connect_lazy(),
                )
            },
        )
        .await
        .unwrap()
    } else {
        fixture.store().await
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let scheme = if mtls { "https" } else { "http" };
    let url = format!("{scheme}://{}", listener.local_addr().unwrap());
    let config = bootstrap_config(
        store.as_ref(),
        "backup".into(),
        url.clone(),
        "http://127.0.0.1:2380".into(),
    )
    .await
    .unwrap();
    let paths = ClusterPaths::new(config.cluster_id);
    let mut pd = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_listener_config(listener_config)
        .start(
            store.clone(),
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    pd.ready().await.unwrap();
    let file = dir.path().join("backup.json");
    let output = backup(&url, &fixture.endpoint, &file, &tls);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end(),
        format!("pd backup successful! dump file is: {}", file.display())
    );
    let bytes = fs::read(&file).unwrap();
    let data: BackupInfo = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(data.cluster_id, config.cluster_id);
    let alloc = store
        .read(Read::range(
            paths.alloc_id_path().as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(alloc.entries[0].value, data.alloc_id_max.to_be_bytes());
    assert!(data.alloc_timestamp_max > 0);
    assert_eq!(data.config["name"], "backup");
    if mtls {
        let failure = backup(&url, &fixture.endpoint, &file, &tls[..1]);
        assert_eq!(failure.status.code(), Some(1));
        assert!(!failure.stderr.is_empty());
        assert_eq!(fs::read(&file).unwrap(), bytes);
    }
    let failure = backup(&url, &fixture.endpoint, dir.path(), &tls);
    assert_eq!(failure.status.code(), Some(1));
    assert!(!failure.stderr.is_empty());
    assert!(dir.path().is_dir());
    #[cfg(unix)]
    if !mtls {
        use std::os::unix::fs::PermissionsExt;

        let parent = dir.path().join("read-only");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o500)).unwrap();
        let target = parent.join("backup.json");
        let denied = backup(&url, &fixture.endpoint, &target, &[]);
        // A privileged test runner may bypass directory mode bits.
        let probe = fs::write(parent.join("probe"), b"");
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        if probe.is_err_and(|err| err.kind() == std::io::ErrorKind::PermissionDenied) {
            assert_eq!(denied.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&denied.stderr).contains("Permission denied"));
            assert_eq!(fs::read_dir(parent).unwrap().count(), 0);
            println!("pd-backup: output permission denied, directory empty");
        }
    }
    pd.shutdown().await.unwrap();
}
