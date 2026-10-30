// Copyright 2026 AsterSQL.
use crate::etcd_diagnostics::EtcdMemberDiagnostics;
use astersql_pd_server::http_health::LocalConsensusProbe;
use std::time::Duration;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_043_bound_etcd_member() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let mut client = etcd_client::Client::connect([fixture.endpoint.clone()], None)
        .await
        .unwrap();
    let member = client.status().await.unwrap().header().unwrap().member_id();
    let provider =
        EtcdMemberDiagnostics::connect(fixture.endpoint.clone(), member, Duration::from_secs(2))
            .await
            .unwrap();
    for probe in [
        LocalConsensusProbe::SerializableRead,
        LocalConsensusProbe::LinearizableRead,
        LocalConsensusProbe::DataCorruption,
        LocalConsensusProbe::NonLearner,
    ] {
        assert!(
            provider.check(probe).await.is_ok(),
            "real initialized etcd probe must succeed: {probe:?}"
        );
    }
}

fn fetch(url: &str, path: &str) -> (u16, String) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "12",
            "-w",
            "\n%{http_code}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    (code.parse().unwrap(), body.to_owned())
}
async fn node(
    store: std::sync::Arc<dyn astersql_pd_storage::MetaStore>,
    provider: std::sync::Arc<EtcdMemberDiagnostics>,
    cluster: u64,
) -> (crate::PdNode, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = crate::PdNodeBuilder::new(
        tokio::runtime::Handle::current(),
        tokio_util::sync::CancellationToken::new(),
    )
    .with_etcd_member_diagnostics(provider)
    .start(
        store,
        crate::NodeConfig {
            cluster_id: cluster,
            member_id: 1,
            name: "bound".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
        listener,
        std::sync::Arc::new(prometheus::Registry::new()),
    )
    .await
    .unwrap();
    node.ready().await.unwrap();
    (node, url)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_043_binding_faults_and_real_alarm() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use etcd_client::{AlarmAction, AlarmOptions, AlarmType};
    use std::sync::Arc;
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let mut admin = etcd_client::Client::connect([fixture.endpoint.clone()], None)
        .await
        .unwrap();
    let member = admin.status().await.unwrap().header().unwrap().member_id();
    assert!(
        EtcdMemberDiagnostics::connect(
            fixture.endpoint.clone(),
            member.wrapping_add(1),
            Duration::from_secs(2)
        )
        .await
        .is_err()
    );
    let provider = Arc::new(
        EtcdMemberDiagnostics::connect(fixture.endpoint.clone(), member, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let other = crate::node_test::fixture::Fixture::start();
    let other_store = other.store().await;
    assert!(
        provider
            .bind(&other_store, 7435, 1)
            .await
            .unwrap_err()
            .contains("different etcd clusters")
    );
    assert!(
        other_store
            .read(Read::prefix(b"/pd/7435/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let cancelled = tokio_util::sync::CancellationToken::new();
    cancelled.cancel();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let attempted = crate::PdNodeBuilder::new(tokio::runtime::Handle::current(), cancelled)
        .with_etcd_member_diagnostics(provider.clone())
        .start(
            store.clone(),
            crate::NodeConfig {
                cluster_id: 74351,
                member_id: 1,
                name: "cancelled".into(),
                client_url: "http://127.0.0.1:0".into(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await;
    assert!(attempted.is_err());
    assert!(
        store
            .read(Read::prefix(b"/pd/74351/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let (mut node, url) = node(store.clone(), provider.clone(), 7435).await;
    assert_eq!(fetch(&url, "/pd/api/v2/readyz").0, 200);
    assert_eq!(fetch(&url, "/pd/api/v2/livez").0, 200);
    let read = Read::prefix(b"/pd/7435/rust/", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap();
    for path in [
        "/pd/api/v2/livez/etcd-serializable-read",
        "/pd/api/v2/readyz/etcd-linearizable-read",
        "/pd/api/v2/readyz/etcd-non-learner",
        "/pd/api/v2/readyz/etcd-data-corruption",
    ] {
        assert_eq!(fetch(&url, path), (200, "ok\n".into()));
    }
    assert_eq!(before.entries, store.read(read).await.unwrap().entries);
    let key = b"/pd/7435/rust/diagnostic-members/1";
    let original = store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries[0]
        .value
        .clone();
    store
        .transact(
            vec![],
            vec![astersql_pd_storage::Mutation::put(key, b"other-member")],
        )
        .await
        .unwrap();
    assert!(
        provider
            .bind(store.as_ref(), 7435, 1)
            .await
            .unwrap_err()
            .contains("binding mismatch")
    );
    assert_eq!(
        store
            .read(Read::range(key, b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"other-member"
    );
    store
        .transact(
            vec![],
            vec![astersql_pd_storage::Mutation::put(key, &original)],
        )
        .await
        .unwrap();
    let mut options = AlarmOptions::new();
    options.with_member(member);
    admin
        .alarm(
            AlarmAction::Activate,
            AlarmType::Corrupt,
            Some(options.clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        fetch(&url, "/pd/api/v2/readyz/etcd-data-corruption"),
        (
            503,
            "[-]etcd-data-corruption failed: etcd data corruption alarm is active\n".into()
        )
    );
    assert_eq!(fetch(&url, "/pd/api/v2/readyz").0, 503);
    admin
        .alarm(AlarmAction::Deactivate, AlarmType::Corrupt, Some(options))
        .await
        .unwrap();
    assert_eq!(fetch(&url, "/pd/api/v2/readyz").0, 200);
    node.shutdown().await.unwrap();
    drop(fixture);
    for probe in [
        LocalConsensusProbe::SerializableRead,
        LocalConsensusProbe::LinearizableRead,
        LocalConsensusProbe::DataCorruption,
        LocalConsensusProbe::NonLearner,
    ] {
        assert!(provider.check(probe).await.is_err());
    }
}

pub(crate) struct Process {
    child: std::process::Child,
    directory: std::path::PathBuf,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
pub(crate) fn address() -> String {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", socket.local_addr().unwrap())
}
pub(crate) fn launch(
    binary: &str,
    name: &str,
    client: &str,
    peer: &str,
    cluster: &str,
    existing: bool,
) -> Process {
    let directory =
        std::path::PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap()).join(format!(
            "task043-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    let mut command = std::process::Command::new(binary);
    command.args([
        format!("--name={name}"),
        format!("--data-dir={}/data", directory.display()),
        format!("--listen-client-urls={client}"),
        format!("--advertise-client-urls={client}"),
        format!("--listen-peer-urls={peer}"),
        format!("--initial-advertise-peer-urls={peer}"),
        format!("--initial-cluster={cluster}"),
        "--log-level=error".into(),
    ]);
    if existing {
        command.arg("--initial-cluster-state=existing");
    }
    let child = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .unwrap();
    Process { child, directory }
}
pub(crate) async fn wait_status(url: &str) -> etcd_client::Client {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            if let Ok(mut client) = etcd_client::Client::connect(
                [url],
                Some(etcd_client::ConnectOptions::new().with_timeout(Duration::from_secs(1))),
            )
            .await
            {
                if client.status().await.is_ok() {
                    return client;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires verified official standalone etcd via PD_ETCD_BIN"]
async fn task_043_independent_etcd_learner() {
    use std::sync::Arc;
    let binary = std::env::var("PD_ETCD_BIN").unwrap();
    let voter_url = address();
    let voter_peer = address();
    let learner_url = address();
    let learner_peer = address();
    let voter = launch(
        &binary,
        "voter",
        &voter_url,
        &voter_peer,
        &format!("voter={voter_peer}"),
        false,
    );
    let mut admin = wait_status(&voter_url).await;
    let response = admin
        .member_add(
            [learner_peer.clone()],
            Some(etcd_client::MemberAddOptions::new().with_is_learner()),
        )
        .await
        .unwrap();
    let id = response.member().unwrap().id();
    let learner = launch(
        &binary,
        "learner",
        &learner_url,
        &learner_peer,
        &format!("voter={voter_peer},learner={learner_peer}"),
        true,
    );
    let mut client = wait_status(&learner_url).await;
    assert!(client.status().await.unwrap().is_learner());
    let provider = Arc::new(
        EtcdMemberDiagnostics::connect(learner_url.clone(), id, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    assert_eq!(
        provider
            .check(LocalConsensusProbe::NonLearner)
            .await
            .unwrap_err(),
        "member is a learner"
    );
    provider
        .check(LocalConsensusProbe::SerializableRead)
        .await
        .unwrap();
    let store = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(&[voter_url], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let (mut node, url) = node(store, provider.clone(), 7436).await;
    assert_eq!(
        fetch(&url, "/pd/api/v2/readyz/etcd-non-learner"),
        (
            503,
            "[-]etcd-non-learner failed: member is a learner\n".into()
        )
    );
    admin.member_promote(id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().await.unwrap().is_learner() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        fetch(&url, "/pd/api/v2/readyz/etcd-non-learner"),
        (200, "ok\n".into())
    );
    assert_eq!(fetch(&url, "/pd/api/v2/readyz").0, 200);
    node.shutdown().await.unwrap();
    drop(admin);
    drop(voter);
    tokio::time::timeout(Duration::from_secs(12), async {
        while client.status().await.unwrap().leader() != 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    provider
        .check(LocalConsensusProbe::SerializableRead)
        .await
        .unwrap();
    assert!(
        provider
            .check(LocalConsensusProbe::LinearizableRead)
            .await
            .is_err()
    );
    // Host cancellation interrupts a real quorum-blocked read before its timeout.
    let store: Arc<dyn astersql_pd_storage::MetaStore> = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(&[learner_url], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let service = Arc::new(crate::member_role_worker::RoleServices::new(
        crate::NodeConfig {
            cluster_id: 7436,
            member_id: 2,
            name: "cancel".into(),
            client_url: url.clone(),
            peer_url: "http://peer2".into(),
        },
        store.clone(),
    ));
    service.set_diagnostics(Some(provider));
    service.set_listening(true);
    let cancel = tokio_util::sync::CancellationToken::new();
    let stop = tokio_util::sync::CancellationToken::new();
    let shutdown = stop.clone();
    let router = astersql_pd_server::router(service, cancel.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let pending = tokio::task::spawn_blocking(move || {
        fetch(&url, "/pd/api/v2/readyz/etcd-linearizable-read")
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .unwrap()
            .unwrap()
            .0,
        503
    );
    stop.cancel();
    server.await.unwrap();
    drop(client);
    drop(learner);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live existing Go PD oracle via scripts/etcd-diagnostics-oracle.py"]
async fn task_043_readonly_etcd_diagnostics_differential() {
    use etcd_client::{AlarmAction, AlarmOptions, AlarmType};
    use std::sync::Arc;
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_ETCD_DIAGNOSTIC_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    let go = oracle["go_url"].as_str().unwrap();
    let mut admin = etcd_client::Client::connect([go], None).await.unwrap();
    let member = admin.status().await.unwrap().header().unwrap().member_id();
    let provider = Arc::new(
        EtcdMemberDiagnostics::connect(go.into(), member, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let store = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(&[go.into()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let (mut node, url) = node(store, provider, 7437).await;
    for path in [
        "/pd/api/v2/livez",
        "/pd/api/v2/livez?verbose",
        "/pd/api/v2/livez/etcd-serializable-read",
        "/pd/api/v2/readyz",
        "/pd/api/v2/readyz?verbose",
        "/pd/api/v2/readyz/etcd-serializable-read",
        "/pd/api/v2/readyz/etcd-linearizable-read",
        "/pd/api/v2/readyz/etcd-non-learner",
        "/pd/api/v2/readyz/etcd-data-corruption",
    ] {
        assert_eq!(fetch(&url, path), fetch(go, path), "{path}");
    }
    let mut options = AlarmOptions::new();
    options.with_member(member);
    admin
        .alarm(
            AlarmAction::Activate,
            AlarmType::Corrupt,
            Some(options.clone()),
        )
        .await
        .unwrap();
    let path = "/pd/api/v2/readyz/etcd-data-corruption";
    let actual = fetch(&url, path);
    assert_eq!(actual.0, 503);
    assert_eq!(actual, fetch(go, path));
    for path in [
        "/pd/api/v2/readyz/etcd-serializable-read",
        "/pd/api/v2/readyz/etcd-linearizable-read",
    ] {
        let actual = fetch(&url, path);
        assert_eq!(actual, fetch(go, path), "{path}");
    }
    admin
        .alarm(AlarmAction::Deactivate, AlarmType::Corrupt, Some(options))
        .await
        .unwrap();
    assert_eq!(
        fetch(&url, "/pd/api/v2/readyz"),
        fetch(go, "/pd/api/v2/readyz")
    );
    node.shutdown().await.unwrap();
}
