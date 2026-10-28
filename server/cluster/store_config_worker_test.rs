// Copyright 2026 AsterSQL.
use super::fetch_store_config;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn task_014_behavior() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/config",
                axum::routing::get(|| async { r#"{"coprocessor":{"region-max-size":"256MiB"}}"# }),
            ),
        )
        .await
        .unwrap();
    });
    let result = fetch_store_config(&address.to_string(), &CancellationToken::new()).await;
    server.abort();
    let config = result.expect("TiKV HTTP configuration must be fetched");
    assert_eq!(config.get_region_max_size(), 256);
}

#[tokio::test]
async fn task_014_http_errors_and_fallback() {
    use astersql_pd_core::{NodeState, StoreInfo, StoreMeta};
    use std::sync::{Arc, RwLock};
    let body = Arc::new(RwLock::new("{bad".to_string()));
    let content = body.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/config",
                axum::routing::get(move || {
                    let content = content.clone();
                    async move {
                        (
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            content.read().unwrap().clone(),
                        )
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let cancel = CancellationToken::new();
    for value in [
        "{bad",
        "[]",
        r#"{"storage":[]}"#,
        r#"{"readpool":{"unified":{"max-thread-count":-1}}}"#,
    ] {
        *body.write().unwrap() = value.into();
        assert!(
            fetch_store_config(&address.to_string(), &cancel)
                .await
                .is_err(),
            "{value}"
        );
    }
    *body.write().unwrap() = r#"{"storage":{"engine":"raft-kv2"}}"#.into();
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_address = dead.local_addr().unwrap();
    drop(dead);
    let store = |id, status_address, node_state| {
        Arc::new(StoreInfo::new(StoreMeta {
            id,
            status_address,
            node_state,
            ..Default::default()
        }))
    };
    let mut candidates = vec![
        store(1, dead_address.to_string(), NodeState::Removed),
        store(2, dead_address.to_string(), NodeState::Serving),
        store(3, address.to_string(), NodeState::Preparing),
    ];
    let config = super::sync_store_config(&mut candidates, &cancel)
        .await
        .unwrap()
        .unwrap();
    assert!(
        config.is_raft_kv2(),
        "Go decodes valid JSON even with HTTP 500"
    );
    assert_eq!(
        candidates.iter().map(|s| s.id()).collect::<Vec<_>>(),
        [1, 3]
    );
    cancel.cancel();
    assert!(
        super::sync_store_config(&mut candidates, &cancel)
            .await
            .is_err()
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test(start_paused = true)]
async fn task_014_worker_clock_and_cancel() {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    let state = Arc::new(Mutex::new(Vec::new()));
    let observed = state.clone();
    let cancel = CancellationToken::new();
    let worker =
        super::StoreConfigWorker::start(&tokio::runtime::Handle::current(), &cancel, move |_| {
            let state = observed.clone();
            async move {
                state.lock().unwrap().push(tokio::time::Instant::now());
            }
        });
    tokio::task::yield_now().await;
    assert_eq!(state.lock().unwrap().len(), 1);
    tokio::time::advance(Duration::from_secs(59)).await;
    tokio::task::yield_now().await;
    assert_eq!(state.lock().unwrap().len(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(state.lock().unwrap().len(), 2);
    worker.stop().await.unwrap();
    tokio::time::advance(Duration::from_secs(120)).await;
    assert_eq!(state.lock().unwrap().len(), 2);
    assert_eq!(Arc::strong_count(&state), 1);
}

#[tokio::test]
async fn task_014_inflight_http_cancel() {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let fetch = tokio::spawn(async move { fetch_store_config(&address.to_string(), &token).await });
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut buffer = [0; 1024];
    assert!(socket.read(&mut buffer).await.unwrap() > 0);
    cancel.cancel();
    assert!(fetch.await.unwrap().unwrap_err().contains("cancelled"));
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(1), socket.read(&mut buffer))
            .await
            .unwrap()
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn task_014_http_timeout() {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        fetch_store_config(&address.to_string(), &CancellationToken::new()).await
    });
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut buffer = [0; 1024];
    assert!(stream.read(&mut buffer).await.unwrap() > 0);
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(3)).await;
    assert!(task.await.unwrap().unwrap_err().contains("timed out"));
    assert_eq!(stream.read(&mut buffer).await.unwrap(), 0);
}

#[tokio::test]
async fn task_014_connection_close_response() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        stream.read(&mut request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{}")
            .await
            .unwrap();
        stream.shutdown().await.unwrap();
    });
    assert_eq!(
        fetch_store_config(&address.to_string(), &CancellationToken::new())
            .await
            .unwrap(),
        astersql_pd_schedule_config::StoreConfig::default()
    );
    server.await.unwrap();
}
