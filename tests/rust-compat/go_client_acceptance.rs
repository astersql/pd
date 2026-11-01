// Copyright 2026 AsterSQL.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::Duration,
};

pub(crate) struct Directory(pub PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs an existing Go tool unchanged; captures its own assertions and completed request counts.
pub(crate) struct GoTool {
    child: Child,
    log: PathBuf,
}
impl GoTool {
    pub(crate) fn start(binary: &Path, args: &[String], log: PathBuf) -> Self {
        let output = fs::File::create(&log).unwrap();
        let child = Command::new(binary)
            .args(args)
            .env("GOFLAGS", "-mod=readonly")
            .env("GOWORK", "off")
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap_or_else(|e| panic!("existing Go tool {}: {e}", binary.display()));
        Self { child, log }
    }
    pub(crate) fn output(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }
    pub(crate) fn samples(&self) -> usize {
        self.output()
            .lines()
            .filter_map(|line| line.strip_prefix("count: "))
            .filter_map(|line| line.split(',').next()?.parse::<u64>().ok())
            .filter(|count| *count > 0)
            .count()
    }
    pub(crate) async fn wait_samples(&mut self, previous: usize) {
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                assert!(
                    self.child.try_wait().unwrap().is_none(),
                    "Go client exited before completing requests: {}",
                    self.output()
                );
                if self.samples() > previous {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Go client made no progress: {}", self.output()));
    }
    pub(crate) async fn cancel(&mut self) {
        assert!(
            Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let status = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("Go client cancellation did not finish");
        assert!(status.success(), "Go tool: {status}: {}", self.output());
        let output = self.output();
        assert!(!output.contains("[FATAL]"), "{output}");
        assert!(!output.contains("unmatched region"), "{output}");
        println!("Go tool {} exit={status}\n{output}", self.log.display());
    }
}
impl Drop for GoTool {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
