// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
struct Certificates(PathBuf, bool);
impl Certificates {
    fn new() -> Self {
        if let Ok(path) = std::env::var("PD_TLS_CERTIFICATES") {
            return Self(path.into(), false);
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap()).join(format!(
            "tls83-{}-{}",
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
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().into()
    }
    fn config(&self) -> astersql_pd_server_config::Config {
        let mut config = astersql_pd_server_config::Config::default();
        config.security.tls.cert_path = self.path("server.pem");
        config.security.tls.key_path = self.path("server.key");
        config.security.tls.ca_path = self.path("ca.pem");
        config
    }
}
impl Drop for Certificates {
    fn drop(&mut self) {
        if self.1 {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn http(c: &Certificates, url: &str, ca: &str, client: &str) -> (bool, u16, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-fsS",
            "-w",
            "\n%{http_code}",
            "--max-time",
            "3",
            "--http1.1",
            "--cacert",
            &c.path(ca),
            "--cert",
            &c.path(&format!("{client}.pem")),
            "--key",
            &c.path(&format!("{client}.key")),
            &format!("{url}/pd/api/v1/ping"),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap();
    (out.status.success(), status.parse().unwrap(), body.into())
}
async fn rpc(
    c: &Certificates,
    url: &str,
    domain: &str,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    let tls = ClientTlsConfig::new()
        .domain_name(domain)
        .ca_certificate(Certificate::from_pem(std::fs::read(c.path("ca.pem"))?))
        .identity(Identity::from_pem(
            std::fs::read(c.path("allowed.pem"))?,
            std::fs::read(c.path("allowed.key"))?,
        ));
    let channel = Endpoint::from_shared(url.to_owned())?
        .tls_config(tls)?
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(3))
        .connect()
        .await?;
    let mut client = PdClient::new(channel);
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await?
        .into_inner();
    assert!(
        members
            .members
            .iter()
            .any(|member| member.client_urls.iter().any(|address| address == url))
    );
    let id = client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: members.header.unwrap().cluster_id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await?
        .into_inner();
    assert!(id.header.unwrap().error.is_none());
    Ok(id.id)
}
fn bad_alpn(c: &Certificates, url: &str) {
    let code = "import ssl,socket,sys; c=ssl.create_default_context(cafile=sys.argv[1]); c.load_cert_chain(sys.argv[2],sys.argv[3]); c.set_alpn_protocols(['invalid-pd']); host,port=sys.argv[4].removeprefix('https://').split(':'); s=socket.create_connection((host,int(port)),timeout=3); c.wrap_socket(s,server_hostname=host)";
    let out = std::process::Command::new("python3")
        .args([
            "-c",
            code,
            &c.path("ca.pem"),
            &c.path("allowed.pem"),
            &c.path("allowed.key"),
            url,
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "unsupported ALPN must fail");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no application protocol"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_083_behavior() {
    let c = Certificates::new();
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("https://{address}");
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .with_listener_config(c.config())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 83,
                member_id: 83,
                name: "tls".into(),
                client_url: url.clone(),
                peer_url: "http://peer:2380".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .expect("HTTPS host must start with configured TLS");
    node.ready().await.unwrap();
    let success = http(&c, &url, "ca.pem", "allowed");
    assert!(success.0);
    assert_eq!(success.1, 200);
    let id = rpc(&c, &url, "localhost").await.unwrap();
    assert!(id > 0);
    let persisted = store
        .read(Read::prefix(b"/pd/83/alloc_id", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(persisted.entries.len(), 1);
    assert!(u64::from_be_bytes(persisted.entries[0].value.as_slice().try_into().unwrap()) >= id);
    eprintln!(
        "task083 HTTPS HTTP/1.1={success:?}; h2 AllocID={id}; durable boundary={:?}",
        persisted.entries[0].value
    );
    let before = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(!http(&c, &url, "rogue.pem", "allowed").0);
    assert!(!http(&c, &url, "ca.pem", "rogue").0);
    assert!(rpc(&c, &url, "wrong.example").await.is_err());
    bad_alpn(&c, &url);
    let after = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
    if let Ok(go) = std::env::var("PD_TLS_GO_URL") {
        assert_eq!(success, http(&c, &go, "ca.pem", "allowed"));
        assert!(rpc(&c, &go, "localhost").await.unwrap() > 0);
        assert!(!http(&c, &go, "rogue.pem", "allowed").0);
        assert!(!http(&c, &go, "ca.pem", "rogue").0);
        assert!(rpc(&c, &go, "wrong.example").await.is_err());
        bad_alpn(&c, &go);
        eprintln!(
            "task083 Go/Rust same-port HTTPS/h2, CA/client-chain/SAN/ALPN differential passed"
        );
    }
    let mut stalled = tokio::net::TcpStream::connect(address).await.unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    use tokio::io::AsyncReadExt;
    let result = tokio::time::timeout(Duration::from_secs(2), stalled.read_u8())
        .await
        .unwrap();
    assert!(result.is_err());
    tokio::net::TcpListener::bind(address).await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_083_invalid_tls_releases_listener_without_storage_effects() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    for scheme in ["http", "https"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let before = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap();
        let mut config = astersql_pd_server_config::Config::default();
        config.security.tls.cert_path = "missing-task83-certificate".into();
        let result =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .with_listener_config(config)
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 83,
                        member_id: 83,
                        name: "tls".into(),
                        client_url: format!("{scheme}://{address}"),
                        peer_url: "http://peer:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await;
        assert!(result.is_err());
        tokio::net::TcpListener::bind(address).await.unwrap();
        assert_eq!(
            before.entries,
            store
                .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
        );
    }
}
