// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn fetch(url: &str, path: &str) -> (u16, String) {
    let (code, body, _) = fetch_response(url, path);
    (code, body)
}
fn fetch_response(url: &str, path: &str) -> (u16, String, String) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let text = String::from_utf8(output.stdout).unwrap();
    let mut fields = text.rsplitn(3, '\n');
    let content = fields.next().unwrap().to_owned();
    let code = fields.next().unwrap().parse().unwrap();
    let body = fields.next().unwrap().to_owned();
    (code, body, content)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_043_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 743,
                    member_id: id,
                    name: format!("probe-{id}"),
                    client_url: url.clone(),
                    peer_url: format!("http://127.0.0.1:{}", 2500 + id),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
    }
    // Collect before assertions so even the pre-implementation failure drains nodes.
    let observations: Vec<_> = urls
        .iter()
        .map(|url| {
            [
                "/pd/api/v1/health",
                "/pd/health",
                "/pd/ping",
                "/pd/api/v1/ping",
                "/pd/api/v2/ready?verbose",
            ]
            .map(|path| fetch(url, path))
        })
        .collect();
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
    use prost::Message;
    let mut client = PdClient::connect(urls[0].clone()).await.unwrap();
    let result = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 743,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(result.header.unwrap().error.is_none());
    let (code, body) = fetch(&urls[0], "/pd/api/v2/ready?verbose");
    assert_eq!(code, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"region_loaded":true,"is_bootstrapped":true})
    );
    assert_eq!(
        fetch(&urls[0], "/pd/api/v2/readyz/leader-promotion?verbose"),
        (200, "[+]leader-promotion ok\nok\n".into())
    );
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_url = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);
    let key = b"/pd/743/rust/members/3";
    store
        .transact(
            vec![],
            vec![Mutation::put(
                key,
                &pdpb::Member {
                    member_id: 3,
                    name: "offline".into(),
                    client_urls: vec![dead_url],
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    let read = Read::prefix(b"/pd/743/rust/members/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap();
    let (code, body) = fetch(&urls[1], "/pd/health");
    assert_eq!(code, 200);
    let health: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(health.as_array().unwrap().len(), 3);
    assert_eq!(
        health
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["member_id"] == 3)
            .unwrap()["health"],
        false
    );
    assert_eq!(before.entries, store.read(read).await.unwrap().entries);
    store
        .transact(vec![], vec![Mutation::put(key, b"\xff")])
        .await
        .unwrap();
    assert_eq!(fetch(&urls[1], "/pd/health").0, 500);
    drop(client);
    for node in &mut nodes {
        tokio::time::timeout(Duration::from_secs(10), node.shutdown())
            .await
            .unwrap()
            .unwrap();
    }
    for results in observations {
        for (status, _) in &results {
            assert_eq!(*status, 200);
        }
        for (_, body) in &results[..2] {
            let health: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(health.as_array().unwrap().len(), 2);
            assert!(
                health
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|member| member["health"] == true)
            );
        }
        assert_eq!(results[2].1, "");
        assert_eq!(results[3].1, "");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&results[4].1).unwrap(),
            serde_json::json!({"region_loaded":false,"is_bootstrapped":false})
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the existing Go binary via scripts/http-health-oracle.py"]
async fn task_043_readonly_go_differential() {
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_HTTP_HEALTH_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 7432,
                member_id: 1,
                name: "probe".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let observations: Vec<_> = oracle["responses"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, go)| (path.clone(), go.clone(), fetch_response(&url, path)))
        .collect();
    node.shutdown().await.unwrap();
    for (path, go, (code, body, content)) in observations {
        assert_eq!(content, go["content_type"].as_str().unwrap(), "{path}");
        assert_eq!(code, go["status"].as_u64().unwrap() as u16, "{path}");
        let expected = go["body"].as_str().unwrap();
        if path.ends_with("health") {
            let mut a: serde_json::Value = serde_json::from_str(&body).unwrap();
            let mut b: serde_json::Value = serde_json::from_str(expected).unwrap();
            assert_eq!(a[0]["member_id"], 1);
            assert_eq!(a[0]["client_urls"], serde_json::json!([url]));
            for v in [&mut a, &mut b] {
                for member in v.as_array_mut().unwrap() {
                    member.as_object_mut().unwrap().remove("member_id");
                    member.as_object_mut().unwrap().remove("client_urls");
                }
            }
            assert_eq!(a, b, "{path}");
        } else if path == "/pd/api/v2/ready?verbose" {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).unwrap(),
                serde_json::from_str::<serde_json::Value>(expected).unwrap()
            );
        } else {
            assert_eq!(body, expected, "{path}");
        }
    }
}

struct Discovery {
    identity: crate::Identity,
    store: Arc<dyn astersql_pd_storage::MetaStore>,
}
#[async_trait::async_trait]
impl crate::PdServices for Discovery {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    fn discovery_store(&self) -> Option<Arc<dyn astersql_pd_storage::MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        Err("not leader".into())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("not leader".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("not leader".into())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_043_health_pending_cancel_and_store_unavailable() {
    use astersql_pd_proto::pdpb;
    use astersql_pd_storage::{MetaStore, Mutation};
    use prost::Message;
    use tokio::io::AsyncReadExt;
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let slow = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let slow_url = format!("http://{}", slow.local_addr().unwrap());
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/7433/rust/members/1",
                &pdpb::Member {
                    member_id: 1,
                    name: "pending".into(),
                    client_urls: vec![slow_url],
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let services = Arc::new(Discovery {
        identity: crate::Identity {
            cluster_id: 7433,
            member_id: 2,
            name: "observer".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
        store,
    });
    let cancel = CancellationToken::new();
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let router = crate::router(services.clone(), cancel.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let target = url.clone();
    let pending = tokio::task::spawn_blocking(move || fetch(&target, "/pd/health"));
    let (mut connection, _) = tokio::time::timeout(Duration::from_secs(5), slow.accept())
        .await
        .unwrap()
        .unwrap();
    let mut data = [0; 2048];
    let n = connection.read(&mut data).await.unwrap();
    assert!(String::from_utf8_lossy(&data[..n]).contains("pd-allow-follower-handle: true"));
    cancel.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap()
            .0,
        503
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), connection.read(&mut data))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    stop.cancel();
    server.await.unwrap();
    drop(connection);
    drop(slow);
    // The real backend is stopped, rather than substituting a mocked error result.
    drop(fixture);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let router = crate::router(services, CancellationToken::new());
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let result = fetch(&url, "/pd/health");
    stop.cancel();
    server.await.unwrap();
    assert_eq!(result.0, 503);
}
