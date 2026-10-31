// Copyright 2026 AsterSQL.
//! Real process coverage for the HTTPS CLI and signal-driven shutdown.
#[allow(dead_code)]
#[path = "../../pkg/storage/kv/etcd_fixture_test.rs"]
mod fixture;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            let _ = self.0.kill();
            self.0.wait().unwrap();
        }
    }
}
#[test]
fn task_083_https_process_shutdown() {
    let fixture = fixture::Fixture::start();
    let directory = std::path::PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap())
        .join(format!("process-tls83-{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _cleanup = Cleanup(directory.clone());
    assert!(
        Command::new("python3")
            .args(["../../scripts/service-auth-oracle.py", "--certificates"])
            .arg(&directory)
            .status()
            .unwrap()
            .success()
    );
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    drop(socket);
    let log = std::fs::File::create(directory.join("process.log")).unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_pd-server"))
            .args([
                format!("--etcd-endpoints={}", fixture.endpoint),
                format!("--client-urls=https://{address}"),
                format!("--cert={}", directory.join("server.pem").display()),
                format!("--key={}", directory.join("server.key").display()),
                format!("--cacert={}", directory.join("ca.pem").display()),
            ])
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "{}",
            std::fs::read_to_string(directory.join("process.log")).unwrap()
        );
        let response = Command::new("curl")
            .args(["-fsS", "--max-time", "1", "--http1.1", "--cacert"])
            .arg(directory.join("ca.pem"))
            .arg("--cert")
            .arg(directory.join("allowed.pem"))
            .arg("--key")
            .arg(directory.join("allowed.key"))
            .arg(format!("https://{address}/pd/api/v1/ping"))
            .output()
            .unwrap();
        if response.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "HTTPS process readiness timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        Command::new("kill")
            .args(["-TERM", &process.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "HTTPS process shutdown timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::net::TcpListener::bind(address).unwrap();
    assert!(
        std::fs::read_to_string(directory.join("process.log"))
            .unwrap()
            .contains("PD stopped")
    );
}
