// Copyright 2026 AsterSQL.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    net::SocketAddr,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{PdNodeBuilder, bootstrap_config};
use astersql_pd_storage_kv::open_isolated;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

const FIXTURE: &str = "native_single_binary_test::run_native_host_fixture";

struct Host {
    child: Child,
    log: PathBuf,
    ready: PathBuf,
}

impl Host {
    fn start(root: &Path, address: Option<SocketAddr>) -> Self {
        let label = if address.is_none() { "first" } else { "reopen" };
        let log = root.join(format!("{label}.log"));
        let output = File::create(&log).unwrap();
        let ready = root.join(format!("{label}.ready"));
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", FIXTURE, "--nocapture"])
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("PD_NATIVE_HOST_STORE", root.join("store"))
            .env("PD_NATIVE_HOST_READY", &ready)
            .env(
                "PD_NATIVE_HOST_ADDRESS",
                address.map(|a| a.to_string()).unwrap_or_default(),
            )
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap();
        Self { child, log, ready }
    }

    fn logs(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    async fn wait_for(&mut self, observe: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !observe(self) {
            assert!(self.child.try_wait().unwrap().is_none(), "{}", self.logs());
            assert!(Instant::now() < deadline, "{}", self.logs());
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn wait(&mut self) -> ExitStatus {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{}", self.logs()))
    }

    fn signal(&self, signal: &str) {
        assert!(
            Command::new("/bin/kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
    }

    fn assert_no_children(&self) {
        let output = Command::new("/bin/ps")
            .args(["-axo", "ppid="])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            !String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .any(|id| id.trim().parse::<u32>() == Ok(self.child.id()))
        );
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[tokio::test]
#[ignore = "test subprocess driven by theme10_native_single_binary_embedded_wal"]
async fn run_native_host_fixture() {
    let root = PathBuf::from(std::env::var_os("PD_NATIVE_HOST_STORE").unwrap());
    let ready = PathBuf::from(std::env::var_os("PD_NATIVE_HOST_READY").unwrap());
    let address = std::env::var("PD_NATIVE_HOST_ADDRESS").unwrap();
    let mut usr1 =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()).unwrap();
    let mut term =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .finish(),
    )
    .unwrap();
    let store = Arc::new(open_isolated(&root, 40, 1).await.unwrap());
    if address.is_empty() {
        store
            .consensus()
            .initialize(BTreeSet::from([1]))
            .await
            .unwrap();
    }
    store
        .consensus()
        .wait(Some(Duration::from_secs(5)))
        .current_leader(1, "local voter")
        .await
        .unwrap();
    let listener = TcpListener::bind(if address.is_empty() {
        "127.0.0.1:0"
    } else {
        &address
    })
    .await
    .unwrap();
    let local = listener.local_addr().unwrap();
    let config = bootstrap_config(
        store.as_ref(),
        "native-host".into(),
        format!("http://{local}"),
        "http://127.0.0.1:2380".into(),
    )
    .await
    .unwrap();
    let token = CancellationToken::new();
    let mut pd = PdNodeBuilder::new(tokio::runtime::Handle::current(), token.clone())
        .start(
            store.clone(),
            config,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    pd.ready().await.unwrap();
    let publish = ready.with_extension("pending");
    fs::write(&publish, local.to_string()).unwrap();
    fs::rename(publish, &ready).unwrap();
    assert_eq!(usr1.recv().await, Some(()));
    tracing::info!("host received SIGUSR1");
    assert_eq!(term.recv().await, Some(()));
    pd.shutdown().await.unwrap();
    drop(pd);
    assert!(!token.is_cancelled());
    assert_eq!(tokio::spawn(async { 40 }).await.unwrap(), 40);
    Arc::try_unwrap(store).ok().unwrap().close().await.unwrap();
    tracing::info!("host closed native store");
}

async fn observe(address: SocketAddr) -> (u64, u64, (i64, i64)) {
    let endpoint = tonic::transport::Endpoint::from_shared(format!("http://{address}"))
        .unwrap()
        .timeout(Duration::from_secs(2));
    let mut client = PdClient::new(endpoint.connect_lazy());
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let cluster = members.header.unwrap().cluster_id;
    assert_ne!(cluster, 0);
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let deadline = Instant::now() + Duration::from_secs(12);
    let id = loop {
        let response = client
            .alloc_id(pdpb::AllocIdRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await;
        if let Ok(response) = response {
            let response = response.into_inner();
            if response.header.is_some_and(|h| h.error.is_none()) && response.id > 0 {
                break response.id;
            }
        }
        assert!(
            Instant::now() < deadline,
            "native host did not acquire the PD term"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let mut tso = client
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header,
            count: 1,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    let tso = tso.message().await.unwrap().unwrap();
    assert!(tso.header.unwrap().error.is_none());
    let ts = tso.timestamp.unwrap();
    let response = Command::new("/usr/bin/curl")
        .args(["-fsS", "--max-time", "3"])
        .arg(format!("http://{address}/pd/api/v1/health"))
        .output()
        .unwrap();
    assert!(response.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.stdout).unwrap()[0]["health"],
        true
    );
    (cluster, id, (ts.physical, ts.logical))
}

#[tokio::test]
async fn theme10_native_single_binary_embedded_wal() {
    // This executes the embedded host only. The standalone pd-server CLI is
    // exercised separately when its native backend selection is available.
    let root = tempfile::tempdir().unwrap();
    let mut first = Host::start(root.path(), None);
    first.wait_for(|host| host.ready.is_file()).await;
    let address: SocketAddr = fs::read_to_string(&first.ready).unwrap().parse().unwrap();
    let before = observe(address).await;
    let wal = root.path().join("store/metadata.sqlite-wal");
    assert!(fs::metadata(&wal).unwrap().len() > 0);
    first.assert_no_children();
    first.signal("-USR1");
    first
        .wait_for(|host| host.logs().contains("host received SIGUSR1"))
        .await;
    first.child.kill().unwrap();
    assert_eq!(first.wait().await.signal(), Some(9));
    assert!(wal.is_file());
    assert!(tokio::net::TcpStream::connect(address).await.is_err());

    let mut reopened = Host::start(root.path(), Some(address));
    reopened.wait_for(|host| host.ready.is_file()).await;
    let after = observe(address).await;
    assert_eq!(after.0, before.0);
    assert!(after.1 > before.1);
    assert!(after.2 > before.2);
    println!(
        "native host WAL replay: cluster {}, ID {} -> {}, TSO {:?} -> {:?}",
        after.0, before.1, after.1, before.2, after.2
    );
    reopened.assert_no_children();
    reopened.signal("-USR1");
    reopened
        .wait_for(|host| host.logs().contains("host received SIGUSR1"))
        .await;
    reopened.signal("-TERM");
    assert!(reopened.wait().await.success(), "{}", reopened.logs());
    assert!(reopened.logs().contains("host closed native store"));
    drop(std::net::TcpListener::bind(address).unwrap());
}
