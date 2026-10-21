// Copyright 2026 AsterSQL.
use astersql_pd_raft_prototype::Cluster;
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Disk(PathBuf);
impl Disk {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pd-native-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Disk {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn start(disk: &Disk) -> Cluster {
    let mut c = Cluster::new(&disk.0).unwrap();
    c.campaign(0).unwrap();
    c
}
#[test]
fn cas_linear_history_and_watch_revisions() {
    let disk = Disk::new();
    let mut c = start(&disk);
    for rev in 0..20 {
        c.cas(0, "key", &format!("v{rev}"), rev).unwrap();
        assert_eq!(c.read(0).unwrap(), (rev + 1, Some(format!("v{rev}"))));
        c.cas(0, "key", "conflicting", 0).unwrap();
        assert_eq!(c.revision(0), rev + 1);
    }
    for id in 0..3 {
        assert_eq!(
            c.events(id).iter().map(|e| e.0).collect::<Vec<_>>(),
            (1..=20).collect::<Vec<_>>()
        );
    }
    println!(
        "linear-history: 20 acknowledged CAS + reads, 20 failed compares; watch 1..20 on 3 nodes"
    );
}
#[test]
fn leader_kill_partition_and_rejoin() {
    let disk = Disk::new();
    let mut c = start(&disk);
    c.cas(0, "key", "before", 0).unwrap();
    c.isolate(0, true);
    assert!(c.read(0).is_err());
    assert!(c.cas(0, "key", "minority", 1).is_err());
    c.kill(0);
    c.tick(30).unwrap();
    let leader = c.leader().unwrap();
    c.cas(leader, "key", "majority", 1).unwrap();
    assert_eq!(c.read(leader).unwrap(), (2, Some("majority".into())));
    c.restart(0).unwrap();
    c.isolate(0, false);
    c.tick(3).unwrap();
    for id in 0..3 {
        assert_eq!(c.revision(id), 2);
        assert_eq!(c.events(id)[1].2, "majority");
    }
    println!(
        "partition: minority read/write refused; leader kill, re-election, conflicting suffix replacement passed"
    );
}
#[test]
fn snapshot_restore_and_watch_resume() {
    let disk = Disk::new();
    let mut c = start(&disk);
    for rev in 0..10 {
        c.cas(0, "key", "snapshot", rev).unwrap();
    }
    for id in 0..3 {
        c.snapshot(id).unwrap();
        c.kill(id);
    }
    let now = Instant::now();
    let mut c = Cluster::new(&disk.0).unwrap();
    c.campaign(1).unwrap();
    assert_eq!(c.read(1).unwrap(), (10, Some("snapshot".into())));
    c.cas(1, "key", "after", 10).unwrap();
    for id in 0..3 {
        assert_eq!(
            c.events(id).iter().map(|e| e.0).collect::<Vec<_>>(),
            (1..=11).collect::<Vec<_>>()
        );
    }
    println!(
        "snapshot: 3 disk snapshots restored, revision/watch 1..11; recovery_us={}",
        now.elapsed().as_micros()
    );
}
#[test]
fn corruption_is_explicit_not_silent_data_loss() {
    for truncate in [false, true] {
        let disk = Disk::new();
        let mut c = start(&disk);
        c.cas(0, "key", "durable", 0).unwrap();
        drop(c);
        let path = disk.0.join("node-1.wal");
        let mut bytes = fs::read(&path).unwrap();
        if truncate {
            bytes.truncate(bytes.len() / 2);
        } else {
            let last = bytes.len() - 1;
            bytes[last] ^= 1;
        }
        fs::write(path, bytes).unwrap();
        assert_eq!(
            Cluster::new(&disk.0).err().unwrap().kind(),
            std::io::ErrorKind::InvalidData
        );
    }
    println!("corruption: checksum bit flip and torn journal both refuse recovery");
}
#[test]
fn incomplete_snapshot_publication_preserves_previous_image() {
    let disk = Disk::new();
    let mut c = start(&disk);
    c.cas(0, "key", "old", 0).unwrap();
    drop(c);
    fs::write(disk.0.join("node-1.pending"), b"torn candidate snapshot").unwrap();
    let mut c = Cluster::new(&disk.0).unwrap();
    c.campaign(0).unwrap();
    assert_eq!(c.read(0).unwrap(), (1, Some("old".into())));
}
struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn process_kill_and_restart_from_fsynced_disks() {
    let disk = Disk::new();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_worker", "--nocapture"])
        .env("PD_RAFT_WORKER", &disk.0)
        .spawn()
        .unwrap();
    let mut worker = Worker(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !disk.0.join("ack").exists() {
        assert!(
            Instant::now() < deadline,
            "worker never acknowledged commit"
        );
        assert!(
            worker.0.try_wait().unwrap().is_none(),
            "worker exited before kill"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    worker.0.kill().unwrap();
    let status = worker.0.wait().unwrap();
    assert!(!status.success());
    let now = Instant::now();
    let mut c = Cluster::new(&disk.0).unwrap();
    c.campaign(2).unwrap();
    assert_eq!(c.read(2).unwrap(), (1, Some("process".into())));
    c.cas(2, "key", "recovered", 1).unwrap();
    assert_eq!(
        c.events(2).iter().map(|e| e.0).collect::<Vec<_>>(),
        vec![1, 2]
    );
    println!(
        "SIGKILL recovery_us={}; acknowledged data/revision/watch retained",
        now.elapsed().as_micros()
    );
}
#[test]
fn process_worker() {
    let Some(path) = std::env::var_os("PD_RAFT_WORKER") else {
        return;
    };
    let path = PathBuf::from(path);
    let mut c = Cluster::new(&path).unwrap();
    c.campaign(0).unwrap();
    c.cas(0, "key", "process", 0).unwrap();
    fs::write(path.join("ack"), b"committed").unwrap();
    loop {
        std::thread::park();
    }
}

/// An acceptance gate that records the candidate's disqualifying limitation.
/// Passing means rejection was reproduced, not that remote snapshots work.
#[test]
fn rejection_gate_lagging_member_needs_real_snapshot_storage() {
    let disk = Disk::new();
    let mut c = start(&disk);
    c.cas(0, "key", "first", 0).unwrap();
    c.kill(2);
    for rev in 1..5 {
        c.cas(0, "key", "compacted", rev).unwrap();
    }
    c.snapshot(0).unwrap();
    c.snapshot(1).unwrap();
    drop(c);
    let mut c = Cluster::new(&disk.0).unwrap();
    c.isolate(2, true);
    c.campaign(0).unwrap();
    c.isolate(2, false);
    let error = c.tick(3).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    println!("REJECT native-backend candidate: {error}");
}
