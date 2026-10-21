// Copyright 2026 AsterSQL.
use astersql_pd_storage::*;
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
#[path = "contract_test.rs"]
mod contract;

struct Fixture {
    child: Child,
    directory: PathBuf,
    endpoint: String,
}
impl Fixture {
    fn start() -> Self {
        static BINARY: OnceLock<PathBuf> = OnceLock::new();
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = PathBuf::from(
            std::env::var_os("CARGO_TARGET_DIR")
                .expect("claim Cargo shared slot and set absolute CARGO_TARGET_DIR first"),
        );
        assert!(target.is_absolute());
        let binary = BINARY.get_or_init(|| {
            let binary = target.join(format!("rust-etcd-fixture-{}", std::process::id()));
            let status = Command::new("go")
                .args(["build", "-o"])
                .arg(&binary)
                .arg("./scripts/rust-etcd-fixture")
                .current_dir(&root)
                .status()
                .unwrap();
            assert!(status.success(), "Go etcd fixture build failed");
            binary
        });
        let directory = target.join(format!(
            "etcd-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let child = Command::new(binary)
            .arg(&directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut fixture = Self {
            child,
            directory,
            endpoint: String::new(),
        };
        fixture.wait_file("ready");
        fixture.endpoint = std::fs::read_to_string(fixture.directory.join("ready")).unwrap();
        fixture
    }
    fn wait_file(&mut self, name: &str) {
        let deadline = Instant::now() + Duration::from_secs(25);
        while !self.directory.join(name).exists() {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "etcd fixture exited: {}",
                self.directory.display()
            );
            assert!(Instant::now() < deadline, "etcd fixture timeout: {name}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn command(&mut self, command: &str, acknowledgement: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
        self.wait_file(acknowledgement);
    }
    async fn store(&self) -> EtcdMetaStore {
        EtcdMetaStore::connect(std::slice::from_ref(&self.endpoint), Duration::from_secs(2))
            .await
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.child.stdin.take()); // EOF requests graceful shutdown, including on test panic.
        let deadline = Instant::now() + Duration::from_secs(5);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
macro_rules! contract_case {
    ($($name:ident),* $(,)?) => {$ (
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() { let fixture = Fixture::start(); contract::$name(&fixture.store().await).await; }
    )*};
}
contract_case!(
    cas_atomic_revision,
    snapshot_scan,
    watch_replay,
    compaction_recovery,
    lease_expiry,
    lease_renew_revoke,
    election_fencing,
    invalid_atomic,
    watch_cancel
);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnect_restart_resume_and_cancel() {
    let mut fixture = Fixture::start();
    let store = fixture.store().await;
    let first = contract::put(&store, b"/fault/a", b"one").await;
    let mut watch = store
        .watch(b"/fault/".to_vec(), first.revision)
        .await
        .unwrap();
    assert_eq!(
        contract::batch(watch.as_mut()).await.revision,
        first.revision
    );
    let lease = store.grant(2).await.unwrap();
    store
        .transact(
            vec![],
            vec![Mutation::Put {
                key: b"/fault/leased".to_vec(),
                value: vec![],
                lease: Some(lease.id),
            }],
        )
        .await
        .unwrap();
    let leased_batch = contract::batch(watch.as_mut()).await;
    fixture.command("stop", "stopped");
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        watch.next(&CancellationToken::new()),
    )
    .await
    .expect("stream failed to notice disconnect")
    .unwrap_err();
    assert_eq!(error, StoreError::Unavailable);
    assert_eq!(
        store
            .read(Read::prefix(b"/fault/", Consistency::Linearizable))
            .await
            .unwrap_err(),
        StoreError::Unavailable
    );
    // Cancellation also interrupts reconnect when the endpoint is down.
    let cancel = CancellationToken::new();
    let mut cancelled = store
        .watch(b"/fault/".to_vec(), leased_batch.revision + 1)
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(
        cancelled.next(&cancel).await.unwrap_err(),
        StoreError::Cancelled
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    fixture.command("start", "restarted");
    let recovered = fixture.store().await;
    let commit = contract::put(&recovered, b"/fault/after", b"two").await;
    let mut observed = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        match tokio::time::timeout_at(deadline, watch.next(&CancellationToken::new()))
            .await
            .unwrap()
        {
            Ok(batch) => {
                assert!(
                    batch
                        .events
                        .iter()
                        .all(|e| e.revision() > leased_batch.revision)
                );
                let reached = batch.revision >= commit.revision;
                observed.extend(batch.events);
                if reached {
                    break;
                }
            }
            Err(StoreError::Unavailable) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(e) => panic!("unexpected resume error: {e}"),
        }
    }
    assert!(observed.iter().any(|e| e.key() == b"/fault/after"));
    // Restart does not guarantee immediate lease expiry; wait for committed invalidation.
    let end = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match recovered.ttl(lease.id).await {
            Err(StoreError::LeaseNotFound) => break,
            Ok(_) => {
                assert!(tokio::time::Instant::now() < end);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => panic!("lease recovery: {e}"),
        }
    }
    assert!(
        contract::read(&recovered, b"/fault/leased")
            .await
            .entries
            .is_empty()
    );
    assert_eq!(
        recovered.keep_alive(lease.id).await.unwrap_err(),
        StoreError::LeaseNotFound
    );
    eprintln!(
        "fault recovery: disconnected, resumed after {}, persisted commit {}, lease {} expired",
        leased_batch.revision, commit.revision, lease.id
    );
}
