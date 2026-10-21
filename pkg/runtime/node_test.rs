// Copyright 2026 AsterSQL.
use super::PdNodeBuilder;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn shared_runtime_lifecycle() {
    let cancellation = CancellationToken::new();
    let mut node =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), cancellation.clone()).start();
    node.ready().await.expect("ready");
    assert!(!cancellation.is_cancelled());
    node.shutdown().await.expect("shutdown");
    node.shutdown().await.expect("repeated shutdown");
    assert!(
        !cancellation.is_cancelled(),
        "node must not cancel its host"
    );
    println!("embedded: start -> ready -> shutdown -> joined");
}

#[tokio::test]
async fn host_cancellation_stops_worker() {
    let cancellation = CancellationToken::new();
    let mut node =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), cancellation.clone()).start();
    node.ready().await.unwrap();
    node.ready().await.unwrap();
    cancellation.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), node.shutdown())
        .await
        .unwrap()
        .unwrap();
}
