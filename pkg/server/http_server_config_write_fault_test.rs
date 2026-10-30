// Copyright 2026 AsterSQL.
use crate::http_schedule_config_write_fault_test::ConfigStore;
use astersql_pd_storage::{MetaStore, Read};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_050_fault_cancel_and_concurrent() {
    use crate::http_member_test::request;
    use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let inner = Arc::new(fixture.store().await);
    let store = Arc::new(ConfigStore {
        inner: inner.clone(),
        mode: AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7502,
                member_id: 1,
                name: "fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 7502,
                    member_id: 2,
                    name: "follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    assert_eq!(
        request(
            &follower_url,
            "POST",
            "/pd/api/v1/config",
            r#"{"pd-server.flow-round-by-digit":88}"#
        )
        .0,
        200
    );
    assert_eq!(
        request(&url, "GET", "/pd/api/v1/config", "").1["pd-server"]["flow-round-by-digit"],
        88
    );
    follower.shutdown().await.unwrap();
    let read = Read::range(
        b"/pd/7502/config",
        b"",
        astersql_pd_storage::Consistency::Linearizable,
    );
    let before = inner.read(read.clone()).await.unwrap().entries;
    let config = request(&url, "GET", "/pd/api/v1/config", "");
    store.mode.store(1, Ordering::SeqCst);
    let failure = request(
        &url,
        "POST",
        "/pd/api/v1/config",
        r#"{"pd-server.flow-round-by-digit":999}"#,
    );
    assert_eq!(failure.0, 400, "{failure:?}");
    assert!(failure.1.as_str().unwrap().contains("lease"), "{failure:?}");
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    assert_eq!(request(&url, "GET", "/pd/api/v1/config", ""), config);
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        request(
            &url,
            "POST",
            "/pd/api/v1/config/cluster-version",
            r#"{"cluster-version":"8.0.0"}"#
        )
        .0,
        500
    );
    assert_eq!(inner.read(read.clone()).await.unwrap().entries, before);
    assert_eq!(request(&url, "GET", "/pd/api/v1/config", ""), config);
    let mut tasks = Vec::new();
    for body in [
        r#"{"pd-server.flow-round-by-digit":123}"#,
        r#"{"pd-server.block-safe-point-v1":"true"}"#,
    ] {
        let url = url.clone();
        tasks.push(tokio::task::spawn_blocking(move || {
            request(&url, "POST", "/pd/api/v1/config", body)
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().0, 200);
    }
    let config = request(&url, "GET", "/pd/api/v1/config", "").1;
    assert_eq!(config["pd-server"]["flow-round-by-digit"], 123);
    assert_eq!(config["pd-server"]["block-safe-point-v1"], "true");
    let before = inner.read(read.clone()).await.unwrap().entries;
    store.mode.store(2, Ordering::SeqCst);
    let write_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                "POST",
                "--data-binary",
                r#"{"pd-server.flow-round-by-digit":789}"#,
                "-w",
                "\n%{http_code}",
                &format!("{write_url}/pd/api/v1/config"),
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let response = pending.await.unwrap();
    let (body, status) = response.rsplit_once('\n').unwrap();
    assert_eq!(status, "500");
    assert!(
        body.contains("[PD:server:ErrServerNotStarted]server not started"),
        "{body}"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inner.read(read).await.unwrap().entries, before);
}
