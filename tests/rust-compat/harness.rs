// Copyright 2026 AsterSQL.
use crate::normalize::{Rules, differences};
use serde_json::{Value, json};
use std::{
    fs, io,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
#[allow(dead_code)]
#[path = "../../pkg/storage/etcd_fixture_test.rs"]
mod fixture;
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Server {
    child: Child,
    _etcd: Option<fixture::Fixture>,
    pub dir: PathBuf,
    pub url: String,
    pub peer: String,
}
impl Server {
    pub fn start_rust(binary: &Path) -> io::Result<Self> {
        let etcd = fixture::Fixture::start();
        let mut server =
            Self::start_with_args(binary, &[&format!("--etcd-endpoints={}", etcd.endpoint)])?;
        server._etcd = Some(etcd);
        Ok(server)
    }
    /// Send SIGTERM and require graceful, successful exit before releasing the fixture.
    pub fn stop(&mut self) -> Result<(), String> {
        let status = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("SIGTERM failed".into());
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                if !status.success() {
                    return Err(format!("shutdown: {status}; {}", self.logs()));
                }
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(format!("graceful shutdown timed out: {}", self.logs()))
    }
    pub fn start(binary: &Path) -> io::Result<Self> {
        Self::start_with_args(binary, &[])
    }
    pub fn start_with_args(binary: &Path, extra: &[&str]) -> io::Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "pd-compat-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir)?;
        let result = (|| {
            // Hold both reservations until immediately before spawn; retry at orchestration
            // level on bind races. PD owns the sockets once launched.
            let client = TcpListener::bind("127.0.0.1:0")?;
            let peer = TcpListener::bind("127.0.0.1:0")?;
            let url = format!("http://{}", client.local_addr()?);
            let peer_url = format!("http://{}", peer.local_addr()?);
            let log = fs::File::create(dir.join("server.log"))?;
            let mut command = Command::new(binary);
            command.args(extra);
            command
                .args([
                    "--name=compat",
                    &format!("--data-dir={}", dir.join("data").display()),
                    &format!("--client-urls={url}"),
                    &format!("--advertise-client-urls={url}"),
                    &format!("--peer-urls={peer_url}"),
                    &format!("--advertise-peer-urls={peer_url}"),
                    &format!("--initial-cluster=compat={peer_url}"),
                ])
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log));
            drop((client, peer));
            Ok(Self {
                child: command.spawn()?,
                _etcd: None,
                dir: dir.clone(),
                url,
                peer: peer_url,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&dir);
        }
        result
    }
    pub fn ready(&mut self) -> Result<(), String> {
        self.ready_with_timeout(Duration::from_secs(40))
    }
    pub fn ready_with_timeout(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!("server exited {status}: {}", self.logs()));
            }
            if let Ok((200, body)) = self.get("/pd/api/v1/health") {
                if body
                    .as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(|m| m["health"] == true))
                {
                    if let Ok((200, members)) = self.get("/pd/api/v1/members") {
                        if members["leader"]["name"] == "compat" {
                            return Ok(());
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!("readiness timeout: {}", self.logs()))
    }
    pub fn logs(&self) -> String {
        fs::read_to_string(self.dir.join("server.log")).unwrap_or_default()
    }
    pub fn get(&self, path: &str) -> Result<(u16, Value), String> {
        let output = Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--max-time",
                "2",
                "--write-out",
                "\n%{http_code}",
                &format!("{}{path}", self.url),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        let body = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        let (body, status) = body.rsplit_once('\n').ok_or("missing HTTP status")?;
        Ok((
            status.parse::<u16>().map_err(|e| e.to_string())?,
            serde_json::from_str(body)
                .map_err(|e| format!("{path} HTTP {status}: {e}; body={body}"))?,
        ))
    }
    pub fn sample(&self) -> Result<Value, String> {
        let (hs, h) = self.get("/pd/api/v1/health")?;
        let (ms, m) = self.get("/pd/api/v1/members")?;
        let (cs, c) = self.get("/pd/api/v1/config")?;
        if h.as_array().map(Vec::len) != Some(1)
            || h[0]["name"] != "compat"
            || h[0]["health"] != true
            || !h[0]["member_id"].as_u64().is_some_and(|id| id > 0)
            || h[0]["client_urls"] != json!([self.url])
        {
            return Err(format!("invalid health oracle: {h}"));
        }
        if m["members"].as_array().map(Vec::len) != Some(1)
            || m["members"][0]["member_id"] != h[0]["member_id"]
            || m["leader"]["member_id"] != h[0]["member_id"]
        {
            return Err(format!("invalid membership oracle: {m}"));
        }
        let rules = Rules(vec![
            (h[0]["member_id"].clone(), json!("<member:compat>")),
            (m["header"]["cluster_id"].clone(), json!("<cluster>")),
            (json!(self.url), json!("<client>")),
            (json!(self.peer), json!("<peer>")),
        ]);
        Ok(rules.apply(&json!({"health_status":hs,"health":h,"members_status":ms,"members":m,"config_status":cs,"config":{"schedule.max-snapshot-count":c["schedule"]["max-snapshot-count"],"replication.max-replicas":c["replication"]["max-replicas"]}})))
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}
pub fn run(mode: &str, go: &Path, rust: Option<&Path>) -> Result<(), String> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/health-member.json")).unwrap();
    let mut results = Vec::new();
    for binary in match mode {
        "go" => vec![go],
        "rust" => vec![rust.ok_or("PD_COMPAT_RUST_BIN required")?],
        "dual" => vec![go, rust.ok_or("PD_COMPAT_RUST_BIN required")?],
        _ => return Err("mode must be go, rust or dual".into()),
    } {
        let mut server = Server::start(binary).map_err(|e| e.to_string())?;
        server.ready()?;
        let sample = server.sample()?;
        // Reviewed assertions are a subset; full responses remain in differential comparison.
        for (key, expected) in fixture.as_object().unwrap() {
            if sample[key] != *expected {
                return Err(format!("fixture {key}: {} != {expected}", sample[key]));
            }
        }
        results.push(sample);
    }
    if results.len() == 2 {
        let diff = differences(&results[0], &results[1]);
        if !diff.is_empty() {
            return Err(diff.join("\n"));
        }
    }
    Ok(())
}

/// Baseline for the vertical MVP. Embedded-etcd topology and build/deployment metadata
/// are excluded explicitly; every supported PD identity, header and health field is compared.
pub fn minimal_sample(server: &Server) -> Result<Value, String> {
    let (hs, h) = server.get("/pd/api/v1/health")?;
    let (ms, m) = server.get("/pd/api/v1/members")?;
    if hs != 200
        || ms != 200
        || h.as_array().map(Vec::len) != Some(1)
        || h[0]["health"] != true
        || m["members"].as_array().map(Vec::len) != Some(1)
    {
        return Err(format!("invalid health/members: {hs} {h}; {ms} {m}"));
    }
    let member_id = h[0]["member_id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("member id missing")?;
    let cluster_id = m["header"]["cluster_id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("cluster id missing")?;
    if m["members"][0]["member_id"] != member_id
        || m["leader"]["member_id"] != member_id
        || h[0]["client_urls"] != json!([server.url])
    {
        return Err("inconsistent PD identity".into());
    }
    for key in m
        .as_object()
        .ok_or("members response must be an object")?
        .keys()
    {
        if ![
            "header",
            "members",
            "leader",
            "etcd_leader",
            "tso_allocator_leaders",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unreviewed members response field: {key}"));
        }
    }
    let member = crate::normalize::minimal_member(&m["members"][0])?;
    let leader = crate::normalize::minimal_member(&m["leader"])?;
    let rules = Rules(vec![
        (json!(member_id), json!("<member:compat>")),
        (json!(cluster_id), json!("<cluster>")),
        (json!(server.url), json!("<client>")),
        (json!(server.peer), json!("<peer>")),
    ]);
    Ok(rules.apply(&json!({"health_status":hs,"health":h,"members_status":ms,"header":m["header"],"members":[member],"leader":leader,"tso_allocator_leaders":m.get("tso_allocator_leaders").cloned().unwrap_or(json!({}))})))
}
pub fn run_minimal(mode: &str, go: &Path, rust: Option<&Path>) -> Result<(), String> {
    let mut samples = Vec::new();
    for is_rust in match mode {
        "go" => vec![false],
        "rust" => vec![true],
        "dual" => vec![false, true],
        _ => return Err("invalid mode".into()),
    } {
        let mut server = if is_rust {
            Server::start_rust(rust.ok_or("Rust binary required")?)
        } else {
            Server::start(go)
        }
        .map_err(|e| e.to_string())?;
        server.ready()?;
        samples.push(minimal_sample(&server)?);
        server.stop()?;
        if is_rust && !server.logs().contains("storage-released") {
            return Err(server.logs());
        }
        let address = server.url.trim_start_matches("http://").to_owned();
        let dir = server.dir.clone();
        drop(server);
        if dir.exists() || std::net::TcpStream::connect(address).is_ok() {
            return Err("process resources leaked".into());
        }
    }
    if samples.len() == 2 {
        let diff = differences(&samples[0], &samples[1]);
        if !diff.is_empty() {
            return Err(diff.join("\n"));
        }
    }
    println!("health-member: supported PD fields equal; graceful process cleanup verified");
    Ok(())
}
