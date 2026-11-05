// Copyright 2026 AsterSQL.
use astersql_pd_storage_kv::EtcdMetaStore;
use std::{
    io::{Read as _, Write},
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
    native: Option<(PathBuf, Vec<String>)>,
}
/// Restores a deliberately suspended fixture even when an assertion unwinds.
pub struct Suspension<'a>(&'a mut Fixture);
impl Drop for Suspension<'_> {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-CONT", &self.0.child.id().to_string()])
            .status();
    }
}
impl Fixture {
    /// Suspend only this fixture's existing process, retaining sockets and WAL.
    pub fn suspend(&mut self) -> Suspension<'_> {
        assert!(
            Command::new("kill")
                .args(["-STOP", &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        Suspension(self)
    }
    pub fn start() -> Self {
        Self::start_with_native_ports(None)
    }
    /// Supply the first reservation to exercise an actual initial bind collision.
    pub fn start_with_native_ports(
        first_ports: Option<(std::net::TcpListener, std::net::TcpListener)>,
    ) -> Self {
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
        let work = std::env::var_os("PD_COMPAT_WORK_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| target.clone());
        let directory = work.join(format!(
            "etcd-contract-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        if let Some(binary) = std::env::var_os("PD_COMPAT_ETCD_BIN") {
            return Self::start_native(PathBuf::from(binary), directory, first_ports);
        }
        assert!(
            first_ports.is_none(),
            "reserved ports require the pinned native fixture"
        );
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
            native: None,
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
        if self.native.is_some() {
            match command {
                "stop" => terminate_native(&mut self.child),
                "start" => {
                    if self.child.try_wait().unwrap().is_some() {
                        let (binary, args) = self.native.as_ref().unwrap();
                        self.child = spawn_native(binary, args, &self.directory);
                        self.wait_native_ready();
                    }
                }
                _ => panic!("unknown native etcd fixture command: {command}"),
            }
            return;
        }
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
        if self.native.is_some() {
            terminate_native(&mut self.child);
            let _ = std::fs::remove_dir_all(&self.directory);
            return;
        }
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

fn spawn_native(binary: &Path, args: &[String], directory: &Path) -> Child {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("etcd.log"))
        .unwrap();
    Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .unwrap()
}
fn terminate_native(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = Command::new("kill")
        .args(["-CONT", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
}
impl Fixture {
    fn start_native(
        binary: PathBuf,
        directory: PathBuf,
        mut first_ports: Option<(std::net::TcpListener, std::net::TcpListener)>,
    ) -> Self {
        assert!(
            binary.is_absolute() && binary.is_file(),
            "PD_COMPAT_ETCD_BIN must name an existing absolute binary"
        );
        let version = Command::new(&binary).arg("--version").output().unwrap();
        assert!(version.status.success());
        assert!(
            String::from_utf8_lossy(&version.stdout)
                .lines()
                .any(|line| line == "etcd Version: 3.5.15"),
            "fixture must use pinned etcd 3.5.15: {version:?}"
        );
        for attempt in 0..5 {
            std::fs::create_dir_all(&directory).unwrap();
            let (client, peer) = first_ports.take().unwrap_or_else(|| {
                (
                    std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
                    std::net::TcpListener::bind("127.0.0.1:0").unwrap(),
                )
            });
            let endpoint = format!("http://{}", client.local_addr().unwrap());
            let peer_url = format!("http://{}", peer.local_addr().unwrap());
            let args = vec![
                "--name=default".into(),
                format!("--data-dir={}", directory.join("data").display()),
                format!("--listen-client-urls={endpoint}"),
                format!("--advertise-client-urls={endpoint}"),
                format!("--listen-peer-urls={peer_url}"),
                format!("--initial-advertise-peer-urls={peer_url}"),
                format!("--initial-cluster=default={peer_url}"),
                "--log-level=error".into(),
            ];
            drop(client);
            drop(peer);
            let child = spawn_native(&binary, &args, &directory);
            let mut fixture = Self {
                child,
                directory: directory.clone(),
                endpoint,
                native: Some((binary.clone(), args)),
            };
            match fixture.try_wait_native_ready() {
                Ok(()) => return fixture,
                Err(error)
                    if attempt < 4
                        && error.starts_with("native etcd exited:")
                        && error.contains("bind: address already in use") =>
                {
                    eprintln!(
                        "initial native bind collision; retrying reservation {}/5: {error}",
                        attempt + 1
                    );
                    drop(fixture);
                }
                Err(error) => panic!("{error}"),
            }
        }
        unreachable!("each initial reservation returns or reports its final failure")
    }
    fn wait_native_ready(&mut self) {
        self.try_wait_native_ready()
            .unwrap_or_else(|error| panic!("{error}"));
    }
    fn try_wait_native_ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(25);
        let address = self
            .endpoint
            .strip_prefix("http://")
            .unwrap()
            .parse()
            .unwrap();
        loop {
            if self.child.try_wait().unwrap().is_some() {
                return Err(format!(
                    "native etcd exited: {}: {}",
                    self.directory.display(),
                    std::fs::read_to_string(self.directory.join("etcd.log")).unwrap_or_default()
                ));
            }
            if let Ok(mut stream) =
                std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200))
            {
                stream
                    .set_read_timeout(Some(Duration::from_millis(500)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_millis(500)))
                    .unwrap();
                if stream
                    .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
                    .is_ok()
                {
                    let mut bytes = Vec::new();
                    if stream.read_to_end(&mut bytes).is_ok() && bytes.starts_with(b"HTTP/1.0 200")
                    {
                        return Ok(());
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "native etcd readiness timeout: {}: {}",
                    self.directory.display(),
                    std::fs::read_to_string(self.directory.join("etcd.log")).unwrap_or_default()
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
