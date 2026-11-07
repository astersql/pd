// Copyright 2026 AsterSQL.
//! Deployment fixtures only: all request and domain behavior remains canonical.
use super::accept_extra_rm_service_test::Sidecar;
use astersql_pd_mcs_discovery::ServiceRegistryEntry;
use astersql_pd_mcs_server::DelegateConnections;
use astersql_pd_storage_kv::EtcdMetaStore;
use prost::Message;
use std::{path::Path, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
enum Host {
    Tso(astersql_pd_mcs_tso_server::TsoServiceHost),
    Router(astersql_pd_mcs_router_server::RouterHost),
    Rm(astersql_pd_rm_server::ResourceManagerHost),
    Scheduling(astersql_pd_runtime::SchedulingHost),
}
pub(super) struct Deployment {
    process: Option<Sidecar>,
    host: Option<Host>,
    parent: CancellationToken,
    delegates: Option<Arc<DelegateConnections>>,
    pub url: String,
}
impl Drop for Deployment {
    fn drop(&mut self) {
        self.parent.cancel();
    }
}
impl Deployment {
    pub async fn start(
        binary: &Path,
        backend: &str,
        service: &str,
        cluster: u64,
        extra: &[&str],
        embedded: bool,
        tls: Option<&Path>,
    ) -> Self {
        let parent = CancellationToken::new();
        if !embedded {
            let process = Sidecar::start_service_with_args(binary, backend, service, "http", extra);
            return Self {
                url: process.url.clone(),
                process: Some(process),
                host: None,
                parent,
                delegates: None,
            };
        }
        let security = tls.map(|certs| astersql_pd_grpcutil::TLSConfig {
            ca_path: certs.join("ca.pem").display().to_string(),
            cert_path: certs.join("server.pem").display().to_string(),
            key_path: certs.join("server.key").display().to_string(),
            cert_allowed_c_ns: Some(if matches!(service, "router" | "scheduling") {
                vec!["allowed".into(), "server".into()]
            } else {
                vec!["allowed".into()]
            }),
            ..Default::default()
        });
        let delegates = Arc::new(if let Some(security) = security.clone() {
            DelegateConnections::new_with_rustls_factory(
                tokio::runtime::Handle::current(),
                &parent,
                Arc::new(move || {
                    astersql_pd_server::client_auth::configuration(&security).map(Arc::new)
                }),
            )
        } else {
            DelegateConnections::new(tokio::runtime::Handle::current(), &parent, None)
        });
        let store = Arc::new(if backend.starts_with("https://") {
            let pool = delegates.clone();
            let address = backend.to_owned();
            EtcdMetaStore::connect_with_transport_factory(
                &[backend.into()],
                Duration::from_secs(2),
                move |_| {
                    astersql_pd_mcs_server::OwnedDelegateChannel::new(vec![(
                        address.clone(),
                        pool.clone(),
                    )])
                    .map_err(|_| astersql_pd_storage_kv::StoreError::Backend)
                },
            )
            .await
            .unwrap()
        } else {
            EtcdMetaStore::connect(&[backend.into()], Duration::from_secs(2))
                .await
                .unwrap()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let host = if let Some(security) = security {
            let listener =
                astersql_pd_server::service_auth::Listener::for_microservice(listener, &security)
                    .unwrap();
            start_host(
                service,
                cluster,
                &url,
                store,
                &parent,
                delegates.clone(),
                listener,
            )
            .await
        } else {
            start_host(
                service,
                cluster,
                &url,
                store,
                &parent,
                delegates.clone(),
                listener,
            )
            .await
        };
        Self {
            url,
            process: None,
            host: Some(host),
            parent,
            delegates: Some(delegates),
        }
    }
    pub fn failed(&mut self) -> Result<bool, String> {
        self.process.as_mut().map_or(Ok(false), |p| {
            p.child
                .try_wait()
                .map(|s| s.is_some())
                .map_err(|e| e.to_string())
        })
    }
    pub fn logs(&self) -> String {
        self.process.as_ref().map_or_else(
            || "actual caller-runtime embedded canonical MCS host".into(),
            Sidecar::logs,
        )
    }
    pub async fn stop(&mut self) {
        if let Some(process) = &mut self.process {
            process.stop();
        }
        if let Some(host) = self.host.take() {
            match host {
                Host::Tso(h) => h.shutdown().await.unwrap(),
                Host::Router(h) => h.shutdown().await.unwrap(),
                Host::Rm(h) => h.shutdown().await.unwrap(),
                Host::Scheduling(h) => h.shutdown().await.unwrap(),
            }
        }
        if let Some(delegates) = self.delegates.take() {
            delegates.shutdown();
        }
        assert!(!self.parent.is_cancelled());
        assert_eq!(tokio::spawn(async { 148 }).await.unwrap(), 148);
    }
}
async fn start_host<L>(
    service: &str,
    cluster: u64,
    url: &str,
    store: Arc<EtcdMetaStore>,
    parent: &CancellationToken,
    delegates: Arc<DelegateConnections>,
    listener: L,
) -> Host
where
    L: axum::serve::Listener<Addr = std::net::SocketAddr> + Send + 'static,
{
    let runtime = tokio::runtime::Handle::current();
    let entry = ServiceRegistryEntry {
        name: "accept-modes".into(),
        service_addr: url.into(),
        ..Default::default()
    };
    match service {
        "tso" => {
            let groups = Arc::new(astersql_pd_keyspace::KeyspaceGroupManager::new(
                store.clone(),
                astersql_pd_keypath::ClusterPaths::new(cluster),
            ));
            let address = url.to_owned();
            let participant = address.clone();
            let config = astersql_pd_tso::GroupWatchConfig {
                store,
                runtime,
                cancel: parent.clone(),
                cluster_id: cluster,
                address,
                bootstrap_keyspace: 0,
                participant: Arc::new(move |id| {
                    astersql_pd_proto::tsopb::Participant {
                        name: format!("mode-{id}"),
                        listen_urls: vec![participant.clone()],
                        ..Default::default()
                    }
                    .encode_to_vec()
                }),
                clock: Arc::new(astersql_pd_tso::SystemClock),
                lease_seconds: 5,
                save_interval_millis: 5000,
            };
            Host::Tso(
                astersql_pd_mcs_tso_server::TsoServiceHost::start_with_entry(
                    config, groups, listener, None, entry,
                )
                .await
                .unwrap(),
            )
        }
        "router" => Host::Router(
            astersql_pd_mcs_router_server::RouterHost::start(
                astersql_pd_mcs_router_server::HostConfig {
                    runtime,
                    cancel: parent.clone(),
                    store,
                    cluster_id: cluster,
                    entry,
                    cluster: Arc::new(std::sync::Mutex::new(astersql_pd_core::BasicCluster::new())),
                    delegates,
                },
                listener,
            )
            .await
            .unwrap(),
        ),
        "resource-manager" => Host::Rm(
            astersql_pd_rm_server::ResourceManagerHost::start(
                astersql_pd_rm_server::HostConfig {
                    runtime,
                    cancel: parent.clone(),
                    store,
                    cluster_id: cluster,
                    entry,
                    controller: Default::default(),
                },
                listener,
            )
            .await
            .unwrap(),
        ),
        "scheduling" => {
            let lookup = store.clone();
            let lookup: astersql_pd_runtime::PdLeaderLookup = Arc::new(move || {
                let store = lookup.clone();
                Box::pin(async move {
                    store
                        .consensus_leader_client_url()
                        .await
                        .map_err(|e| e.to_string())
                })
            });
            Host::Scheduling(
                astersql_pd_runtime::SchedulingHost::start(
                    astersql_pd_runtime::SchedulingHostConfig {
                        runtime,
                        cancel: parent.clone(),
                        store,
                        cluster_id: cluster,
                        entry,
                        delegates,
                        registry: Arc::new(prometheus::Registry::new()),
                        pd_leader_lookup: lookup,
                    },
                    listener,
                )
                .await
                .unwrap(),
            )
        }
        other => panic!("unsupported actual host {other}"),
    }
}

pub(super) async fn metastorage_matrix() {
    use astersql_pd_storage_kv::{Consistency, MetaStore, Read};
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut rows = vec![];
    for mode in 0..3 {
        let parent = CancellationToken::new();
        let mut process = None;
        let mut node = None;
        let fixture = if mode == 2 {
            Some(crate::harness::fixture::Fixture::start())
        } else {
            None
        };
        let (url, store): (String, Arc<dyn MetaStore>) = if let Some(fixture) = &fixture {
            let store = Arc::new(fixture.store().await);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let mut host = astersql_pd_runtime::PdNodeBuilder::new(
                tokio::runtime::Handle::current(),
                parent.clone(),
            )
            .start(
                store.clone(),
                astersql_pd_runtime::NodeConfig {
                    cluster_id: 14801,
                    member_id: 1,
                    name: "meta-modes".into(),
                    client_url: url.clone(),
                    peer_url: "http://127.0.0.1:2380".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
            host.ready().await.unwrap();
            node = Some(host);
            (url, store)
        } else {
            let mut server = if mode == 0 {
                crate::harness::Server::start(&go)
            } else {
                crate::harness::Server::start_rust(&rust)
            }
            .unwrap();
            server.ready().unwrap();
            let store = Arc::new(
                EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(2))
                    .await
                    .unwrap(),
            );
            let url = server.url.clone();
            process = Some(server);
            (url, store)
        };
        let mut row = metastorage_requests(&url, store.clone()).await;
        row["mode"] = serde_json::json!(mode);
        rows.push(row);
        std::fs::write(
            std::path::PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                .with_extension("meta-modes.json"),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
        if let Some(mut node) = node {
            node.shutdown().await.unwrap();
            assert!(!parent.is_cancelled());
            assert_eq!(tokio::spawn(async { 148 }).await.unwrap(), 148);
        }
        if let Some(mut process) = process {
            process.stop().unwrap();
        }
        if mode == 2 {
            assert!(
                store
                    .read(Read::range(b"sa148/mode", b"", Consistency::Linearizable))
                    .await
                    .unwrap()
                    .entries
                    .is_empty()
            );
        }
    }
    for index in [1, 2] {
        assert_eq!(rows[0]["normalized"], rows[index]["normalized"]);
    }
}
async fn metastorage_requests(
    url: &str,
    store: Arc<dyn astersql_pd_storage_kv::MetaStore>,
) -> serde_json::Value {
    use astersql_pd_proto::meta_storagepb as p;
    use astersql_pd_storage_kv::{Consistency, Read};
    use serde_json::json;
    let mut client = p::meta_storage_client::MetaStorageClient::connect(url.to_owned())
        .await
        .unwrap();
    let key = b"sa148/mode".to_vec();
    let value = b"real\0mode".to_vec();
    let mut raw = vec![];
    let mut normalized = vec![];
    fn header(h: &mut Option<p::ResponseHeader>) {
        let h = h.as_mut().unwrap();
        assert!(h.error.is_none());
        h.cluster_id = 0;
        h.revision = 0;
    }
    fn kv(k: &mut p::KeyValue) {
        k.create_revision = 0;
        k.mod_revision = 0;
    }
    let mut put = client
        .put(p::PutRequest {
            key: key.clone(),
            value: value.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let put_revision = put.header.as_ref().unwrap().revision;
    assert!(put_revision > 0);
    raw.push(put.encode_to_vec());
    header(&mut put.header);
    normalized.push(put.encode_to_vec());
    let persisted = store
        .read(Read::range(&key, b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(persisted.entries.len(), 1);
    assert_eq!(persisted.entries[0].value, value);
    assert_eq!(persisted.entries[0].mod_revision, put_revision);
    let mut get = client
        .get(p::GetRequest {
            key: key.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(get.kvs.len(), 1);
    assert_eq!(get.kvs[0].key, key);
    assert_eq!(get.kvs[0].value, value);
    // Go unary adapters intentionally expose only key/value; watch carries MVCC metadata.
    assert_eq!(get.kvs[0].mod_revision, 0);
    assert_eq!(get.kvs[0].create_revision, 0);
    assert_eq!(get.kvs[0].version, 0);
    raw.push(get.encode_to_vec());
    header(&mut get.header);
    for k in &mut get.kvs {
        kv(k);
    }
    normalized.push(get.encode_to_vec());
    let mut watch = client
        .watch(p::WatchRequest {
            key: key.clone(),
            start_revision: put_revision,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let mut created = tokio::time::timeout(Duration::from_secs(5), watch.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(created.events.len(), 1);
    assert_eq!(
        created.events[0].kv.as_ref().unwrap().mod_revision,
        put_revision
    );
    assert_eq!(
        created.events[0].kv.as_ref().unwrap().create_revision,
        put_revision
    );
    assert_eq!(created.events[0].kv.as_ref().unwrap().version, 1);
    assert_eq!(created.events[0].kv.as_ref().unwrap().value, value);
    raw.push(created.encode_to_vec());
    header(&mut created.header);
    for e in &mut created.events {
        if let Some(k) = &mut e.kv {
            kv(k);
        }
        if let Some(k) = &mut e.prev_kv {
            kv(k);
        }
    }
    normalized.push(created.encode_to_vec());
    drop(watch);
    let mut deleted = client
        .delete(p::DeleteRequest {
            key: key.clone(),
            prev_kv: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let delete_revision = deleted.header.as_ref().unwrap().revision;
    assert!(delete_revision > put_revision);
    assert_eq!(
        deleted.deleted, 0,
        "Go intentionally leaves the wire deleted field unset"
    );
    assert_eq!(deleted.prev_kvs[0].value, value);
    raw.push(deleted.encode_to_vec());
    header(&mut deleted.header);
    for k in &mut deleted.prev_kvs {
        kv(k);
    }
    normalized.push(deleted.encode_to_vec());
    let mut replay = client
        .watch(p::WatchRequest {
            key: key.clone(),
            start_revision: delete_revision,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let mut removed = tokio::time::timeout(Duration::from_secs(5), replay.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(removed.events.len(), 1);
    assert_eq!(removed.events[0].r#type, 1);
    assert_eq!(
        removed.events[0].kv.as_ref().unwrap().mod_revision,
        delete_revision
    );
    raw.push(removed.encode_to_vec());
    header(&mut removed.header);
    for e in &mut removed.events {
        if let Some(k) = &mut e.kv {
            kv(k);
        }
        if let Some(k) = &mut e.prev_kv {
            kv(k);
        }
    }
    normalized.push(removed.encode_to_vec());
    drop(replay);
    let mut empty = client
        .get(p::GetRequest {
            key: key.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(empty.kvs.is_empty());
    raw.push(empty.encode_to_vec());
    header(&mut empty.header);
    normalized.push(empty.encode_to_vec());
    assert!(
        store
            .read(Read::range(&key, b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    json!({"raw":raw,"normalized":normalized,"put_revision":put_revision,"delete_revision":delete_revision,"persisted_value":persisted.entries[0].value,"persisted_create_revision":persisted.entries[0].create_revision,"persisted_mod_revision":persisted.entries[0].mod_revision,"deleted_from_same_backend":true,"cancel_reopen_replay":true})
}

pub(super) struct PdDeployment {
    process: Option<crate::harness::Server>,
    node: Option<astersql_pd_runtime::PdNode>,
    _fixture: Option<crate::harness::fixture::Fixture>,
    parent: CancellationToken,
    pub url: String,
    metadata: String,
}
impl Drop for PdDeployment {
    fn drop(&mut self) {
        self.parent.cancel();
    }
}
impl PdDeployment {
    pub fn metadata_url(&self) -> &str {
        &self.metadata
    }
    pub async fn start(
        binary: &Path,
        is_rust: bool,
        embedded: bool,
        fallback: bool,
        flag: &str,
        env: &[(String, String)],
        leader: Option<&Self>,
    ) -> Self {
        let parent = CancellationToken::new();
        if !embedded {
            let mut process = if let Some(leader) = leader {
                let extra = if is_rust {
                    vec![
                        format!("--etcd-endpoints={}", leader.metadata_url()),
                        "--name=follower16".into(),
                        flag.into(),
                    ]
                } else {
                    vec![
                        format!("--join={}", leader.url),
                        "--initial-cluster=".into(),
                        "--name=follower16".into(),
                        flag.into(),
                    ]
                };
                let args = extra.iter().map(String::as_str).collect::<Vec<_>>();
                crate::harness::Server::start_with_environment(binary, &args, env)
            } else if is_rust {
                crate::harness::Server::start_rust_with_environment(binary, &[flag], env)
            } else {
                crate::harness::Server::start_with_environment(binary, &[flag], env)
            }
            .unwrap();
            process.ready().unwrap();
            return Self {
                url: process.url.clone(),
                metadata: process.metadata_url().into(),
                process: Some(process),
                node: None,
                _fixture: None,
                parent,
            };
        }
        let fixture = if leader.is_none() {
            Some(crate::harness::fixture::Fixture::start())
        } else {
            None
        };
        let metadata = leader.map_or_else(
            || fixture.as_ref().unwrap().endpoint.clone(),
            |p| p.metadata.clone(),
        );
        let store = Arc::new(
            EtcdMetaStore::connect(&[metadata.clone()], Duration::from_secs(2))
                .await
                .unwrap(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let name = if leader.is_some() {
            "follower16"
        } else {
            "owner16"
        };
        let config = astersql_pd_runtime::bootstrap_config(
            store.as_ref(),
            name.into(),
            url.clone(),
            "http://127.0.0.1:2380".into(),
        )
        .await
        .unwrap();
        let mut node = astersql_pd_runtime::PdNodeBuilder::new(
            tokio::runtime::Handle::current(),
            parent.clone(),
        )
        .with_keyspace_groups(!fallback)
        .with_resource_manager_fallback(fallback)
        .start(
            store,
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
        node.ready().await.unwrap();
        Self {
            url,
            metadata,
            process: None,
            node: Some(node),
            _fixture: fixture,
            parent,
        }
    }
    pub async fn stop(&mut self) {
        if let Some(mut node) = self.node.take() {
            node.shutdown().await.unwrap();
        }
        if let Some(process) = &mut self.process {
            process.stop().unwrap();
        }
        assert!(!self.parent.is_cancelled());
        assert_eq!(tokio::spawn(async { 138 }).await.unwrap(), 138);
    }
}

pub(super) async fn channel(
    url: &str,
    certs: Option<&Path>,
    identity: Option<&str>,
) -> Result<tonic::transport::Channel, String> {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    let mut endpoint = Endpoint::from_shared(if certs.is_some() {
        url.replacen("http://", "https://", 1)
    } else {
        url.into()
    })
    .unwrap()
    .connect_timeout(Duration::from_secs(2))
    .timeout(Duration::from_secs(5));
    if let Some(certs) = certs {
        let mut tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(Certificate::from_pem(
                std::fs::read(certs.join("ca.pem")).unwrap(),
            ));
        if let Some(identity) = identity {
            tls = tls.identity(Identity::from_pem(
                std::fs::read(certs.join(format!("{identity}.pem"))).unwrap(),
                std::fs::read(certs.join(format!("{identity}.key"))).unwrap(),
            ));
        }
        endpoint = endpoint.tls_config(tls).unwrap();
    }
    endpoint.connect().await.map_err(|e| e.to_string())
}
pub(super) fn tls_argument(certs: &Path, role: &str) -> String {
    let path = certs.join(format!("{role}-business.toml"));
    std::fs::write(&path,format!("[security]\ncacert-path={}\ncert-path={}\nkey-path={}\ncert-allowed-cn=[\"allowed\",\"server\"]\n",serde_json::json!(certs.join("ca.pem")),serde_json::json!(certs.join("server.pem")),serde_json::json!(certs.join("server.key")))).unwrap();
    format!("--config={}", path.display())
}

pub(super) async fn pd_owner(
    go: &Path,
    extra: &[&str],
    env: &[(String, String)],
    certs: Option<&Path>,
) -> crate::harness::Server {
    if let Some(certs) = certs {
        let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("https://{}", reserve.local_addr().unwrap());
        drop(reserve);
        let base = extra
            .iter()
            .find_map(|arg| arg.strip_prefix("--config="))
            .map(|p| std::fs::read_to_string(p).unwrap())
            .unwrap_or_default();
        let config = certs.join("pd-owner.toml");
        std::fs::write(&config,format!("{base}\n[security]\ncacert-path={}\ncert-path={}\nkey-path={}\ncert-allowed-cn=[\"allowed\",\"server\"]\n",serde_json::json!(certs.join("ca.pem")),serde_json::json!(certs.join("server.pem")),serde_json::json!(certs.join("server.key")))).unwrap();
        let mut args = extra.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        args.extend([
            format!("--config={}", config.display()),
            format!("--client-urls={url}"),
            format!("--advertise-client-urls={url}"),
        ]);
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let mut server = crate::harness::Server::start_with_environment(go, &refs, env).unwrap();
        server.url = url.clone();
        tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let output = std::process::Command::new("curl")
                    .args(["-fsS", "--max-time", "1", "--cacert"])
                    .arg(certs.join("ca.pem"))
                    .arg("--cert")
                    .arg(certs.join("allowed.pem"))
                    .arg("--key")
                    .arg(certs.join("allowed.key"))
                    .arg(format!("{url}/pd/api/v1/members"))
                    .output()
                    .unwrap();
                if output.status.success()
                    && serde_json::from_slice::<serde_json::Value>(&output.stdout)
                        .is_ok_and(|v| v["leader"]["member_id"].as_u64().is_some_and(|id| id > 0))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("secure real PD startup: {}", server.logs()));
        server
    } else {
        let mut server = crate::harness::Server::start_with_environment(go, extra, env).unwrap();
        server.ready().unwrap();
        server
    }
}
pub(super) async fn metadata(backend: &str, certs: Option<&Path>) -> EtcdMetaStore {
    if backend.starts_with("https://") {
        let channel = channel(backend, certs, Some("allowed")).await.unwrap();
        EtcdMetaStore::connect_with_transport_factory(
            &[backend.into()],
            Duration::from_secs(2),
            move |_| Ok(channel.clone()),
        )
        .await
        .unwrap()
    } else {
        EtcdMetaStore::connect(&[backend.into()], Duration::from_secs(2))
            .await
            .unwrap()
    }
}
