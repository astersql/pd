// Copyright 2026 AsterSQL.
use crate::{Identity, PdServices, request_context};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use prost::Message;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

struct Services {
    identity: Identity,
    store: Arc<dyn MetaStore>,
    seen: Mutex<Vec<Option<request_context::RequestContext>>>,
}
#[async_trait::async_trait]
impl PdServices for Services {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        self.seen.lock().unwrap().push(request_context::current());
        if let Some(context) = request_context::current() {
            if context.request_id == "http-wait" {
                let mut watch = self
                    .store
                    .watch(b"/task040/blocked".to_vec(), 1)
                    .await
                    .map_err(|e| e.to_string())?;
                watch
                    .next(&context.cancel)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        self.store
            .read(Read::prefix(b"/task040/", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        let context = request_context::current();
        self.seen.lock().unwrap().push(context.clone());
        let value = context.map(|c| c.request_id).unwrap_or_default();
        self.store
            .transact(
                vec![],
                vec![Mutation::put(b"/task040/id", value.as_bytes())],
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(40)
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        let context = request_context::current().expect("stream polling must restore context");
        self.seen.lock().unwrap().push(Some(context.clone()));
        if context.request_id == "stream-wait" {
            let mut watch = self
                .store
                .watch(b"/task040/blocked".to_vec(), 1)
                .await
                .map_err(|e| e.to_string())?;
            watch
                .next(&context.cancel)
                .await
                .map_err(|e| e.to_string())?;
        }
        let snapshot = self
            .store
            .read(Read::prefix(b"/task040/", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        Ok(astersql_pd_tso::Timestamp {
            physical: snapshot.revision as u64,
            logical: 1,
        })
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_040_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/40/leader",
                &pdpb::Member {
                    member_id: 1,
                    client_urls: vec![url.clone()],
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    let services = Arc::new(Services {
        identity: Identity {
            cluster_id: 40,
            member_id: 1,
            name: "ctx".into(),
            client_url: url.clone(),
            peer_url: "http://peer:2380".into(),
        },
        store: store.clone(),
        seen: Mutex::new(vec![]),
    });
    let cancel = CancellationToken::new();
    let router = crate::router(services.clone(), cancel.clone());
    let stop = cancel.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(stop.cancelled_owned())
        .await
        .unwrap();
    });
    let output = std::process::Command::new("curl")
        .args([
            "-fsS",
            "-H",
            "X-Request-ID: http-40",
            "-H",
            "X-Forwarded-For: 203.0.113.1, 10.0.0.1",
            "-H",
            "X-Forwarded-Port: 1234",
            "-H",
            "X-Caller-ID: caller",
            &format!("{url}/pd/api/v1/health"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let contexts = services.seen.lock().unwrap().clone();
    let context = contexts[0]
        .as_ref()
        .expect("HTTP domain call must receive request context");
    assert_eq!(context.request_id, "http-40");
    assert_eq!(context.service_label, "GetHealthStatus");
    assert_eq!(context.source_ip, "203.0.113.1");
    assert_eq!(context.source_port, "1234");
    assert_eq!(context.caller_id, "caller");
    assert_eq!(context.forwarded_for, ["203.0.113.1", "10.0.0.1"]);
    assert!(context.peer.unwrap().ip().is_loopback());
    assert!(request_context::current().is_none());
    tokio::time::timeout(Duration::from_secs(3), context.cancel.cancelled())
        .await
        .unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let mut request = tonic::Request::new(pdpb::AllocIdRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 40,
            ..Default::default()
        }),
        ..Default::default()
    });
    request
        .metadata_mut()
        .insert("x-request-id", "grpc-40".parse().unwrap());
    request
        .metadata_mut()
        .insert("x-forwarded-for", "198.51.100.1".parse().unwrap());
    request
        .metadata_mut()
        .insert("pd-forwarded-host", url.parse().unwrap());
    assert_eq!(client.alloc_id(request).await.unwrap().into_inner().id, 40);
    let stored = store
        .read(Read::prefix(b"/task040/id", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(stored.entries[0].value, b"grpc-40");
    let grpc_context = services
        .seen
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .clone()
        .unwrap();
    assert_eq!(grpc_context.service_label, "/pdpb.PD/AllocID");
    assert_eq!(grpc_context.source_ip, "127.0.0.1");
    assert_eq!(grpc_context.forwarded_for, ["198.51.100.1"]);
    assert_eq!(grpc_context.forwarded_host, url);
    assert!(grpc_context.caller_id.is_empty());
    let error = client
        .alloc_id(pdpb::AllocIdRequest::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert_eq!(
        store
            .read(Read::prefix(b"/task040/id", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        stored.entries
    );
    let error_context = services
        .seen
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .clone()
        .unwrap();
    assert!(!error_context.request_id.is_empty());
    assert_ne!(error_context.request_id, grpc_context.request_id);
    tokio::time::timeout(Duration::from_secs(3), error_context.cancel.cancelled())
        .await
        .unwrap();

    // Request stream polling occurs after the response future has returned.
    for wait in [false, true] {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let mut request = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx));
        let id = if wait {
            "stream-wait"
        } else {
            "stream-success"
        };
        request
            .metadata_mut()
            .insert("x-request-id", id.parse().unwrap());
        let mut stream = client.tso(request).await.unwrap().into_inner();
        tx.send(pdpb::TsoRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 40,
                ..Default::default()
            }),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
        if !wait {
            assert!(
                stream
                    .message()
                    .await
                    .unwrap()
                    .unwrap()
                    .timestamp
                    .unwrap()
                    .physical
                    > 0
            );
        }
        let context = wait_context(&services, id).await;
        assert_eq!(context.service_label, "/pdpb.PD/Tso");
        assert!(
            !context.cancel.is_cancelled(),
            "stream lifetime must survive response headers"
        );
        drop(stream);
        drop(tx);
        tokio::time::timeout(Duration::from_secs(3), context.cancel.cancelled())
            .await
            .unwrap();
    }
    // Disconnect an HTTP request while the domain is awaiting a real etcd watch.
    use tokio::io::AsyncWriteExt;
    let mut socket = tokio::net::TcpStream::connect(url.trim_start_matches("http://"))
        .await
        .unwrap();
    socket
        .write_all(
            b"GET /pd/api/v1/health HTTP/1.1\r\nHost: localhost\r\nX-Request-ID: http-wait\r\n\r\n",
        )
        .await
        .unwrap();
    let context = wait_context(&services, "http-wait").await;
    drop(socket);
    tokio::time::timeout(Duration::from_secs(3), context.cancel.cancelled())
        .await
        .unwrap();
    assert_eq!(
        store
            .read(Read::prefix(b"/task040/id", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        stored.entries
    );
    cancel.cancel();
    drop(client);
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

async fn wait_context(services: &Services, id: &str) -> request_context::RequestContext {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(context) = services
                .seen
                .lock()
                .unwrap()
                .iter()
                .flatten()
                .find(|c| c.request_id == id)
                .cloned()
            {
                return context;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[test]
fn task_040_source_boundaries() {
    use axum::http::HeaderMap;
    let peer = "127.0.0.1:12345".parse().unwrap();
    for (forwarded, real, port, expected) in [
        ("", "", "", ("127.0.0.1", "12345")),
        (
            " , 203.0.113.1",
            "198.51.100.1",
            "123",
            ("198.51.100.1", ""),
        ),
        (
            " 203.0.113.1 , 10.0.0.1",
            "ignored",
            " 80, 81",
            ("203.0.113.1", "80"),
        ),
        ("[2001:db8::1]:abc", "", "123", ("2001:db8::1", "abc")),
        ("2001:db8::1", "", "123", ("2001:db8::1", "123")),
        ("host:", "", "123", ("host", "")),
        ("malformed:x:y", "", "123", ("malformed:x:y", "123")),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", forwarded.parse().unwrap());
        headers.insert("x-real-ip", real.parse().unwrap());
        headers.insert("x-forwarded-port", port.parse().unwrap());
        assert_eq!(
            request_context::http_source(&headers, Some(peer)),
            (expected.0.into(), expected.1.into())
        );
    }
}

#[test]
#[ignore = "requires the explicitly executed existing Go oracle"]
fn task_040_readonly_go_differential() {
    let record: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_REQUEST_CONTEXT_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    for case in record["cases"].as_array().unwrap() {
        let mut headers = axum::http::HeaderMap::new();
        for (name, value) in case["headers"].as_object().unwrap() {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.as_str().unwrap().parse().unwrap(),
            );
        }
        let info = case["info"].as_str().unwrap();
        let field = |name: &str| {
            info.split(&format!("{name}:"))
                .nth(1)
                .unwrap()
                .split(',')
                .next()
                .unwrap()
        };
        // Only the ephemeral socket port is normalized, using the observed Go peer.
        let peer = format!(
            "127.0.0.1:{}",
            field("Port").parse::<u16>().unwrap_or(12345)
        )
        .parse()
        .unwrap();
        let (ip, port) = request_context::http_source(&headers, Some(peer));
        assert_eq!(ip, field("IP"));
        assert_eq!(port, field("Port"));
        assert_eq!(request_context::caller(&headers), field("CallerID"));
        assert_eq!(field("ServiceLabel"), "SetMemberPropertyByName");
        assert_eq!(case["status"], 400);
    }
    assert_eq!(record["health"]["status"], 200);
    assert_eq!(record["before"], record["after"]);
}
