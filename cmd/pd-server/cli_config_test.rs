// Copyright 2026 AsterSQL.
use crate::entry::Config;

#[test]
fn task_090_behavior() {
    let path = std::env::temp_dir().join(format!("pd-cli90-{}.toml", std::process::id()));
    std::fs::write(
        &path,
        "name = 'from-file'\nclient-urls = 'http://127.0.0.1:3456'\n",
    )
    .unwrap();
    let result = Config::parse(
        [
            "--etcd-endpoints=http://127.0.0.1:2379".into(),
            format!("--config={}", path.display()),
            "--name=from-cli".into(),
        ]
        .into_iter(),
    );
    std::fs::remove_file(path).unwrap();
    assert!(
        result.is_ok(),
        "configuration file must merge: {:?}",
        result.err()
    );
    let config = result.unwrap();
    assert_eq!(config.name, "from-cli");
    assert_eq!(config.client, "http://127.0.0.1:3456");
}

fn file_config(input: &str, extra: &[&str]) -> Result<Config, crate::entry::ProcessError> {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "pd-cli90-{}-{}.toml",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, input).unwrap();
    let result = Config::parse(
        std::iter::once(format!("--config={}", path.display()))
            .chain(extra.iter().map(|s| s.to_string())),
    );
    std::fs::remove_file(path).unwrap();
    result
}

#[test]
fn task_090_precedence_defaults_deprecated_and_errors() {
    let config = file_config(
        "name='file'\nlog-level='debug'\nlog-file='old.log'\n[log]\nlevel='warn'\n",
        &["--config-check", "--name", "", "-L", "error"],
    )
    .unwrap();
    assert_eq!(config.name, "file");
    assert_eq!(config.security.log.level, "error");
    assert_eq!(config.security.log.file.filename, "old.log");
    assert_eq!(config.security.warning_msgs.len(), 2);
    assert_eq!(config.client, "http://127.0.0.1:2379");
    assert_eq!(
        config.advertise_client.as_deref(),
        Some(config.client.as_str())
    );
    assert_eq!(
        config.security.scalars.data_dir,
        std::env::current_dir()
            .unwrap()
            .join("default.file")
            .to_str()
            .unwrap()
    );
    let tls = file_config(
        "[security]\ncacert-path='file-ca'\ncert-path='file-cert'\nkey-path='file-key'\n",
        &[
            "--config-check",
            "--cacert=cli-ca",
            "--cert=",
            "--metrics-addr=http://metrics:9091",
        ],
    )
    .unwrap();
    assert_eq!(tls.security.security.tls.ca_path, "cli-ca");
    assert_eq!(tls.security.security.tls.cert_path, "file-cert");
    assert_eq!(tls.security.security.tls.key_path, "file-key");
    assert_eq!(tls.security.metric.push_address, "http://metrics:9091");
    let config = file_config(
        "ForceNewCluster=true\n",
        &["--config-check=true", "--force-new-cluster=false"],
    )
    .unwrap();
    assert!(config.security.scalars.force_new_cluster);
    assert_eq!(config.unsupported, ["force-new-cluster"]);
    assert!(file_config("name=[", &["--config-check"]).is_err());
    assert!(
        file_config(
            "join='http://a:2379'\ninitial-cluster='a=http://a:2380'",
            &["--config-check"]
        )
        .is_err()
    );
    let dash_name = file_config("", &["--config-check", "--name", "-name"]).unwrap();
    assert_eq!(dash_name.name, "-name");
    assert!(file_config("", &["--config-check=no", "--config-check=true"]).is_err());
    assert!(file_config("", &["--config-check=no"]).is_err());
    assert!(file_config("", &["--config-check", "--unknown=1"]).is_err());
    assert!(file_config("", &["--name"]).is_err());
    assert!(
        Config::parse(
            ["--config=/path/does/not/exist", "--config-check"]
                .into_iter()
                .map(String::from)
        )
        .is_err()
    );
    let config = file_config("", &["--config-check"]).unwrap();
    let host = rustix::system::uname();
    assert_eq!(
        config.name,
        format!("pd-{}", host.nodename().to_string_lossy())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_090_file_runtime_and_shutdown() {
    use crate::main_test::fixture;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let fixture = fixture::Fixture::start();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let input = format!(
        "name='file-runtime90'\nclient-urls='http://{address}'\npeer-urls='http://127.0.0.1:2380'\n"
    );
    let endpoint = format!("--etcd-endpoints={}", fixture.endpoint);
    let config = file_config(&input, &[&endpoint]).unwrap();
    let store = fixture.store().await;
    let stopped = async {
        for _ in 0..100 {
            let output = std::process::Command::new("curl")
                .args([
                    "-fsS",
                    "--max-time",
                    "1",
                    &format!("http://{address}/pd/api/v1/members"),
                ])
                .output()
                .unwrap();
            if output.status.success() {
                let members: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(members["members"][0]["name"], "file-runtime90");
                assert_eq!(
                    members["members"][0]["client_urls"][0],
                    format!("http://{address}")
                );
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }
        panic!("file-configured HTTP listener never ready");
    };
    crate::entry::run_until(config, stopped).await.unwrap();
    tokio::net::TcpListener::bind(address).await.unwrap();
    let persisted = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(!persisted.entries.is_empty());
    let before = persisted.entries;
    let rejected = file_config("data-dir='unsupported90'", &[&endpoint]).unwrap();
    let error = crate::entry::run_until(rejected, async { Ok(()) })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cannot apply: data-dir"));
    assert_eq!(
        store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        before
    );
    assert!(!std::path::Path::new("unsupported90").exists());
}
