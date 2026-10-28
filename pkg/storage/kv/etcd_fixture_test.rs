// Copyright 2026 AsterSQL.
use astersql_pd_storage_kv::EtcdMetaStore;
use std::{
    io::Write,
    path::{Path, PathBuf},
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
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest
            .ancestors()
            .find(|root| root.join("scripts/rust-etcd-fixture").is_dir())
            .expect("repository fixture source");
        let target = PathBuf::from(
            std::env::var_os("CARGO_TARGET_DIR")
                .expect("claim Cargo shared slot and set absolute CARGO_TARGET_DIR first"),
        );
        assert!(target.is_absolute());
        let binary = BINARY.get_or_init(|| {
            let binary = target.join(format!("rust-etcd-fixture-{}", std::process::id()));
            let status = fixture_go(root)
                .env("GOWORK", "off")
                .env("GOFLAGS", "-mod=readonly")
                .env("CGO_ENABLED", "0")
                .env("GOPROXY", "off")
                .env("GOTOOLCHAIN", "local")
                .args(["build", "-mod=readonly", "-o"])
                .arg(&binary)
                .arg("./scripts/rust-etcd-fixture")
                .current_dir(&root)
                .status()
                .unwrap();
            assert!(status.success(), "Go etcd fixture build failed");
            binary
        });
        let work = std::env::var_os("PD_COMPAT_WORK_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| target.clone());
        let directory = work.join(format!(
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
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

// Prefer the already installed toolchain matching go.mod. GOTOOLCHAIN=local
// rejects a missing toolchain instead of downloading Go sources behind the guard.
fn fixture_go(root: &Path) -> Command {
    let output = Command::new("go")
        .args(["env", "GOMODCACHE", "GOOS", "GOARCH"])
        .env("GOTOOLCHAIN", "local")
        .env("GOWORK", "off")
        .current_dir(root)
        .output()
        .expect("locate existing Go toolchain cache");
    assert!(output.status.success(), "cannot locate Go toolchain cache");
    let environment = String::from_utf8(output.stdout).unwrap();
    let values: Vec<_> = environment.lines().collect();
    assert_eq!(values.len(), 3);
    let module = std::fs::read_to_string(root.join("go.mod")).unwrap();
    let version = module
        .lines()
        .find_map(|line| line.strip_prefix("go "))
        .unwrap()
        .trim();
    let cached = PathBuf::from(values[0]).join(format!(
        "golang.org/toolchain@v0.0.1-go{version}.{}-{}/bin/go",
        values[1], values[2]
    ));
    Command::new(if cached.is_file() {
        cached
    } else {
        PathBuf::from("go")
    })
}
