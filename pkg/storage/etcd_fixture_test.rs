// Copyright 2026 AsterSQL.
use astersql_pd_storage::EtcdMetaStore;
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
pub struct Fixture {
    child: Child,
    directory: PathBuf,
    pub endpoint: String,
}
impl Fixture {
    pub fn start() -> Self {
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
    pub fn command(&mut self, command: &str, acknowledgement: &str) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{command}").unwrap();
        self.wait_file(acknowledgement);
    }
    pub async fn store(&self) -> EtcdMetaStore {
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
