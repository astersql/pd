// Copyright 2026 AsterSQL.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use rusqlite::OptionalExtension;

use super::candidate::Candidate;

const CHILD: &str = "candidate_commit_test::candidate_commit_child";

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "subprocess entry for the commit boundary test"]
fn candidate_commit_child() {
    let path = std::env::var_os("PD_CANDIDATE_COMMIT_DB").unwrap();
    let stage = std::env::var("PD_CANDIDATE_COMMIT_STAGE").unwrap();
    let mut store = Candidate::open(Path::new(&path)).unwrap();
    let tx = store.db.transaction().unwrap();
    tx.execute("INSERT INTO state VALUES ('new', X'02')", [])
        .unwrap();
    if stage != "before_sync" {
        tx.commit().unwrap();
    }
    println!("READY {stage}");
    io::stdout().flush().unwrap();
    let mut line = String::new();
    io::stdin().read_line(&mut line).unwrap();
    panic!("parent must interrupt the child at the reported boundary");
}

#[test]
fn theme10_candidate_commit_boundaries() {
    for stage in ["before_sync", "synced_without_ack", "acknowledged"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("meta.db");
        let seed = Candidate::open(&path).unwrap();
        seed.db
            .execute("INSERT INTO state VALUES ('seed', X'01')", [])
            .unwrap();
        seed.db.close().unwrap();

        let mut child = Process(
            Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", CHILD, "--nocapture"])
                .env("PD_CANDIDATE_COMMIT_DB", &path)
                .env("PD_CANDIDATE_COMMIT_STAGE", stage)
                .stdout(Stdio::piped())
                .stdin(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let lines = BufReader::new(child.0.stdout.take().unwrap()).lines();
        assert!(
            lines
                .map(Result::unwrap)
                .any(|line| line == format!("READY {stage}"))
        );
        let ack = temp.path().join("client.ack");
        if stage == "acknowledged" {
            let mut file = File::create(&ack).unwrap();
            file.write_all(&[2]).unwrap();
            file.sync_all().unwrap();
        }
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());

        let reopened = Candidate::open(&path).unwrap();
        let get = |key| -> Option<Vec<u8>> {
            reopened
                .db
                .query_row("SELECT payload FROM state WHERE name=?1", [key], |row| {
                    row.get(0)
                })
                .optional()
                .unwrap()
        };
        assert_eq!(get("seed"), Some(vec![1]));
        assert_eq!(get("new"), (stage != "before_sync").then_some(vec![2]));
        if stage == "acknowledged" {
            assert_eq!(get("new"), Some(fs::read(&ack).unwrap()));
        } else {
            assert!(!ack.exists());
        }
        println!("commit_boundary={stage} recovered={:?}", get("new"));
    }
}
