// Copyright 2026 AsterSQL.
use crate::node_test::fixture::Fixture;
use astersql_pd_storage::{Consistency, MetaStore, Read};

#[tokio::test]
async fn task_039_behavior() {
    let fixture = Fixture::start();
    let store = fixture.store().await;
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    let result = crate::bootstrap_config(
        &store,
        "invalid".into(),
        "http://user:secret@127.0.0.1:2379".into(),
        "http://127.0.0.1:2380".into(),
    )
    .await;
    assert!(
        result.is_err(),
        "credentials in advertised URL must fail before durable bootstrap"
    );
    let after = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
}

#[test]
fn task_039_endpoint_boundaries() {
    use crate::net_address::Endpoint;
    for (raw, normalized) in [
        ("HTTP://PD.EXAMPLE.:02379", "http://pd.example:2379"),
        ("http://[0:0:0:0:0:0:0:1]:02379", "http://[::1]:2379"),
        ("https://pd.example:443", "https://pd.example:443"),
        ("http://127.0.0.1:0", "http://127.0.0.1:0"),
    ] {
        assert_eq!(Endpoint::parse(raw).unwrap().url(), normalized);
    }
    for raw in [
        "",
        "http://",
        "ftp://host:1",
        "http://host:",
        "http://host:65536",
        "http://host:+1",
        "http://::1:23",
        "http://[bad]:23",
        "http://host:1/path",
        "http://host:1?q=a",
        "http://host:1#secret",
        "http://user:secret@host:1",
        "http://host:1,http://other:2",
        "http://host:1\n",
        " http://host:1",
        "http://[fe80::1%25en0]:1",
    ] {
        let error = Endpoint::parse(raw).unwrap_err().to_string();
        assert!(!error.contains("secret"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_039_reserved_listener_lifecycle() {
    use crate::{PdNodeBuilder, net_address::Endpoint};
    use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    let fixture = Fixture::start();
    let store = Arc::new(fixture.store().await);
    for (index, raw) in ["http://127.0.0.1:0", "http://[::1]:0"]
        .into_iter()
        .enumerate()
    {
        let endpoint = Endpoint::parse(raw).unwrap();
        let listener = endpoint.bind().await.unwrap();
        let bound = listener.local_addr().unwrap();
        assert!(tokio::net::TcpListener::bind(bound).await.is_err());
        let advertised = endpoint.advertised_url(bound).unwrap();
        let config = crate::bootstrap_config(
            store.as_ref(),
            format!("addr-{index}"),
            raw.into(),
            "http://peer:2380".into(),
        )
        .await
        .unwrap();
        let cancel = CancellationToken::new();
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                config,
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        let mut client = PdClient::connect(advertised.clone()).await.unwrap();
        let members = client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            members
                .members
                .iter()
                .find(|m| m.name == format!("addr-{index}"))
                .unwrap()
                .client_urls,
            vec![advertised.clone()]
        );
        let output = std::process::Command::new("curl")
            .args([
                "--noproxy",
                "*",
                "--max-time",
                "5",
                "-fsS",
                &format!("{advertised}/pd/api/v1/members"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains(&advertised)
        );
        let persisted = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap();
        assert!(persisted.entries.iter().any(|e| {
            e.value
                .windows(advertised.len())
                .any(|w| w == advertised.as_bytes())
        }));
        drop(client);
        cancel.cancel();
        node.shutdown().await.unwrap();
        assert!(tokio::net::TcpStream::connect(bound).await.is_err());
        let rebound = tokio::net::TcpListener::bind(bound).await.unwrap();
        drop(rebound);
    }
}

#[tokio::test]
async fn task_039_bind_errors_and_drop() {
    use crate::net_address::Endpoint;
    // HTTPS now reserves TCP; PdNodeBuilder validates and wraps it before serving.
    let tls = Endpoint::parse("https://127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap();
    let tls_address = tls.local_addr().unwrap();
    assert!(tokio::net::TcpListener::bind(tls_address).await.is_err());
    drop(tls);
    assert_released(tls_address).await;
    assert!(
        Endpoint::parse("http://localhost")
            .unwrap()
            .bind()
            .await
            .is_err()
    );
    let listener = Endpoint::parse("http://127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap();
    let bound = listener.local_addr().unwrap();
    assert!(
        Endpoint::parse(&format!("http://{bound}"))
            .unwrap()
            .bind()
            .await
            .is_err()
    );
    assert!(
        Endpoint::parse("http://0.0.0.0:0")
            .unwrap()
            .advertised_url(bound)
            .is_err()
    );
    assert!(
        Endpoint::parse("http://[::]:0")
            .unwrap()
            .advertised_url(bound)
            .is_err()
    );
    // An explicit advertise host may differ from the listener (NAT/proxy).
    assert_eq!(
        Endpoint::parse("http://PD.EXAMPLE.:02379")
            .unwrap()
            .advertised_url(bound)
            .unwrap(),
        "http://pd.example:2379"
    );
    drop(listener);
    assert_released(bound).await;
}

#[test]
#[ignore = "requires fresh Go observations from scripts/net-address-oracle.py"]
fn task_039_readonly_go_differential() {
    use crate::net_address::Endpoint;
    let path = std::env::var("PD_NET_ADDRESS_ORACLE").unwrap();
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for case in data["cases"].as_array().unwrap() {
        let raw = case["url"].as_str().unwrap();
        if case["accepted"] == true {
            assert_eq!(
                Endpoint::parse(raw).unwrap().url(),
                case["members"]["members"][0]["client_urls"][0]
                    .as_str()
                    .unwrap()
            );
            assert_eq!(case["health"][0]["health"], true);
        } else {
            assert_ne!(case["exit"], 0);
            assert!(Endpoint::parse(raw).is_err(), "{raw}");
        }
    }
    assert_eq!(data["cases"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn task_039_invalid_embedded_start_releases_reservation() {
    use crate::{NodeConfig, PdNodeBuilder, net_address::Endpoint};
    use std::sync::Arc;
    let fixture = Fixture::start();
    let store = Arc::new(fixture.store().await);
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    for raw in [
        "http://user:secret@localhost:2379",
        "http://0.0.0.0:2379",
        "https://localhost:2379",
    ] {
        let listener = Endpoint::parse("http://127.0.0.1:0")
            .unwrap()
            .bind()
            .await
            .unwrap();
        let bound = listener.local_addr().unwrap();
        let result = PdNodeBuilder::new(
            tokio::runtime::Handle::current(),
            tokio_util::sync::CancellationToken::new(),
        )
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 39,
                member_id: 39,
                name: "invalid".into(),
                client_url: raw.into(),
                peer_url: "http://peer:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await;
        assert!(result.is_err());
        assert_released(bound).await;
    }
    assert_eq!(
        before.entries,
        store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
    );
}

// Tokio may defer deregistration/close until the current-thread reactor polls.
// Observe actual OS availability with a bounded deadline, not a fixed sleep.
async fn assert_released(bound: std::net::SocketAddr) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match tokio::net::TcpListener::bind(bound).await {
            Ok(listener) => {
                drop(listener);
                return;
            }
            Err(error) => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "listener {bound} not released: {error}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }
}
