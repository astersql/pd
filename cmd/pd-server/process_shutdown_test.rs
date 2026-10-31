// Copyright 2026 AsterSQL.
use crate::main_test::fixture;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use std::{
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};
use tokio_stream::wrappers::ReceiverStream;

struct Process {
    child: Child,
    args: Vec<String>,
    dir: PathBuf,
    url: String,
    binary: Option<PathBuf>,
}
impl Process {
    fn start(endpoint: &str, binary: Option<PathBuf>, go: bool) -> Self {
        let client = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", client.local_addr().unwrap());
        let peer_url = format!("http://{}", peer.local_addr().unwrap());
        let dir = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join(format!(
            "process97-{}-{}",
            std::process::id(),
            client.local_addr().unwrap().port()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut args = vec![
            "--name=process97".into(),
            format!("--client-urls={url}"),
            format!("--peer-urls={peer_url}"),
        ];
        if go {
            args.extend([
                format!("--data-dir={}", dir.join("data").display()),
                format!("--initial-cluster=process97={peer_url}"),
            ]);
        } else {
            args.push(format!("--etcd-endpoints={endpoint}"));
        }
        drop((client, peer));
        let child = Self::spawn(&binary, &args, &dir);
        Self {
            child,
            args,
            dir,
            url,
            binary,
        }
    }
    fn spawn(binary: &Option<PathBuf>, args: &[String], dir: &std::path::Path) -> Child {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("log"))
            .unwrap();
        let mut command = Command::new(
            binary
                .clone()
                .unwrap_or_else(|| std::env::current_exe().unwrap()),
        );
        if binary.is_some() {
            command.args(args);
        } else {
            // Execute the real process entry (including its private executor and signal handlers)
            // in a separate OS process, without requiring a prebuilt binary for unit tests.
            command
                .args([
                    "--exact",
                    "cli_exit_test::task_091_process_entry",
                    "--nocapture",
                ])
                .env("PD_TASK091_CHILD_ARGS", args.join("\u{1f}"));
        }
        command
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap()
    }
    fn logs(&self) -> String {
        std::fs::read_to_string(self.dir.join("log")).unwrap()
    }
    async fn ready(&mut self) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "early exit: {}",
                self.logs()
            );
            if let Ok(mut client) = PdClient::connect(self.url.clone()).await {
                if let Ok(response) = client.get_members(pdpb::GetMembersRequest::default()).await {
                    let members = response.into_inner();
                    if members.leader.is_some() {
                        let output = Command::new("curl")
                            .args([
                                "-fsS",
                                "--max-time",
                                "2",
                                &format!("{}/pd/api/v1/health", self.url),
                            ])
                            .output()
                            .unwrap();
                        if output.status.success()
                            && serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
                                [0]["health"]
                                == true
                        {
                            return members.header.unwrap().cluster_id;
                        }
                    }
                }
            }
            assert!(Instant::now() < deadline, "readiness: {}", self.logs());
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }
    async fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "process did not exit: {}",
                self.logs()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    fn signal(&self, signal: &str) {
        assert!(
            Command::new("kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn terminal<T: prost::Message + Default>(stream: &mut tonic::Streaming<T>) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match stream.message().await {
                Ok(Some(_)) => continue,
                Ok(None) => return "EOF".into(),
                Err(error) => return format!("{:?}", error.code()),
            }
        }
    })
    .await
    .expect("live stream must terminate after signal")
}
async fn scenario(binary: Option<PathBuf>, go: bool, signal: &str) {
    tokio::time::timeout(Duration::from_secs(75), scenario_inner(binary, go, signal))
        .await
        .expect("signal shutdown scenario timeout");
}
async fn scenario_inner(binary: Option<PathBuf>, go: bool, signal: &str) {
    let fixture = fixture::Fixture::start();
    let mut process = Process::start(&fixture.endpoint, binary, go);
    let cluster_id = process.ready().await;
    let store = astersql_pd_storage::EtcdMetaStore::connect(
        &[if go {
            process.url.clone()
        } else {
            fixture.endpoint.clone()
        }],
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let mut client = PdClient::connect(process.url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    // Bootstrap is needed for the Go heartbeat/sync adapters; Rust uses the same RPC.
    let bootstrap = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29997".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                region_epoch: Some(metapb::RegionEpoch {
                    conf_ver: 1,
                    version: 1,
                }),
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    // A restarted durable cluster is already bootstrapped (only this first launch writes).
    assert!(bootstrap.header.unwrap().error.is_none());
    let first_id = client
        .alloc_id(pdpb::AllocIdRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    let (tso_tx, tso_rx) = tokio::sync::mpsc::channel(1);
    tso_tx
        .send(pdpb::TsoRequest {
            header: header.clone(),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    let mut tso = client
        .tso(ReceiverStream::new(tso_rx))
        .await
        .unwrap()
        .into_inner();
    assert!(tso.message().await.unwrap().unwrap().timestamp.is_some());
    let (hb_tx, hb_rx) = tokio::sync::mpsc::channel(1);
    hb_tx
        .send(pdpb::RegionHeartbeatRequest {
            header: header.clone(),
            region: Some(metapb::Region::default()),
            leader: Some(metapb::Peer {
                id: 3,
                store_id: 1,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut heartbeat = client
        .region_heartbeat(ReceiverStream::new(hb_rx))
        .await
        .unwrap()
        .into_inner();
    assert!(
        heartbeat
            .message()
            .await
            .unwrap()
            .unwrap()
            .header
            .unwrap()
            .error
            .is_some()
    );
    let (sync_tx, sync_rx) = tokio::sync::mpsc::channel(1);
    sync_tx
        .send(pdpb::SyncRegionRequest {
            header: header.clone(),
            member: Some(pdpb::Member {
                name: "peer97".into(),
                client_urls: vec!["http://127.0.0.1:29996".into()],
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut sync = client
        .sync_regions(ReceiverStream::new(sync_rx))
        .await
        .unwrap()
        .into_inner();
    loop {
        if sync.message().await.unwrap().unwrap().regions.is_empty() {
            break;
        }
    }
    let revision = store
        .transact(
            vec![],
            vec![Mutation::put(b"/global/config/task097", b"durable")],
        )
        .await
        .unwrap()
        .revision;
    let mut watch = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: "/global/config/task097".into(),
            revision,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        watch.message().await.unwrap().unwrap().changes[0].payload,
        b"durable"
    );
    let leader_key = format!("/pd/{cluster_id}/leader");
    let before = store
        .read(Read::range(
            leader_key.as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(before.entries.len(), 1);
    let lease = before.entries[0].lease.expect("leader uses real lease");
    process.signal(signal);
    let status = process.wait().await;
    // Go's documented process contract: SIGTERM=0; SIGINT=1 after graceful cleanup.
    assert_eq!(
        status.code(),
        Some(if signal == "-TERM" { 0 } else { 1 }),
        "{}",
        process.logs()
    );
    println!(
        "task097 go={go} signal={signal} exit={status} streams: tso={} heartbeat={} sync={} watch={}",
        terminal(&mut tso).await,
        terminal(&mut heartbeat).await,
        terminal(&mut sync).await,
        terminal(&mut watch).await
    );
    assert!(
        tokio::net::TcpStream::connect(process.url.trim_start_matches("http://"))
            .await
            .is_err()
    );
    let socket = std::net::TcpListener::bind(process.url.trim_start_matches("http://")).unwrap();
    drop(socket);
    if !go {
        assert_eq!(
            store.ttl(lease).await,
            Err(astersql_pd_storage::StoreError::LeaseNotFound)
        );
        assert!(
            store
                .read(Read::range(
                    leader_key.as_bytes(),
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries
                .is_empty()
        );
        assert_eq!(
            store
                .read(Read::range(
                    b"/global/config/task097",
                    b"",
                    Consistency::Linearizable
                ))
                .await
                .unwrap()
                .entries[0]
                .value,
            b"durable"
        );
    }
    drop((client, store, tso_tx, hb_tx, sync_tx));
    process.child = Process::spawn(&process.binary, &process.args, &process.dir);
    assert_eq!(process.ready().await, cluster_id);
    let mut client = PdClient::connect(process.url.clone()).await.unwrap();
    let next = client
        .alloc_id(pdpb::AllocIdRequest {
            header,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    assert!(next > first_id);
    // Observe the persistent global config through the reopened MetaStore (Go embeds it).
    let reopened = astersql_pd_storage::EtcdMetaStore::connect(
        &[if go {
            process.url.clone()
        } else {
            fixture.endpoint.clone()
        }],
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    assert_eq!(
        reopened
            .read(Read::range(
                b"/global/config/task097",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"durable"
    );
    process.signal("-TERM");
    assert!(process.wait().await.success());
    println!(
        "task097 go={go} signal={signal}: port/identity/leadership reused; ID increased; durable write survived"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_097_behavior() {
    for signal in ["-TERM", "-INT"] {
        scenario(None, false, signal).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_097_startup_failure() {
    startup_failure(None, false).await;
}
async fn startup_failure(binary: Option<PathBuf>, go: bool) {
    let fixture = fixture::Fixture::start();
    let mut process = Process::start(&fixture.endpoint, binary, go);
    process.ready().await;
    let dir = process.dir.with_extension("rejected");
    std::fs::create_dir(&dir).unwrap();
    let mut rejected = Process {
        child: Process::spawn(&process.binary, &process.args, &dir),
        args: process.args.clone(),
        url: process.url.clone(),
        binary: process.binary.clone(),
        dir,
    };
    assert_eq!(rejected.wait().await.code(), Some(1));
    assert!(
        rejected
            .logs()
            .to_lowercase()
            .contains("address already in use")
    );
    process.ready().await;
    process.signal("-TERM");
    assert!(process.wait().await.success());
    if !go {
        let mut bad_args = rejected.args.clone();
        *bad_args
            .iter_mut()
            .find(|arg| arg.starts_with("--etcd-endpoints="))
            .unwrap() = "--etcd-endpoints=http://127.0.0.1:1".into();
        rejected.child = Process::spawn(&rejected.binary, &bad_args, &rejected.dir);
        assert_eq!(rejected.wait().await.code(), Some(1));
        assert!(
            tokio::net::TcpStream::connect(rejected.url.trim_start_matches("http://"))
                .await
                .is_err()
        );
        drop(std::net::TcpListener::bind(rejected.url.trim_start_matches("http://")).unwrap());
    }
    rejected.child = Process::spawn(&rejected.binary, &rejected.args, &rejected.dir);
    rejected.ready().await;
    rejected.signal("-TERM");
    assert!(rejected.wait().await.success());
    println!(
        "task097 go={go}: bind failure exited/reaped, original remained healthy, retry acquired same port/identity; Rust backend-connect failure released bound listener"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires existing Go/Rust binaries and read-only supervisor"]
async fn task_097_readonly_differential() {
    startup_failure(
        Some(PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap())),
        true,
    )
    .await;
    startup_failure(
        Some(PathBuf::from(
            std::env::var_os("PD_COMPAT_RUST_BIN").unwrap(),
        )),
        false,
    )
    .await;
    for signal in ["-TERM", "-INT"] {
        scenario(
            Some(PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap())),
            true,
            signal,
        )
        .await;
        scenario(
            Some(PathBuf::from(
                std::env::var_os("PD_COMPAT_RUST_BIN").unwrap(),
            )),
            false,
            signal,
        )
        .await;
    }
}
