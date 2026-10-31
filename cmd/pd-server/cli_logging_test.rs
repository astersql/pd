// Copyright 2026 AsterSQL.
use crate::cli_logging::{Writer, subscriber};
use astersql_pd_logutil::{LogConfig, LogLevelControl};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn logger() -> (
    impl tracing::Subscriber + Send + Sync,
    Arc<crate::cli_logging::LogControl>,
    Capture,
) {
    let capture = Capture::default();
    let config = LogConfig {
        level: "info".into(),
        format: "json".into(),
        disable_timestamp: true,
        disable_caller: true,
        ..Default::default()
    };
    let (subscriber, control) = subscriber(
        &config,
        Writer(Arc::new(Mutex::new(Box::new(capture.clone())))),
    )
    .unwrap();
    (subscriber, control, capture)
}
fn debug_event() {
    tracing::debug!(target: "pd", "debug event");
}
fn error_event() {
    tracing::error!(target: "pd", "error event");
}
#[test]
fn task_092_structured_hot_filter_and_secrets() {
    let (subscriber, control, capture) = logger();
    tracing::subscriber::with_default(subscriber, || {
        debug_event();
        tracing::info!(target: "pd", "ready");
        tracing::error!(target: "transport", endpoint="http://user:password@localhost", token="sensitive-token", "unsafe dependency error");
        control.set_level("DeBuG").unwrap();
        debug_event();
        assert!(control.set_level("trace").is_err());
        debug_event();
        control.set_level("fatal").unwrap();
        error_event();
        control.set_level("warning").unwrap();
        error_event();
    });
    let output = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(!output.contains("password") && !output.contains("sensitive-token"));
    let events: Vec<serde_json::Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["fields"]["message"] == "debug event")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["fields"]["message"] == "error event")
            .count(),
        1
    );
    assert!(
        events
            .iter()
            .any(|e| e["level"] == "INFO" && e["fields"]["message"] == "ready")
    );
    assert!(events.iter().all(|e| e.get("timestamp").is_none()));
}
async fn post(address: std::net::SocketAddr, body: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream.write_all(format!("POST /pd/api/v1/admin/log HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}
#[test]
fn task_092_behavior() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (subscriber, control, capture) = logger();
    let _host_guard = tracing::subscriber::set_default(subscriber);
    runtime.block_on(async {
        for host_control in [false, true] {
            let store = Arc::new(astersql_pd_storage::MemoryStore::default());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let identity = astersql_pd_runtime::bootstrap_config(
                store.as_ref(),
                "logging".into(),
                format!("http://{address}"),
                "http://127.0.0.1:2380".into(),
            )
            .await
            .unwrap();
            let builder = astersql_pd_runtime::PdNodeBuilder::new(
                tokio::runtime::Handle::current(),
                tokio_util::sync::CancellationToken::new(),
            );
            let builder = if host_control {
                builder.with_log_level_control(control.clone())
            } else {
                builder
            };
            let mut node = builder
                .start(
                    store.clone(),
                    identity,
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
            node.ready().await.unwrap();
            use astersql_pd_storage::{Consistency, MetaStore, Read};
            let before = store
                .read(Read::prefix(b"/", Consistency::Linearizable))
                .await
                .unwrap();
            let response = post(address, "\"debug\"").await;
            assert!(
                response.starts_with(if host_control {
                    "HTTP/1.1 200"
                } else {
                    "HTTP/1.1 503"
                }),
                "{response}"
            );
            if host_control {
                assert!(response.contains("The log level is updated."));
                debug_event();
                for invalid in [
                    "\"trace\"",
                    "\"\"",
                    "null",
                    "{}",
                    "12",
                    "[1]",
                    "\"unterminated",
                    "",
                ] {
                    let response = post(address, invalid).await;
                    assert!(
                        response.starts_with("HTTP/1.1 400"),
                        "{invalid}: {response}"
                    );
                }
                let large_invalid = format!("\"{}\"", "x".repeat(1024 * 1024));
                assert!(post(address, &large_invalid).await.starts_with("HTTP/1.1 400"));
                debug_event();
                for valid in [
                    "\"warning\"",
                    "\"WARN\"",
                    "\"fatal\"",
                    "\"error\"",
                    "\"info\"",
                ] {
                    assert!(post(address, valid).await.starts_with("HTTP/1.1 200"));
                }
                assert!(post(address, "\"debug\"").await.starts_with("HTTP/1.1 200"));
            }
            let after = store
                .read(Read::prefix(b"/", Consistency::Linearizable))
                .await
                .unwrap();
            assert_eq!(
                before
                    .entries
                    .iter()
                    .filter(|e| e.key.ends_with(b"/config"))
                    .collect::<Vec<_>>(),
                after
                    .entries
                    .iter()
                    .filter(|e| e.key.ends_with(b"/config"))
                    .collect::<Vec<_>>(),
                "log changes must not persist configuration"
            );
            if host_control {
                let follower_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let follower_address = follower_listener.local_addr().unwrap();
                let follower_identity = astersql_pd_runtime::bootstrap_config(store.as_ref(), "logging-follower".into(), format!("http://{follower_address}"), "http://127.0.0.1:2381".into()).await.unwrap();
                let mut follower = astersql_pd_runtime::PdNodeBuilder::new(tokio::runtime::Handle::current(), tokio_util::sync::CancellationToken::new())
                    .with_log_level_control(control.clone())
                    .start(store.clone(), follower_identity, follower_listener, Arc::new(prometheus::Registry::new())).await.unwrap();
                follower.ready().await.unwrap();
                assert!(follower.try_cluster().is_none(), "second member must be an actual follower");
                let response = post(follower_address, "\"debug\"").await;
                assert!(response.starts_with("HTTP/1.1 200"), "follower must change its host's local level without redirecting: {response}");
                follower.shutdown().await.unwrap();
                assert!(tokio::net::TcpStream::connect(follower_address).await.is_err());
                // Wait for actual body polling (100 Continue), then cancel while
                // the complete level is still absent. No threshold may publish.
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                control.set_level("error").unwrap();
                let mut pending = tokio::net::TcpStream::connect(address).await.unwrap();
                pending.write_all(b"POST /pd/api/v1/admin/log HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n").await.unwrap();
                let mut interim = [0; 25];
                pending.read_exact(&mut interim).await.unwrap();
                assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");
                tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown()).await.unwrap().unwrap();
                let mut final_response = String::new();
                pending.read_to_string(&mut final_response).await.unwrap();
                assert!(final_response.starts_with("HTTP/1.1 500"), "{final_response}");
                debug_event(); // Still error: canceled request did not change it.
                control.set_level("debug").unwrap();
            } else {
                node.shutdown().await.unwrap();
            }
            assert!(tokio::net::TcpStream::connect(address).await.is_err());
        }
        debug_event(); // Host subscriber is still installed after embedded shutdown.
    });
    let output = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert_eq!(
        output
            .lines()
            .filter(|line| line.contains("debug event"))
            .count(),
        3
    );
}
#[test]
fn task_092_initialization_errors() {
    let capture = Capture::default();
    for config in [
        LogConfig {
            level: "trace".into(),
            ..Default::default()
        },
        LogConfig {
            level: "info".into(),
            format: "invalid".into(),
            ..Default::default()
        },
    ] {
        assert!(
            subscriber(
                &config,
                Writer(Arc::new(Mutex::new(Box::new(capture.clone()))))
            )
            .is_err()
        );
    }
    let config = LogConfig {
        level: "info".into(),
        file: astersql_pd_logutil::FileLogConfig {
            filename: "/dev/null/pd.log".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(crate::cli_logging::init(&config).is_err());
}

#[test]
fn task_092_config_secret() {
    let path = std::env::temp_dir().join(format!("pd-log-secret92-{}.toml", std::process::id()));
    std::fs::write(&path, "name = [\"TASK092-CONFIG-SECRET\" invalid]\n").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "cli_exit_test::task_091_process_entry",
            "--nocapture",
            "--quiet",
        ])
        .env(
            "PD_TASK091_CHILD_ARGS",
            format!("--config={}", path.display()),
        )
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(result.status.code(), Some(0));
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("configuration could not be loaded or validated"));
    assert!(!output.contains("TASK092-CONFIG-SECRET"));
    assert!(result.stderr.is_empty());
}
