// Copyright 2026 AsterSQL.
use crate::{Identity, PdServices};
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
struct Certificates(PathBuf, bool);
impl Certificates {
    fn new() -> Self {
        if let Ok(path) = std::env::var("PD_AUTH_CERTIFICATES") {
            return Self(path.into(), false);
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap()).join(format!(
            "auth-certificates-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        assert!(
            std::process::Command::new("python3")
                .args(["../../scripts/service-auth-oracle.py", "--certificates"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        Self(path, true)
    }
    fn path(&self, file: &str) -> String {
        self.0.join(file).to_str().unwrap().into()
    }
}
impl Drop for Certificates {
    fn drop(&mut self) {
        if self.1 {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
struct Services {
    identity: Identity,
    store: Arc<dyn MetaStore>,
    allocator: Arc<astersql_pd_id::Allocator>,
}
#[async_trait::async_trait]
impl PdServices for Services {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    fn recovery_marker_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        self.allocator.alloc().await.map_err(|e| e.to_string())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("unused".into())
    }
}
fn config(certs: &Certificates, ca: bool, cns: &[&str]) -> astersql_pd_server_config::Config {
    let mut c = astersql_pd_server_config::Config::default();
    c.security.tls.cert_path = certs.path("server.pem");
    c.security.tls.key_path = certs.path("server.key");
    if ca {
        c.security.tls.ca_path = certs.path("ca.pem");
    }
    c.security.tls.cert_allowed_c_ns = Some(cns.iter().map(|s| s.to_string()).collect());
    c
}
fn http(
    certs: &Path,
    url: &str,
    client: &str,
    method: &str,
    path: &str,
    body: &str,
) -> (bool, u16, String) {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-sS", "--max-time", "5", "--cacert"])
        .arg(certs.join("ca.pem"));
    if !client.is_empty() {
        cmd.arg("--cert")
            .arg(certs.join(format!("{client}.pem")))
            .arg("--key")
            .arg(certs.join(format!("{client}.key")));
    }
    let output = cmd
        .args([
            "-X",
            method,
            "-H",
            "Authorization: Bearer allowed",
            "-H",
            "X-Forwarded-Client-Cert: CN=allowed",
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/{path}"),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (
        output.status.success(),
        status.parse().unwrap(),
        body.into(),
    )
}
async fn rpc(
    certs: &Certificates,
    url: &str,
    client: &str,
    forward: Option<&str>,
) -> Result<pdpb::AllocIdResponse, String> {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    let mut tls = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(
        std::fs::read(certs.path("ca.pem")).unwrap(),
    ));
    if !client.is_empty() {
        tls = tls.identity(Identity::from_pem(
            std::fs::read(certs.path(&format!("{client}.pem"))).unwrap(),
            std::fs::read(certs.path(&format!("{client}.key"))).unwrap(),
        ));
    }
    let channel = Endpoint::from_shared(url.to_owned())
        .unwrap()
        .tls_config(tls)
        .unwrap()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(3))
        .connect()
        .await
        .map_err(|e| e.to_string())?;
    let mut client = pdpb::pd_client::PdClient::new(channel);
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    let cluster_id = members.header.ok_or("missing member header")?.cluster_id;
    let mut request = tonic::Request::new(pdpb::AllocIdRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id,
            ..Default::default()
        }),
        ..Default::default()
    });
    if let Some(forward) = forward {
        request
            .metadata_mut()
            .insert("pd-forwarded-host", forward.parse().unwrap());
    }
    let response = client
        .alloc_id(request)
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    if response
        .header
        .as_ref()
        .and_then(|h| h.error.as_ref())
        .is_some()
    {
        return Err(format!("{:?}", response.header));
    }
    Ok(response)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_082_behavior() {
    let certs = Certificates::new();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let mut observations = vec![];
    let go_urls: Option<Vec<String>> = std::env::var("PD_AUTH_GO_URLS")
        .ok()
        .map(|v| serde_json::from_str(&v).unwrap());
    let mut scenario = 0;
    for (ca, cns) in [
        (true, vec!["allowed", "server"]),
        (true, vec!["server"]),
        (true, vec![]),
        (false, vec!["allowed", "server"]),
    ] {
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = tcp.local_addr().unwrap();
        let url = format!("https://{address}");
        let member = pdpb::Member {
            member_id: 1,
            name: "auth".into(),
            client_urls: vec![url.clone()],
            ..Default::default()
        };
        use prost::Message;
        let term_cancel = CancellationToken::new();
        let member = astersql_pd_member::Member::bootstrap(
            store.clone(),
            term_cancel.clone(),
            82,
            1,
            member.encode_to_vec(),
        )
        .await
        .unwrap();
        let leader = Arc::new(member.campaign(60).await.unwrap());
        let allocator = Arc::new(
            astersql_pd_id::Allocator::new(store.clone(), leader.clone(), term_cancel.clone(), 1)
                .unwrap(),
        );
        let services = Arc::new(Services {
            identity: Identity {
                cluster_id: 82,
                member_id: 1,
                name: "auth".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            store: store.clone(),
            allocator,
        });
        let listener = super::service_auth::Listener::new(tcp, &config(&certs, ca, &cns)).unwrap();
        let cancel = CancellationToken::new();
        let app = crate::router(services, cancel.clone());
        let shutdown = cancel.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<super::service_auth::PeerAddr>(),
            )
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
        });
        // A stalled pre-handshake TCP connection must not block authenticated clients.
        let mut stalled = tokio::net::TcpStream::connect(address).await.unwrap();
        for client in ["", "rogue", "denied", "allowed"] {
            let admitted = !ca || (client == "allowed" || (client == "denied" && cns.len() <= 1));
            let before = store
                .read(Read::prefix(b"/", Consistency::Linearizable))
                .await
                .unwrap()
                .entries;
            let path = certs.0.clone();
            let u = url.clone();
            let name = client.to_owned();
            let read =
                tokio::task::spawn_blocking(move || http(&path, &u, &name, "GET", "ping", ""))
                    .await
                    .unwrap();
            let path = certs.0.clone();
            let u = url.clone();
            let name = client.to_owned();
            let write = tokio::task::spawn_blocking(move || {
                http(
                    &path,
                    &u,
                    &name,
                    "POST",
                    "service-middleware/config",
                    "{\"audit.enable-audit\":\"false\"}",
                )
            })
            .await
            .unwrap();
            let result = rpc(&certs, &url, client, None).await;
            assert_eq!(
                read.0, admitted,
                "HTTP read client={client} ca={ca} cns={cns:?}: {read:?}"
            );
            assert_eq!(
                write.0, admitted,
                "HTTP write client={client} ca={ca} cns={cns:?}: {write:?}"
            );
            assert_eq!(
                result.is_ok(),
                admitted,
                "RPC client={client} ca={ca} cns={cns:?}: {result:?}"
            );
            if admitted {
                assert_eq!(read.1, 200);
                assert_eq!(write.1, 200);
                let id = result.unwrap().id;
                assert!(id > 0);
                let persisted = store
                    .read(Read::prefix(b"/pd/82/alloc_id", Consistency::Linearizable))
                    .await
                    .unwrap();
                assert_eq!(persisted.entries.len(), 1);
                assert!(
                    u64::from_be_bytes(persisted.entries[0].value.clone().try_into().unwrap())
                        >= id
                );
                let middleware = store
                    .read(Read::prefix(
                        b"/pd/82/service_middleware",
                        Consistency::Linearizable,
                    ))
                    .await
                    .unwrap();
                assert!(!middleware.entries.is_empty());
                assert!(
                    String::from_utf8(middleware.entries[0].value.clone())
                        .unwrap()
                        .contains("false")
                );
            } else {
                assert_eq!(
                    store
                        .read(Read::prefix(b"/", Consistency::Linearizable))
                        .await
                        .unwrap()
                        .entries,
                    before,
                    "denied authentication changed persistent state"
                );
            }
            if let Some(urls) = &go_urls {
                let go = &urls[scenario];
                let path = certs.0.clone();
                let u = go.clone();
                let before_go = tokio::task::spawn_blocking(move || {
                    http(&path, &u, "allowed", "GET", "service-middleware/config", "")
                })
                .await
                .unwrap();
                let path = certs.0.clone();
                let u = go.clone();
                let name = client.to_owned();
                let go_read =
                    tokio::task::spawn_blocking(move || http(&path, &u, &name, "GET", "ping", ""))
                        .await
                        .unwrap();
                let path = certs.0.clone();
                let u = go.clone();
                let name = client.to_owned();
                let go_write = tokio::task::spawn_blocking(move || {
                    http(
                        &path,
                        &u,
                        &name,
                        "POST",
                        "service-middleware/config",
                        "{\"audit.enable-audit\":\"false\"}",
                    )
                })
                .await
                .unwrap();
                let go_rpc = rpc(&certs, go, client, None).await;
                if !admitted {
                    let path = certs.0.clone();
                    let u = go.clone();
                    let after_go = tokio::task::spawn_blocking(move || {
                        http(&path, &u, "allowed", "GET", "service-middleware/config", "")
                    })
                    .await
                    .unwrap();
                    assert_eq!(
                        before_go, after_go,
                        "denied Go request changed service config"
                    );
                }
                assert_eq!(
                    (go_read.0, go_read.1, go_write.0, go_write.1, go_rpc.is_ok()),
                    (read.0, read.1, write.0, write.1, admitted),
                    "Go/Rust mismatch client={client} cns={cns:?} rpc={go_rpc:?}"
                );
            }
            observations.push(serde_json::json!({"ca":ca,"cns":cns,"client":client,"read":read.1,"write":write.1,"rpc":admitted}));
        }
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
        use tokio::io::AsyncReadExt;
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), stalled.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        drop(stalled);
        leader.resign().await.unwrap();
        term_cancel.cancel();
        store
            .transact(
                vec![],
                vec![Mutation::Delete {
                    key: b"/pd/82/rust/members/1".to_vec(),
                }],
            )
            .await
            .unwrap();
        scenario += 1;
        let rebound = tokio::net::TcpListener::bind(address).await.unwrap();
        drop(rebound);
    }
    forwarding(&certs, store.clone()).await;
    println!(
        "TASK082_OBSERVATIONS={}",
        serde_json::to_string(&observations).unwrap()
    );
    if let Ok(path) = std::env::var("PD_AUTH_OBSERVATIONS") {
        std::fs::write(path, serde_json::to_vec(&observations).unwrap()).unwrap();
    }
}

async fn forwarding(certs: &Certificates, store: Arc<dyn MetaStore>) {
    use prost::Message;
    let leader = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let leader_url = format!("http://{}", leader.local_addr().unwrap());
    let follower = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("https://{}", follower.local_addr().unwrap());
    let term_cancel = CancellationToken::new();
    let member = pdpb::Member {
        member_id: 1,
        client_urls: vec![leader_url.clone()],
        ..Default::default()
    };
    let member = astersql_pd_member::Member::bootstrap(
        store.clone(),
        term_cancel.clone(),
        82,
        1,
        member.encode_to_vec(),
    )
    .await
    .unwrap();
    let term = Arc::new(member.campaign(60).await.unwrap());
    let allocator = Arc::new(
        astersql_pd_id::Allocator::new(store.clone(), term.clone(), term_cancel.clone(), 1)
            .unwrap(),
    );
    let make = |id, url: String| {
        Arc::new(Services {
            identity: Identity {
                cluster_id: 82,
                member_id: id,
                name: "auth".into(),
                client_url: url,
                peer_url: "http://peer".into(),
            },
            store: store.clone(),
            allocator: allocator.clone(),
        })
    };
    let cancel = CancellationToken::new();
    let app = crate::router(make(1, leader_url.clone()), cancel.clone());
    let stop = cancel.clone();
    let leader_task = tokio::spawn(async move {
        axum::serve(
            leader,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(stop.cancelled_owned())
        .await
        .unwrap();
    });
    let listener =
        super::service_auth::Listener::new(follower, &config(certs, true, &["allowed", "server"]))
            .unwrap();
    let app = crate::router(make(2, follower_url.clone()), cancel.clone());
    let stop = cancel.clone();
    let follower_task = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<super::service_auth::PeerAddr>(),
        )
        .with_graceful_shutdown(stop.cancelled_owned())
        .await
        .unwrap();
    });
    for client in ["", "rogue", "denied", "allowed"] {
        let before = store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries;
        let path = certs.0.clone();
        let url = follower_url.clone();
        let name = client.to_owned();
        let result = tokio::task::spawn_blocking(move || {
            http(
                &path,
                &url,
                &name,
                "POST",
                "service-middleware/config",
                "{\"audit.enable-audit\":\"true\"}",
            )
        })
        .await
        .unwrap();
        let rpc = rpc(certs, &follower_url, client, Some(&leader_url)).await;
        assert_eq!(result.0, client == "allowed");
        assert_eq!(rpc.is_ok(), client == "allowed", "{rpc:?}");
        let after = store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries;
        if client != "allowed" {
            assert_eq!(
                before, after,
                "unauthorized forwarding changed persistent state"
            );
        } else {
            assert_eq!(result.1, 200);
            assert_ne!(before, after);
        }
    }
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(3), leader_task)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), follower_task)
        .await
        .unwrap()
        .unwrap();
    term.resign().await.unwrap();
    term_cancel.cancel();
}

#[tokio::test]
async fn task_082_configuration_failure_releases_listener() {
    let certs = Certificates::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut cfg = config(&certs, true, &["allowed", "server"]);
    cfg.security.tls.key_path = certs.path("ca.pem");
    assert!(super::service_auth::Listener::new(listener, &cfg).is_err());
    let rebound = tokio::net::TcpListener::bind(address).await.unwrap();
    drop(rebound);
}
