// Copyright 2026 AsterSQL.
use super::{BaseServer, StartCallback};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_062_listener_shutdown_closes_retained_connection_and_keeps_host_runtime() {
    let parent = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let callback = called.clone();
    let mut server = BaseServer::start(
        tokio::runtime::Handle::current(),
        &parent,
        listener,
        |_| Ok(axum::Router::new().route("/ready", axum::routing::get(|| async { "ready" }))),
        vec![Box::new(move || {
            callback.store(true, Ordering::SeqCst);
            Ok(())
        }) as StartCallback],
    )
    .await
    .unwrap();
    assert!(called.load(Ordering::SeqCst));
    let address = server.address;
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET /ready HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut buffer = [0; 1024];
    let size = client.read(&mut buffer).await.unwrap();
    assert!(
        std::str::from_utf8(&buffer[..size])
            .unwrap()
            .contains("200 OK")
    );
    server.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
    let closed = tokio::time::timeout(std::time::Duration::from_secs(1), client.read(&mut buffer))
        .await
        .unwrap();
    assert!(matches!(closed, Ok(0) | Err(_)));
    assert!(!parent.is_cancelled());
    tokio::spawn(async { 7 })
        .await
        .map(|value| assert_eq!(value, 7))
        .unwrap();
    tokio::net::TcpListener::bind(address).await.unwrap();
}
#[tokio::test]
async fn mcs_062_start_callback_failure_releases_listener() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let result = BaseServer::start(
        tokio::runtime::Handle::current(),
        &CancellationToken::new(),
        listener,
        |_| Ok(axum::Router::new()),
        vec![Box::new(|| Err("start callback failed".into()))],
    )
    .await;
    assert!(matches!(result,Err(error) if error=="start callback failed"));
    tokio::net::TcpListener::bind(address).await.unwrap();
}
