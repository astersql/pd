// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) fn fetch(url: &str, path: &str) -> (u16, String, String) {
    request(url, "GET", path, "")
}
fn request(url: &str, method: &str, path: &str, body: &str) -> (u16, String, String) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "-H",
            "X-Caller-ID: private-task086",
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let mut fields = text.rsplitn(3, '\n');
    let content = fields.next().unwrap().to_owned();
    let status = fields.next().unwrap().parse().unwrap();
    (status, fields.next().unwrap().into(), content)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let marker =
        prometheus::IntCounter::new("task086_host_total", "Host supplied collector").unwrap();
    registry.register(Box::new(marker.clone())).unwrap();
    marker.inc_by(7);
    // Process/runtime collectors belong to the embedding host, not a PD node.
    let capacity =
        prometheus::Gauge::new("pd_service_maxprocs", "The value of GOMAXPROCS.").unwrap();
    capacity.set(tokio::runtime::Handle::current().metrics().num_workers() as f64);
    registry.register(Box::new(capacity)).unwrap();
    let quota = prometheus::Gauge::new(
        "pd_service_memory_quota_bytes",
        "The value of memory quota bytes.",
    )
    .unwrap();
    quota.set(64.0 * 1024.0 * 1024.0); // Explicit caller-supplied test quota, not OS collection.
    registry.register(Box::new(quota)).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let mut config = astersql_pd_server_config::Config::default();
    config.pd_server.metric_storage = std::env::var("PD_METRICS_STORAGE_URL").unwrap_or_default();
    if let Ok(ca) = std::env::var("PD_METRICS_CA") {
        config.security.tls.ca_path = ca;
    }
    if let Ok(cert) = std::env::var("PD_METRICS_CERT") {
        config.security.tls.cert_path = cert;
        config.security.tls.key_path = std::env::var("PD_METRICS_KEY").unwrap();
    }

    if let Ok(address) = std::env::var("PD_METRICS_PUSH_ADDRESS") {
        config.metric.push_address = address;
        config.metric.push_job = std::env::var("PD_METRICS_PUSH_JOB").unwrap();
        config.metric.push_interval = astersql_pd_typeutil::Duration::from_nanos(100_000_000);
    }

    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_telemetry_config(config)
        .start(
            store,
            NodeConfig {
                cluster_id: 86,
                member_id: 86,
                name: "metrics".into(),
                client_url: url.clone(),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(fetch(&url, "/pd/api/v1/config").0, 200);
    assert_eq!(request(&url, "POST", "/pd/api/v1/config", "[").0, 400);
    for base in std::iter::once(url.clone()).chain(std::env::var("PD_METRICS_GO_URL").ok()) {
        let output = std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/service-metrics-probe.py"
            ))
            .arg(base)
            .output()
            .unwrap();
        print!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let observation = fetch(&url, "/metrics");
    assert!(observation.1.contains("pd_service_audit_requests_total{caller_component=\"anonymous\",method=\"HTTP\",service=\"GetConfig\"} 2"), "{}", observation.1);
    assert!(observation.1.contains("service=\"SetConfig\"} 2"));
    assert!(!observation.1.contains("private-task086"));
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        observation.0, 200,
        "host Registry must be exported: {observation:?}"
    );
    assert!(observation.1.contains("task086_host_total 7"));
    assert!(
        observation
            .1
            .contains("pd_service_memory_quota_bytes 67108864")
    );
    assert!(observation.1.contains("# TYPE pd_server_info gauge"));
    assert!(observation.2.starts_with("text/plain; version=0.0.4"));
    assert_eq!(
        registry.gather().len(),
        3,
        "host collectors survive shutdown"
    );
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}

#[test]
fn task_086_registration_rollback() {
    use crate::service_metrics::ServiceMetrics;
    let registry = Arc::new(prometheus::Registry::new());
    let host = prometheus::IntCounterVec::new(
        prometheus::Opts::new(
            "pd_service_audit_requests_total",
            "Total number of service requests for audit",
        ),
        &["service", "method", "caller_component"],
    )
    .unwrap();
    host.with_label_values(&["host", "HTTP", "anonymous"])
        .inc_by(19);
    registry.register(Box::new(host.clone())).unwrap();
    assert!(ServiceMetrics::register(registry.clone()).is_err());
    let families = registry.gather();
    assert_eq!(families.len(), 1, "partial registrations must roll back");
    assert_eq!(families[0].get_metric()[0].get_counter().get_value(), 19.0);
    registry.unregister(Box::new(host)).unwrap();
    let scope = ServiceMetrics::register(registry.clone()).unwrap();
    let registered = registry.gather();
    assert!(ServiceMetrics::register(registry.clone()).is_err());
    assert_eq!(registry.gather(), registered);
    drop(scope);
    assert!(registry.gather().is_empty());
    drop(ServiceMetrics::register(registry.clone()).unwrap());
    assert!(registry.gather().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_startup_retry_and_cancel() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let registry = Arc::new(prometheus::Registry::new());
    let host = prometheus::IntCounterVec::new(
        prometheus::Opts::new(
            "pd_service_audit_requests_total",
            "Total number of service requests for audit",
        ),
        &["service", "method", "caller_component"],
    )
    .unwrap();
    host.with_label_values(&["host", "HTTP", "anonymous"])
        .inc_by(19);
    registry.register(Box::new(host.clone())).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let config = NodeConfig {
        cluster_id: 8686,
        member_id: 86,
        name: "retry".into(),
        client_url: url.clone(),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let before = store
        .read(Read::prefix(b"/pd/8686/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(store.clone(), config.clone(), listener, registry.clone())
            .await
            .is_err()
    );
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/pd/8686/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    assert_eq!(registry.gather().len(), 1);
    registry.unregister(Box::new(host)).unwrap();
    for _ in 0..2 {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let cancel = CancellationToken::new();
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(store.clone(), config.clone(), listener, registry.clone())
            .await
            .unwrap();
        node.ready().await.unwrap();
        assert!(fetch(&url, "/metrics").1.contains("pd_server_info"));
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), node.shutdown())
            .await
            .unwrap()
            .unwrap();
        assert!(registry.gather().is_empty());
        assert!(tokio::net::TcpStream::connect(address).await.is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_086_push_retry_cancel() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_address = gateway.local_addr().unwrap();
    let (arrived, waiting) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        for attempt in 0..3 {
            let (mut socket, _) = gateway.accept().await.unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let b = socket.read_u8().await.unwrap();
                bytes.push(b);
                if bytes.ends_with(b"\r\n\r\n") {
                    break bytes.len();
                }
            };
            let headers = String::from_utf8(bytes.clone()).unwrap();
            assert!(
                headers.starts_with("PUT /metrics/job@base64/dGVuYW50L25hbWU/instance/"),
                "{headers}"
            );
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("content-type: application/vnd.google.protobuf")
            );
            let length: usize = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            bytes.resize(end + length, 0);
            socket.read_exact(&mut bytes[end..]).await.unwrap();
            assert!(
                bytes[end..]
                    .windows(b"task086_host_total".len())
                    .any(|w| w == b"task086_host_total")
            );
            assert!(
                bytes[end..]
                    .windows(b"pd_server_info".len())
                    .any(|w| w == b"pd_server_info")
            );
            if attempt == 2 {
                arrived.send(()).unwrap();
                let mut b = [0];
                assert_eq!(
                    tokio::time::timeout(Duration::from_secs(5), socket.read(&mut b))
                        .await
                        .unwrap()
                        .unwrap(),
                    0,
                    "host cancellation must close pending push"
                );
                return;
            }
            socket
                .write_all(if attempt == 0 {
                    b"HTTP/1.1 500 Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                })
                .await
                .unwrap();
        }
    });
    let fixture = crate::etcd_fixture::Fixture::start();
    let registry = Arc::new(prometheus::Registry::new());
    let host =
        prometheus::IntCounter::new("task086_host_total", "Host supplied collector").unwrap();
    host.inc_by(7);
    registry.register(Box::new(host)).unwrap();
    let mut cfg = astersql_pd_server_config::Config::default();
    cfg.metric.push_job = "tenant/name".into();
    cfg.metric.push_address = gateway_address.to_string();
    cfg.metric.push_interval = astersql_pd_typeutil::Duration::from_nanos(50_000_000);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_listener_config(cfg)
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 8688,
                member_id: 86,
                name: "push".into(),
                client_url: format!("http://{address}"),
                peer_url: "http://127.0.0.1:2380".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();
    assert_eq!(registry.gather().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn task_086_wall_clock_and_cancel() {
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Clock(AtomicU64);
    impl crate::service_metrics::WallClock for Clock {
        fn now(&self) -> std::time::SystemTime {
            std::time::UNIX_EPOCH + Duration::from_millis(self.0.load(Ordering::SeqCst))
        }
    }
    let registry = Arc::new(prometheus::Registry::new());
    let metrics = crate::service_metrics::ServiceMetrics::register(registry.clone()).unwrap();
    let clock = Arc::new(Clock(AtomicU64::new(100)));
    let cancel = CancellationToken::new();
    let worker = tokio::spawn(
        metrics
            .clone()
            .run_worker_with_clock(cancel.clone(), clock.clone()),
    );
    tokio::task::yield_now().await;
    clock.0.store(50, Ordering::SeqCst);
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    let value = || {
        registry
            .gather()
            .iter()
            .find(|f| f.name() == "pd_monitor_time_jump_back_total")
            .unwrap()
            .get_metric()[0]
            .get_counter()
            .get_value()
    };
    assert_eq!(value(), 1.0);
    clock.0.store(150, Ordering::SeqCst);
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert_eq!(value(), 1.0);
    cancel.cancel();
    worker.await.unwrap();
    drop(metrics);
    assert!(registry.gather().is_empty());
}
#[tokio::test]
async fn task_086_destination_policy() {
    use crate::metric_transport::{Target, permitted};
    for raw in [
        "",
        "file:///tmp/a",
        "http://user:secret@example.com",
        "http://example.com:",
        "http://example.com:70000",
        "http://example.com:0",
        "http://example.com/#secret",
    ] {
        assert!(Target::parse(raw).is_err(), "{raw}");
    }
    for ip in [
        "127.0.0.1",
        "::1",
        "::ffff:127.0.0.1",
        "0.0.0.0",
        "::",
        "169.254.169.254",
        "fe80::1",
        "224.0.0.1",
        "240.0.0.1",
        "100.100.100.200",
        "192.0.0.192",
        "fd00:ec2::254",
        "fd20:ce::254",
    ] {
        assert!(!permitted(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["192.168.1.1", "10.0.0.1", "2001:db8::1"] {
        assert!(permitted(ip.parse().unwrap()), "{ip}");
    }
    assert!(
        Target::parse("http://localhost:9090")
            .unwrap()
            .addresses(true)
            .await
            .is_err()
    );
}

#[test]
fn task_086_store_label_cleanup() {
    let registry = Arc::new(prometheus::Registry::new());
    let metrics = crate::service_metrics::ServiceMetrics::register(registry.clone()).unwrap();
    metrics.stores.counter(1, "old-address", false, "recv");
    metrics.stores.observe(1, "old-address", "region", 0.1);
    metrics.stores.counter(1, "new-address", false, "recv");
    metrics.stores.counter(2, "another-store", true, "err");
    let encoder = prometheus::TextEncoder::new();
    let mut bytes = Vec::new();
    prometheus::Encoder::encode(&encoder, &registry.gather(), &mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("old-address"));
    assert!(text.contains("new-address"));
    metrics.retain_stores([2]);
    let mut bytes = Vec::new();
    prometheus::Encoder::encode(&encoder, &registry.gather(), &mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("new-address"));
    assert!(text.contains("another-store"));
    drop(metrics);
    assert!(registry.gather().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_086_disabled_push_has_no_io() {
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    for case in 0..3 {
        let mut config = astersql_pd_server_config::Config::default();
        config.metric.push_address = if case == 0 {
            String::new()
        } else {
            gateway.local_addr().unwrap().to_string()
        };
        config.metric.push_job = if case == 2 {
            String::new()
        } else {
            "disabled".into()
        };
        config.metric.push_interval =
            astersql_pd_typeutil::Duration::from_nanos(if case == 1 { 0 } else { 1_000_000 });
        let cancel = CancellationToken::new();
        let worker = tokio::spawn(crate::service_metrics::push_worker(
            Arc::new(prometheus::Registry::new()),
            Arc::new(config),
            "host".into(),
            cancel.clone(),
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), gateway.accept())
                .await
                .is_err()
        );
        cancel.cancel();
        worker.await.unwrap();
    }
}
