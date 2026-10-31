// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, tls_listener_test::Certificates};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_085_behavior() {
    let certificates = Certificates::new();
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("https://{address}");
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .with_listener_config(certificates.config())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 85,
                member_id: 85,
                name: "reload".into(),
                client_url: url.clone(),
                peer_url: "http://peer:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let id = crate::tls_listener_test::rpc(&certificates, &url, "localhost")
        .await
        .unwrap();
    assert!(id > 0);
    let before = store
        .read(Read::prefix(b"/pd/85/alloc_id", Consistency::Linearizable))
        .await
        .unwrap();
    for endpoint in std::iter::once(url.clone()).chain(std::env::var("PD_TLS_GO_URL").ok()) {
        let result = std::process::Command::new("python3")
            .arg("../../scripts/tls-reload-probe.py")
            .arg(
                std::path::Path::new(&certificates.path("server.pem"))
                    .parent()
                    .unwrap(),
            )
            .arg(&endpoint)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{endpoint}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        eprintln!("{}", String::from_utf8_lossy(&result.stdout));
    }
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/pd/85/alloc_id", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
    assert!(
        crate::tls_listener_test::rpc(&certificates, &url, "localhost")
            .await
            .unwrap()
            > id
    );
    let mut stalled = tokio::net::TcpStream::connect(address).await.unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    use tokio::io::AsyncReadExt;
    assert!(
        tokio::time::timeout(Duration::from_secs(2), stalled.read_u8())
            .await
            .unwrap()
            .is_err()
    );
    tokio::net::TcpListener::bind(address).await.unwrap();
}
