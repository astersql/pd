// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_045_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let id = store.consensus_leader_id().await.unwrap().unwrap();
    let admin = Arc::new(
        astersql_pd_runtime::member_admin::EtcdMemberAdmin::connect(
            fixture.endpoint.clone(),
            id,
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_etcd_member_admin(admin)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 745,
                member_id: 1,
                name: "member-1".into(),
                client_url: url.clone(),
                peer_url: "http://peer-1".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/leader"),
        ])
        .output()
        .unwrap();
    use crate::http_member_test::request;
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let read = Read::range(b"/pd/745/leader", b"", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap();
    for (path, expected) in [
        (
            "/pd/api/v1/leader/transfer/absent",
            (
                500,
                serde_json::json!("no valid pd to transfer etcd leader"),
            ),
        ),
        (
            "/pd/api/v1/leader/transfer/member-1",
            (200, serde_json::json!("The transfer command is submitted.")),
        ),
        (
            "/pd/api/v1/leader/resign",
            (200, serde_json::json!("The resign command is submitted.")),
        ),
    ] {
        assert_eq!(request(&url, "POST", path, ""), expected);
    }
    assert_eq!(before.entries, store.read(read).await.unwrap().entries);
    assert_eq!(store.consensus_leader_id().await.unwrap(), Some(id));
    node.shutdown().await.unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    assert_eq!(status, "200", "leader route must read the real lease");
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["member_id"], 1);
    assert_eq!(body["name"], "member-1");
}
