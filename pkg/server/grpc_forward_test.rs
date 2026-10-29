// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_042_behavior() {
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
                    cluster_id: 741,
                    member_id: id,
                    name: format!("redirect-{id}"),
                    client_url: url.clone(),
                    peer_url: format!("http://127.0.0.1:{}", 2400 + id),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        urls.push(url);
        nodes.push(node);
    }
    use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
    let mut client = PdClient::connect(urls[1].clone()).await.unwrap();
    let mut request = tonic::Request::new(pdpb::AllocIdRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 741,
            ..Default::default()
        }),
        count: 1,
    });
    request
        .metadata_mut()
        .insert("pd-forwarded-host", urls[0].parse().unwrap());
    let result = client.alloc_id(request).await;
    if result.is_ok() {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let mut request = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx));
        request
            .metadata_mut()
            .insert("pd-forwarded-host", urls[0].parse().unwrap());
        let pending = tokio::spawn(async move { client.tso(request).await });
        tx.send(pdpb::TsoRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 741,
                ..Default::default()
            }),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
        let mut stream = pending.await.unwrap().unwrap().into_inner();
        let first = stream.message().await.unwrap().unwrap();
        tx.send(pdpb::TsoRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: 741,
                ..Default::default()
            }),
            count: 3,
            ..Default::default()
        })
        .await
        .unwrap();
        let second = stream.message().await.unwrap().unwrap();
        assert_eq!(second.count, 3);
        let a = first.timestamp.unwrap();
        let b = second.timestamp.unwrap();
        assert!((b.physical, b.logical) > (a.physical, a.logical));
        drop(tx);
        drop(stream);
    }
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
    assert!(
        result.is_ok(),
        "forwarded AllocID must reach leader: {result:?}"
    );
    assert!(result.unwrap().into_inner().id > 0);
}

use crate::{Identity, PdServices};
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use axum::{body::Body, extract::State, http::Request, response::IntoResponse};
use prost::Message;
use serde_json::Value;
use std::time::Duration;

struct Follower {
    identity: Identity,
    store: Arc<dyn MetaStore>,
}
#[async_trait::async_trait]
impl PdServices for Follower {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
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
async fn publish(store: &Arc<dyn MetaStore>, urls: Vec<String>) {
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/741/leader",
                &pdpb::Member {
                    member_id: 1,
                    name: "upstream".into(),
                    client_urls: urls,
                    ..Default::default()
                }
                .encode_to_vec(),
            )],
        )
        .await
        .unwrap();
}
async fn serve(
    router: axum::Router,
    cancel: CancellationToken,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await
        .unwrap();
    });
    (url, handle)
}
async fn follower(
    store: Arc<dyn MetaStore>,
    cancel: CancellationToken,
) -> (String, tokio::task::JoinHandle<()>) {
    let services = Arc::new(Follower {
        identity: Identity {
            cluster_id: 741,
            member_id: 2,
            name: "follower".into(),
            client_url: "http://unused".into(),
            peer_url: "http://unused".into(),
        },
        store,
    });
    serve(crate::router(services, cancel.clone()), cancel).await
}

use astersql_pd_proto::pdpb::pd_client::PdClient;
use futures_util::StreamExt;
use http_body_util::BodyStream;
fn alloc(target: &str) -> tonic::Request<pdpb::AllocIdRequest> {
    let mut r = tonic::Request::new(pdpb::AllocIdRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 741,
            ..Default::default()
        }),
        count: 1,
    });
    r.metadata_mut()
        .insert("pd-forwarded-host", target.parse().unwrap());
    r
}
#[derive(Clone)]
struct Observe {
    gate: Arc<tokio::sync::Notify>,
    store: Arc<dyn MetaStore>,
    seen: tokio::sync::mpsc::UnboundedSender<(CancellationToken, axum::http::HeaderMap)>,
}
async fn observe(State(state): State<Observe>, request: Request<Body>) -> axum::response::Response {
    let context = crate::request_context::current().unwrap();
    let (parts, body) = request.into_parts();
    let mut body = Box::pin(BodyStream::new(body));
    let frame = body.next().await.unwrap().unwrap().into_data().unwrap();
    state
        .store
        .transact(vec![], vec![Mutation::put(b"/task042/write", &frame)])
        .await
        .unwrap();
    state
        .seen
        .send((context.cancel, parts.headers.clone()))
        .unwrap();
    match parts
        .headers
        .get("x-case")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
    {
        "error" | "change" => {
            if parts.headers.get("x-case").unwrap() == "change" {
                state.gate.notified().await;
            }
            let mut status = tonic::Status::aborted("committed before leader change");
            status
                .metadata_mut()
                .insert("x-upstream", "preserved".parse().unwrap());
            status.into_http::<Body>()
        }
        "stream" | "trailer" => {
            let bytes = pdpb::TsoResponse {
                count: 1,
                ..Default::default()
            }
            .encode_to_vec();
            let mut frame = vec![0];
            frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            frame.extend(bytes);
            let first = hyper::body::Frame::data(axum::body::Bytes::from(frame));
            let stream: std::pin::Pin<
                Box<
                    dyn futures_util::Stream<
                            Item = Result<hyper::body::Frame<axum::body::Bytes>, std::io::Error>,
                        > + Send,
                >,
            > = if parts.headers.get("x-case").unwrap() == "trailer" {
                let mut status = tonic::Status::aborted("stream finished with error");
                status
                    .metadata_mut()
                    .insert("x-trailer", "preserved".parse().unwrap());
                Box::pin(futures_util::stream::iter(vec![
                    Ok(first),
                    Ok(hyper::body::Frame::trailers(
                        status.into_http::<Body>().into_parts().0.headers,
                    )),
                ]))
            } else {
                Box::pin(
                    futures_util::stream::once(async move { Ok(first) })
                        .chain(futures_util::stream::pending()),
                )
            };
            (
                [
                    ("content-type", "application/grpc"),
                    ("x-response", "preserved"),
                ],
                Body::new(http_body_util::StreamBody::new(stream)),
            )
                .into_response()
        }
        _ => std::future::pending().await,
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_042_metadata_cancel_errors_and_no_replay() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let (seen_tx, mut seen) = tokio::sync::mpsc::unbounded_channel();
    let gate = Arc::new(tokio::sync::Notify::new());
    let router = axum::Router::new()
        .fallback(observe)
        .with_state(Observe {
            gate: gate.clone(),
            store: store.clone(),
            seen: seen_tx,
        })
        .layer(axum::middleware::from_fn_with_state(
            crate::request_context::ContextState::new(cancel.clone()),
            crate::request_context::middleware,
        ));
    let (upstream, up_task) = serve(router, cancel.clone()).await;
    let (url, server) = follower(store.clone(), cancel.clone()).await;
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    assert_eq!(
        client.alloc_id(alloc(&upstream)).await.unwrap_err().code(),
        tonic::Code::Unavailable
    );
    publish(&store, vec![upstream.clone()]).await;
    let mut r = alloc(&upstream);
    r.metadata_mut().insert("x-case", "error".parse().unwrap());
    r.metadata_mut().append("x-repeat", "one".parse().unwrap());
    r.metadata_mut().append("x-repeat", "two".parse().unwrap());
    r.metadata_mut().insert_bin(
        "x-opaque-bin",
        tonic::metadata::MetadataValue::from_bytes(&[0, 255, 1]),
    );
    let status = client.alloc_id(r).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Aborted);
    assert_eq!(status.metadata().get("x-upstream").unwrap(), "preserved");
    let (_, headers) = seen.recv().await.unwrap();
    assert_eq!(headers.get("pd-forwarded-host").unwrap(), "");
    assert_eq!(headers.get_all("x-repeat").iter().count(), 2);
    assert_eq!(
        tonic::metadata::MetadataMap::from_headers(headers.clone())
            .get_bin("x-opaque-bin")
            .unwrap()
            .to_bytes()
            .unwrap()
            .as_ref(),
        &[0, 255, 1]
    );
    let stored = store
        .read(Read::range(
            b"/task042/write",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert!(!stored.entries[0].value.is_empty());
    // Change leadership after the real transaction commits but before its error
    // returns. The transaction revision must not advance through a replay.
    let mut r = alloc(&upstream);
    r.metadata_mut().insert("x-case", "change".parse().unwrap());
    let mut pending = client.clone();
    let write = tokio::spawn(async move { pending.alloc_id(r).await });
    seen.recv().await.unwrap();
    let committed = store
        .read(Read::range(
            b"/task042/write",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries;
    publish(&store, vec!["http://127.0.0.1:1".into()]).await;
    gate.notify_one();
    assert_eq!(
        write.await.unwrap().unwrap_err().code(),
        tonic::Code::Aborted
    );
    assert_eq!(
        store
            .read(Read::range(
                b"/task042/write",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries,
        committed
    );
    assert_eq!(
        client.alloc_id(alloc(&upstream)).await.unwrap_err().code(),
        tonic::Code::InvalidArgument
    );
    publish(&store, vec![upstream.clone()]).await;
    // Deadline includes forwarding; the upstream observes cancellation.
    let mut r = alloc(&upstream);
    r.set_timeout(Duration::from_millis(300));
    let mut pending = client.clone();
    let deadline = tokio::spawn(async move { pending.alloc_id(r).await });
    let (token, headers) = seen.recv().await.unwrap();
    assert!(headers.contains_key("grpc-timeout"));
    let error = deadline.await.unwrap().unwrap_err();
    assert!(matches!(
        error.code(),
        tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
    ));
    tokio::time::timeout(Duration::from_secs(3), token.cancelled())
        .await
        .unwrap();
    // Dropping an in-flight unary client cancels the upstream before headers.
    let r = alloc(&upstream);
    let mut pending = client.clone();
    let call = tokio::spawn(async move { pending.alloc_id(r).await });
    let (token, _) = seen.recv().await.unwrap();
    call.abort();
    let _ = call.await;
    tokio::time::timeout(Duration::from_secs(3), token.cancelled())
        .await
        .unwrap();
    // Dropping a live response stream also closes the upstream connection.
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    tx.send(pdpb::TsoRequest {
        count: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut r = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx));
    r.metadata_mut()
        .insert("pd-forwarded-host", upstream.parse().unwrap());
    r.metadata_mut().insert("x-case", "stream".parse().unwrap());
    let response = client.tso(r).await.unwrap();
    assert_eq!(response.metadata().get("x-response").unwrap(), "preserved");
    let mut stream = response.into_inner();
    assert_eq!(stream.message().await.unwrap().unwrap().count, 1);
    let (token, _) = seen.recv().await.unwrap();
    drop(stream);
    drop(tx);
    tokio::time::timeout(Duration::from_secs(3), token.cancelled())
        .await
        .unwrap();
    let mut r = tonic::Request::new(tokio_stream::iter(vec![pdpb::TsoRequest {
        count: 1,
        ..Default::default()
    }]));
    r.metadata_mut()
        .insert("pd-forwarded-host", upstream.parse().unwrap());
    r.metadata_mut()
        .insert("x-case", "trailer".parse().unwrap());
    let mut stream = client.tso(r).await.unwrap().into_inner();
    assert_eq!(stream.message().await.unwrap().unwrap().count, 1);
    let error = stream.message().await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Aborted);
    assert_eq!(error.metadata().get("x-trailer").unwrap(), "preserved");
    seen.recv().await.unwrap();
    drop(stream);
    // Unknown/local methods never follow caller-controlled destinations.
    let mut r = tonic::Request::new(pdpb::GetMembersRequest::default());
    r.metadata_mut()
        .insert("pd-forwarded-host", "http://127.0.0.1:1".parse().unwrap());
    assert!(client.get_members(r).await.is_ok());
    let mut pending = client.clone();
    let r = alloc(&upstream);
    let call = tokio::spawn(async move { pending.alloc_id(r).await });
    let (token, _) = seen.recv().await.unwrap();
    cancel.cancel();
    assert!(call.await.unwrap().is_err());
    tokio::time::timeout(Duration::from_secs(3), token.cancelled())
        .await
        .unwrap();
    drop(client);
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), up_task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires scripts/grpc-forward-oracle.py and a real three-node Go cluster"]
async fn task_042_readonly_go_differential() {
    use astersql_pd_proto::metapb;
    let record: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_GRPC_FORWARD_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    let leader = record["leader"].as_str().unwrap();
    let go = record["follower"].as_str().unwrap();
    let mut leader_client = PdClient::connect(leader.to_owned()).await.unwrap();
    let id = leader_client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    publish(&store, vec![leader.into()]).await;
    let cancel = CancellationToken::new();
    let (rust, server) = follower(store, cancel.clone()).await;
    let header = Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    });
    let mut clients = [
        PdClient::connect(go.to_owned()).await.unwrap(),
        PdClient::connect(rust).await.unwrap(),
    ];
    let mut ids = Vec::new();
    for client in &mut clients {
        let mut r = alloc(leader);
        r.get_mut().header = header.clone();
        let result = client.alloc_id(r).await.unwrap().into_inner();
        assert!(result.id > 0);
        ids.push(result.id);
    }
    assert!(ids[1] > ids[0]);
    println!("oracle AllocID: {ids:?} unique and increasing through Go/Rust followers");
    let mut outcomes = Vec::new();
    for client in &mut clients {
        let mut r = tonic::Request::new(pdpb::IsBootstrappedRequest {
            header: header.clone(),
        });
        r.metadata_mut()
            .insert("pd-forwarded-host", leader.parse().unwrap());
        outcomes.push(client.is_bootstrapped(r).await.unwrap().into_inner());
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert!(!outcomes[0].bootstrapped);
    // Same malformed request must preserve the leader's gRPC status/message.
    let mut errors = Vec::new();
    for client in &mut clients {
        let e = client.alloc_id(alloc(leader)).await.unwrap_err();
        errors.push((e.code(), e.message().to_owned()));
    }
    println!("oracle wrong cluster: {errors:?}");
    assert_eq!(errors[0], errors[1]);
    let mut stale = Vec::new();
    for client in &mut clients {
        let e = client
            .alloc_id(alloc("http://127.0.0.1:1"))
            .await
            .unwrap_err();
        stale.push((e.code(), e.message().to_owned()));
    }
    println!("oracle stale host: {stale:?}");
    assert_eq!(stale[0], stale[1]);
    // A forwarded write is visible via the actual Go leader's storage-backed RPC.
    let mut r = tonic::Request::new(pdpb::BootstrapRequest {
        header: header.clone(),
        store: Some(metapb::Store {
            id: 10,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 11,
            peers: vec![metapb::Peer {
                id: 12,
                store_id: 10,
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    r.metadata_mut()
        .insert("pd-forwarded-host", leader.parse().unwrap());
    let response = clients[1].bootstrap(r).await.unwrap().into_inner();
    assert!(response.header.unwrap().error.is_none());
    assert!(
        leader_client
            .is_bootstrapped(pdpb::IsBootstrappedRequest {
                header: header.clone()
            })
            .await
            .unwrap()
            .into_inner()
            .bootstrapped
    );
    assert_eq!(
        leader_client
            .get_store(pdpb::GetStoreRequest {
                header: header.clone(),
                store_id: 10
            })
            .await
            .unwrap()
            .into_inner()
            .store
            .unwrap()
            .id,
        10
    );
    println!("oracle Rust forwarded Bootstrap persisted store 10, region 11");
    let mut previous = (0, 0);
    for client in &mut clients {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let mut r = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx));
        r.metadata_mut()
            .insert("pd-forwarded-host", leader.parse().unwrap());
        tx.send(pdpb::TsoRequest {
            header: header.clone(),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
        let mut stream = client.tso(r).await.unwrap().into_inner();
        for count in [1, 3] {
            if count == 3 {
                tx.send(pdpb::TsoRequest {
                    header: header.clone(),
                    count,
                    ..Default::default()
                })
                .await
                .unwrap();
            }
            let r = stream.message().await.unwrap().unwrap();
            assert_eq!(r.count, count);
            let t = r.timestamp.unwrap();
            assert!((t.physical, t.logical) > previous);
            previous = (t.physical, t.logical);
        }
        drop(stream);
        drop(tx);
    }
    println!("oracle bidirectional Tso: both followers return counts 1,3, monotonic timestamps");
    cancel.cancel();
    drop(clients);
    drop(leader_client);
    server.await.unwrap();
}
