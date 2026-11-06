// Copyright 2026 AsterSQL.
use super::{BaseServer, auxiliary_routes};
use astersql_pd_proto::{
    diagnosticspb::{SearchLogRequest, diagnostics_client::DiagnosticsClient},
    health::{HealthCheckRequest, health_client::HealthClient},
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct LogFile(std::path::PathBuf);
impl Drop for LogFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[tokio::test]
async fn common_listener_reads_host_log_and_releases_retained_health_watch() {
    let path = LogFile(std::env::temp_dir().join(format!(
        "mcs-common-{}-{}.log",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::write(&path.0,"[2020/01/01 00:00:00.000 +00:00] [INFO] [chosen-42]\n[2020/01/01 00:00:00.001 +00:00] [INFO] [excluded]\n").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let runtime = tokio::runtime::Handle::current();
    let cancel = CancellationToken::new();
    let log_file = path.0.display().to_string();
    let mut server = BaseServer::start(
        runtime.clone(),
        &cancel,
        listener,
        move |cancel| Ok(auxiliary_routes(runtime, cancel, log_file).into_axum_router()),
        vec![],
    )
    .await
    .unwrap();
    let mut logs = DiagnosticsClient::connect(url.clone()).await.unwrap();
    let mut stream = logs
        .search_log(SearchLogRequest {
            start_time: 1577836800000,
            end_time: 1577836800001,
            patterns: vec![r"\Qchosen-\E\d+".into()],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let response = stream.message().await.unwrap().unwrap();
    assert_eq!(response.messages.len(), 1);
    assert_eq!(response.messages[0].message, "[chosen-42]");
    assert!(stream.message().await.unwrap().is_none());
    let mut health = HealthClient::connect(url).await.unwrap();
    let mut watch = health
        .watch(HealthCheckRequest {
            service: "missing".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(watch.message().await.unwrap().unwrap().status, 3);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), watch.message())
            .await
            .is_err()
    );
    assert_eq!(
        health
            .check(HealthCheckRequest::default())
            .await
            .unwrap()
            .into_inner()
            .status,
        1
    );
    tokio::time::timeout(Duration::from_secs(2), server.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(health.check(HealthCheckRequest::default()).await.is_err());
    assert!(watch.message().await.is_err());
}
