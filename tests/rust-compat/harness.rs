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
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Server {
    child: Child,
    pub dir: PathBuf,
    pub url: String,
    pub peer: String,
}
impl Server {
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
                    if let (Ok((200, members)), Ok((200, config))) = (
                        self.get("/pd/api/v1/members"),
                        self.get("/pd/api/v1/config"),
                    ) {
                        if members["leader"]["name"] == "compat" && config["schedule"].is_object() {
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
