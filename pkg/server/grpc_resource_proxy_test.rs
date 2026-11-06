// Copyright 2026 AsterSQL.
use super::{client_tls, delegate_error};
use astersql_pd_mcs_metastorage_server::{Host, Service};
use astersql_pd_mcs_server::{BaseServer, DelegateConnections};
use astersql_pd_proto::meta_storagepb::{
    self as p, meta_storage_client::MetaStorageClient, meta_storage_server::MetaStorageServer,
};
use astersql_pd_storage_domain::{MemoryStore, MetaStore, Mutation};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
struct Certificates(PathBuf);
impl Certificates {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join(format!(
            "rm-proxy-certificates-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert!(
            std::process::Command::new("python3")
                .arg(root.join("scripts/service-auth-oracle.py"))
                .arg("--certificates")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        Self(path)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
    fn config(&self, ca: &str, identity: &str) -> astersql_pd_grpcutil::TLSConfig {
        astersql_pd_grpcutil::TLSConfig {
            ca_path: self.path(ca),
            cert_path: self.path(&format!("{identity}.pem")),
            key_path: self.path(&format!("{identity}.key")),
            ..Default::default()
        }
    }
}
impl Drop for Certificates {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Storage(Arc<MemoryStore>);
#[async_trait::async_trait]
impl Host for Storage {
    fn cluster_id(&self) -> u64 {
        63
    }
    fn store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.0.clone())
    }
    async fn is_serving(&self) -> bool {
        true
    }
}
#[tokio::test]
async fn mcs_063_proxy_tls_credentials_ignore_uri_scheme_and_enforce_ca_and_cn() {
    let certificates = Certificates::new();
    let parent = CancellationToken::new();
    let store = Arc::new(MemoryStore::default());
    store
        .transact(vec![], vec![Mutation::put(b"hello", b"authenticated")])
        .await
        .unwrap();
    let mut config = astersql_pd_server_config::Config::default();
    config.security.tls = certificates.config("ca.pem", "server");
    config.security.tls.cert_allowed_c_ns = Some(vec!["allowed".into(), "server".into()]);
    let listener = crate::service_auth::Listener::new(
        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(),
        &config,
    )
    .unwrap();
    let mut server = BaseServer::start(
        tokio::runtime::Handle::current(),
        &parent,
        listener,
        move |cancel| {
            Ok(
                tonic::service::Routes::new(MetaStorageServer::new(Service::new(
                    Arc::new(Storage(store)),
                    cancel,
                )))
                .into_axum_router(),
            )
        },
        vec![],
    )
    .await
    .unwrap();
    let target = format!("http://{}", server.address);
    for (ca, identity, allowed) in [
        ("ca.pem", "allowed", true),
        ("ca.pem", "denied", false),
        ("rogue.pem", "allowed", false),
        ("ca.pem", "rogue", false),
    ] {
        let pool = DelegateConnections::new_with_rustls(
            tokio::runtime::Handle::current(),
            &parent,
            client_tls(Some(certificates.config(ca, identity)))
                .unwrap()
                .unwrap(),
        );
        let response = tokio::time::timeout(Duration::from_secs(4), async {
            let mut client = MetaStorageClient::new(pool.get(&target).await?);
            client
                .get(p::GetRequest {
                    key: b"hello".to_vec(),
                    ..Default::default()
                })
                .await
                .map(|response| response.into_inner())
        })
        .await
        .expect("TLS outcome must be bounded");
        if allowed {
            assert_eq!(response.unwrap().kvs[0].value, b"authenticated");
        } else {
            assert!(
                response.is_err(),
                "TLS must reject ca={ca}, identity={identity}"
            );
        }
        pool.shutdown();
    }
    let mut accepted_policy = certificates.config("ca.pem", "allowed");
    accepted_policy.cert_allowed_c_ns = Some(vec!["server".into()]);
    let pool = DelegateConnections::new_with_rustls(
        tokio::runtime::Handle::current(),
        &parent,
        client_tls(Some(accepted_policy)).unwrap().unwrap(),
    );
    let mut client = MetaStorageClient::new(pool.get(&target).await.unwrap());
    client.get(p::GetRequest::default()).await.unwrap();
    pool.shutdown();
    let mut peer_policy = certificates.config("ca.pem", "allowed");
    peer_policy.cert_allowed_c_ns = Some(vec!["not-server".into()]);
    let pool = DelegateConnections::new_with_rustls(
        tokio::runtime::Handle::current(),
        &parent,
        client_tls(Some(peer_policy)).unwrap().unwrap(),
    );
    let rejected = async {
        let mut client = MetaStorageClient::new(pool.get(&target).await?);
        client
            .get(p::GetRequest {
                key: b"hello".to_vec(),
                ..Default::default()
            })
            .await
            .map(|r| r.into_inner())
    }
    .await;
    assert!(
        rejected.is_err(),
        "outbound peer CN not present in the configured allowlist must be rejected"
    );
    pool.shutdown();
    let mut bytes = astersql_pd_grpcutil::TLSConfig::default();
    bytes.sslca_bytes = Some(std::fs::read(certificates.path("ca.pem")).unwrap());
    bytes.ssl_cert_bytes = Some(std::fs::read(certificates.path("allowed.pem")).unwrap());
    bytes.sslkey_bytes = Some(std::fs::read(certificates.path("allowed.key")).unwrap());
    bytes.cert_path = "missing path must be ignored when byte credentials are present".into();
    bytes.cert_allowed_c_ns = Some(vec!["not-server".into()]);
    let pool = DelegateConnections::new_with_rustls(
        tokio::runtime::Handle::current(),
        &parent,
        client_tls(Some(bytes)).unwrap().unwrap(),
    );
    let mut client = MetaStorageClient::new(pool.get(&target).await.unwrap());
    assert_eq!(
        client
            .get(p::GetRequest {
                key: b"hello".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .kvs[0]
            .value,
        b"authenticated"
    );
    pool.shutdown();
    // A cached connection keeps its established trust. Rebuilds refresh the CA
    // through the host factory, and full handshakes reload file client identities.
    let rotated = Certificates::new();
    std::fs::copy(
        certificates.path("ca.pem"),
        certificates.path("trust-live.pem"),
    )
    .unwrap();
    let mut refresh = certificates.config("trust-live.pem", "allowed");
    refresh.cert_allowed_c_ns = Some(vec!["server".into()]);
    let factory = refresh.clone();
    let pool = DelegateConnections::new_with_rustls_factory(
        tokio::runtime::Handle::current(),
        &parent,
        Arc::new(move || client_tls(Some(factory.clone()))?.ok_or_else(|| "missing TLS".into())),
    );
    let mut client = MetaStorageClient::new(pool.get(&target).await.unwrap());
    client.get(p::GetRequest::default()).await.unwrap();
    std::fs::copy(rotated.path("server.pem"), certificates.path("server.pem")).unwrap();
    std::fs::copy(rotated.path("server.key"), certificates.path("server.key")).unwrap();
    client.get(p::GetRequest::default()).await.unwrap();
    pool.invalidate(&target);
    assert!(
        pool.get(&target).await.is_err(),
        "old trust cannot authenticate rotated server CA"
    );
    std::fs::copy(rotated.path("ca.pem"), certificates.path("trust-live.pem")).unwrap();
    pool.invalidate(&target);
    let mut client = MetaStorageClient::new(pool.get(&target).await.unwrap());
    client.get(p::GetRequest::default()).await.unwrap();
    pool.shutdown();
    std::fs::copy(
        certificates.path("allowed.pem"),
        certificates.path("client-live.pem"),
    )
    .unwrap();
    std::fs::copy(
        certificates.path("allowed.key"),
        certificates.path("client-live.key"),
    )
    .unwrap();
    let mut live = certificates.config("trust-live.pem", "client-live");
    live.cert_allowed_c_ns = Some(vec!["server".into()]);
    let pool = DelegateConnections::new_with_rustls(
        tokio::runtime::Handle::current(),
        &parent,
        client_tls(Some(live)).unwrap().unwrap(),
    );
    let mut client = MetaStorageClient::new(pool.get(&target).await.unwrap());
    client.get(p::GetRequest::default()).await.unwrap();
    std::fs::copy(
        certificates.path("denied.pem"),
        certificates.path("client-live.pem"),
    )
    .unwrap();
    std::fs::copy(
        certificates.path("denied.key"),
        certificates.path("client-live.key"),
    )
    .unwrap();
    pool.invalidate(&target);
    let denied = async {
        let mut client = MetaStorageClient::new(pool.get(&target).await?);
        client.get(p::GetRequest::default()).await
    }
    .await;
    assert!(
        denied.is_err(),
        "same client policy must reload the denied leaf at a new handshake"
    );
    std::fs::copy(
        certificates.path("allowed.pem"),
        certificates.path("client-live.pem"),
    )
    .unwrap();
    std::fs::copy(
        certificates.path("allowed.key"),
        certificates.path("client-live.key"),
    )
    .unwrap();
    pool.invalidate(&target);
    let mut client = MetaStorageClient::new(pool.get(&target).await.unwrap());
    client.get(p::GetRequest::default()).await.unwrap();
    pool.shutdown();
    server.shutdown().await.unwrap();
    assert!(!parent.is_cancelled());
    assert_eq!(tokio::spawn(async { 7 }).await.unwrap(), 7);
}

#[test]
fn mcs_100_upstream_io_status_preserves_business_unknown_details_and_metadata() {
    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert("owner", "manager".parse().unwrap());
    let status = tonic::Status::with_details_and_metadata(
        tonic::Code::Unknown,
        "resource group does not exist",
        b"details".to_vec().into(),
        metadata.clone(),
    );
    let status = delegate_error(status);
    assert_eq!(status.code(), tonic::Code::Unknown);
    assert_eq!(status.message(), "resource group does not exist");
    assert_eq!(status.details(), b"details");
    assert_eq!(status.metadata().len(), metadata.len());
    assert_eq!(status.metadata().get("owner"), metadata.get("owner"));
    let io = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "EOF");
    let mut status = tonic::Status::from_error(Box::new(io));
    assert_eq!(status.code(), tonic::Code::Unknown);
    *status.metadata_mut() = metadata.clone();
    let original = status.message().to_owned();
    let status = delegate_error(status);
    assert_eq!(status.code(), tonic::Code::Unavailable);
    assert_eq!(status.message(), original);
    assert_eq!(status.metadata().len(), metadata.len());
    assert_eq!(status.metadata().get("owner"), metadata.get("owner"));
    assert!(std::error::Error::source(&status).is_some());
}
