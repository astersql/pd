// Copyright 2026 AsterSQL.
use crate::tls_listener_test::{Certificates, rpc};
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn probe(c: &Certificates, url: &str, version: &str, path: &str, content: &str) -> String {
    let mut command = std::process::Command::new("curl");
    command.args([
        "-sS",
        "--max-time",
        "5",
        version,
        "-i",
        "-w",
        "\nVERSION=%{http_version}",
    ]);
    if url.starts_with("https") {
        command.args([
            "--cacert",
            &c.path("ca.pem"),
            "--cert",
            &c.path("allowed.pem"),
            "--key",
            &c.path("allowed.key"),
        ]);
    }
    if !content.is_empty() {
        command.args([
            "-H",
            &format!("Content-Type: {content}"),
            "--data-binary",
            "",
        ]);
    }
    let out = command.arg(format!("{url}{path}")).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_087_behavior() {
    let c = Arc::new(Certificates::new());
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    for tls in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("{}://{address}", if tls { "https" } else { "http" });
        let cancel = CancellationToken::new();
        let mut builder = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone());
        if tls {
            builder = builder.with_listener_config(c.config());
        }
        let mut node = builder
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: if tls { 8788 } else { 8787 },
                    member_id: 87,
                    name: "shared87".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer:2380".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        let mut urls = vec![url.clone()];
        if let Ok(go) = std::env::var(if tls {
            "PD_SHARED_GO_TLS_URL"
        } else {
            "PD_SHARED_GO_URL"
        }) {
            urls.push(go);
        }
        for endpoint in urls {
            let h2 = if tls {
                "--http2"
            } else {
                "--http2-prior-knowledge"
            };
            let a = {
                let c = c.clone();
                let url = endpoint.clone();
                tokio::task::spawn_blocking(move || {
                    probe(&c, &url, "--http1.1", "/pd/api/v1/ping", "")
                })
            };
            let b = {
                let c = c.clone();
                let url = endpoint.clone();
                tokio::task::spawn_blocking(move || probe(&c, &url, h2, "/pd/api/v1/ping", ""))
            };
            if tls {
                let id = rpc(&c, &endpoint, "localhost").await.unwrap();
                assert!(id > 0);
                if endpoint == url {
                    let allocation = store
                        .read(Read::prefix(
                            b"/pd/8788/alloc_id",
                            Consistency::Linearizable,
                        ))
                        .await
                        .unwrap();
                    assert_eq!(allocation.entries.len(), 1);
                    assert!(
                        u64::from_be_bytes(
                            allocation.entries[0].value.as_slice().try_into().unwrap()
                        ) >= id
                    );
                }
            } else {
                let mut client =
                    astersql_pd_proto::pdpb::pd_client::PdClient::connect(endpoint.clone())
                        .await
                        .unwrap();
                let reply = client
                    .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
                    .await
                    .unwrap()
                    .into_inner();
                assert!(!reply.members.is_empty());
            }
            let a = a.await.unwrap();
            let b = b.await.unwrap();
            assert!(a.starts_with("HTTP/1.1 200"), "{a}");
            let go_plain = !tls && endpoint != url;
            assert!(b.contains("VERSION=2"), "{b}");
            assert!(
                b.starts_with(if go_plain { "HTTP/2 415" } else { "HTTP/2 200" }),
                "{b}"
            );
            // Listener readiness precedes the scheduler worker's default-config
            // transactions. Wait for that real startup activity to settle before
            // attributing persistent changes to the requests below.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            loop {
                let first = store
                    .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(500)).await;
                let second = store
                    .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                    .await
                    .unwrap();
                if first.entries == second.entries {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "startup did not settle"
                );
            }
            let before_errors = store
                .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                .await
                .unwrap();
            for (path, ct, status, grpc) in [
                ("/unknown87", "", 404, None),
                ("/unknown87", "application/grpc", 200, Some("12")),
                ("/pdpb.PD/Unknown87", "application/grpc", 200, Some("12")),
                ("/pdpb.PD/GetMembers", "application/json", 404, None),
                ("/pdpb.PD/GetMembers", "application/grpc-web", 415, None),
                (
                    "/pdpb.PD/Unknown87",
                    "application/grpc+proto",
                    200,
                    Some("12"),
                ),
                (
                    "/pdpb.PD/Unknown87",
                    "application/grpc; charset=utf-8",
                    200,
                    Some("12"),
                ),
                ("/pd/api/v1/ping", "application/grpc", 405, None),
                ("/metrics", "application/grpc", 200, Some("12")),
            ] {
                let out = probe(&c, &endpoint, h2, path, ct);
                let plain_rejected = go_plain && grpc.is_none();
                let expected = if go_plain && path.starts_with("/pd/") {
                    200
                } else if plain_rejected {
                    415
                } else {
                    status
                };
                assert!(
                    out.starts_with(&format!("HTTP/2 {expected}")),
                    "{endpoint} {path} {ct}: {out}"
                );
                let grpc = if go_plain && path.starts_with("/pd/") {
                    Some("12")
                } else if plain_rejected {
                    Some("3")
                } else {
                    grpc
                };
                if let Some(code) = grpc {
                    assert!(out.contains(&format!("grpc-status: {code}")), "{out}");
                } else {
                    assert!(!out.contains("grpc-status:"), "{out}");
                }
                eprintln!("task087 {endpoint} {path} {ct}: status={expected} grpc={grpc:?}");
            }
            for ct in ["", "application/grpc", "application/grpc-web"] {
                let out = probe(&c, &endpoint, "--http1.1", "/unknown87", ct);
                assert!(
                    out.starts_with("HTTP/1.1 404") && !out.contains("grpc-status:"),
                    "{out}"
                );
            }
            let after_errors = store
                .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                .await
                .unwrap();
            assert_eq!(
                before_errors.entries, after_errors.entries,
                "rejected protocol/path requests cannot mutate MetaStore"
            );
            if endpoint != url {
                eprintln!(
                    "task087 differential passed tls={tls}; plain HTTP/2 HTTP extension explicitly compared"
                );
            }
        }
        let before = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap();
        let mut stalled = tokio::net::TcpStream::connect(address).await.unwrap();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), node.shutdown())
            .await
            .unwrap()
            .unwrap();
        use tokio::io::AsyncReadExt;
        let mut byte = [0];
        let read = tokio::time::timeout(Duration::from_secs(2), stalled.read(&mut byte))
            .await
            .unwrap();
        assert!(
            matches!(read, Ok(0) | Err(_)),
            "cancelled listener must close stalled sockets"
        );
        tokio::net::TcpListener::bind(address).await.unwrap();
        let after = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap();
        assert!(after.revision >= before.revision);
    }
}
