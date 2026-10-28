// Copyright 2026 AsterSQL.
use crate::{ClusterMetricsWorker, HealthMember};
use prometheus::Registry;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn task_015_behavior() {
    let healthy = Arc::new(AtomicBool::new(true));
    let h = healthy.clone();
    let app = axum::Router::new().route(
        "/pd/api/v1/ping",
        axum::routing::get(move |headers: axum::http::HeaderMap| {
            let h = h.clone();
            async move {
                assert_eq!(headers["PD-Allow-follower-handle"], "true");
                if h.load(Ordering::SeqCst) {
                    axum::http::StatusCode::OK
                } else {
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let registry = Registry::new();
    let cancel = CancellationToken::new();
    let worker = ClusterMetricsWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        &registry,
        move || {
            let url = url.clone();
            async move {
                Ok(vec![HealthMember {
                    name: "pd1".into(),
                    client_urls: vec!["invalid://unreachable".into(), url],
                }])
            }
        },
    )
    .unwrap();
    tokio::task::yield_now().await;
    assert!(registry.gather().is_empty());
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::time::resume();
    for _ in 0..100 {
        if !registry.gather().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        registry.gather()[0].get_metric()[0].get_gauge().get_value(),
        1.,
        "real HTTP healthy member must update registry"
    );
    healthy.store(false, Ordering::SeqCst);
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::time::resume();
    for _ in 0..100 {
        if registry.gather()[0].get_metric()[0].get_gauge().get_value() == 0. {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        registry.gather()[0].get_metric()[0].get_gauge().get_value(),
        0.
    );
    worker.stop().await.unwrap();
    assert!(registry.gather().is_empty());
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn task_015_members_error_and_registration() {
    let registry = Registry::new();
    let unrelated = prometheus::IntGauge::new("unrelated", "host metric").unwrap();
    registry.register(Box::new(unrelated)).unwrap();
    let cancel = CancellationToken::new();
    let worker = ClusterMetricsWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        &registry,
        || async { Err("membership unavailable".into()) },
    )
    .unwrap();
    assert!(
        ClusterMetricsWorker::start(
            &tokio::runtime::Handle::current(),
            &cancel,
            &registry,
            || async { Ok(vec![]) }
        )
        .is_err()
    );
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::task::yield_now().await;
    worker.stop().await.unwrap();
    assert_eq!(registry.gather().len(), 1);
    assert_eq!(registry.gather()[0].name(), "unrelated");
}

#[tokio::test]
async fn task_015_cancel_inflight() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let registry = Registry::new();
    let cancel = CancellationToken::new();
    let worker = ClusterMetricsWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        &registry,
        move || {
            let url = url.clone();
            async move {
                Ok(vec![HealthMember {
                    name: "blocked".into(),
                    client_urls: vec![url],
                }])
            }
        },
    )
    .unwrap();
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::time::resume();
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut buffer = [0; 1024];
    assert!(socket.read(&mut buffer).await.unwrap() > 0);
    worker.stop().await.unwrap();
    assert_eq!(socket.read(&mut buffer).await.unwrap(), 0);
    assert!(registry.gather().is_empty());
    let _ = socket.shutdown().await;
}

#[tokio::test]
async fn task_015_timeout_marks_unhealthy() {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let registry = Registry::new();
    let worker = ClusterMetricsWorker::start(
        &tokio::runtime::Handle::current(),
        &CancellationToken::new(),
        &registry,
        move || {
            let url = url.clone();
            async move {
                Ok(vec![HealthMember {
                    name: "timeout".into(),
                    client_urls: vec![url],
                }])
            }
        },
    )
    .unwrap();
    tokio::task::yield_now().await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(10)).await;
    tokio::time::resume();
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut buffer = [0; 1024];
    assert!(socket.read(&mut buffer).await.unwrap() > 0);
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    tokio::time::resume();
    assert_eq!(socket.read(&mut buffer).await.unwrap(), 0);
    assert_eq!(
        registry.gather()[0].get_metric()[0].get_gauge().get_value(),
        0.
    );
    worker.stop().await.unwrap();
    assert!(registry.gather().is_empty());
}
