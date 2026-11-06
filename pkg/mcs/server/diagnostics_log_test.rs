// Copyright 2026 AsterSQL.
use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "sa014-server-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn file(&self, name: &str, bytes: &str) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_str().unwrap().into()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn sa_014_iterator_boundaries() {
    let dir = Directory::new();
    let path=dir.file("pd.log", "[2020/01/01 08:00:00.000 +08:00] [INFO] [field=1] [field=2]\r\ncontinuation\r\n[2020/01/01 00:00:00.001 +00:00] [MiXeD] [unknown]\n[2020/01/01 00:00:00.002 +00:00] [DEBUG] [debug]");
    let batches = Mutex::new(Vec::new());
    search(
        &path,
        SearchLogRequest {
            start_time: 1577836800000,
            end_time: 1577836800001,
            levels: vec![2],
            ..Default::default()
        },
        &CancellationToken::new(),
        &|b| {
            batches.lock().unwrap().push(b.unwrap());
            true
        },
    )
    .unwrap();
    let batches = batches.into_inner().unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches[0].messages,
        vec![
            LogMessage {
                time: 1577836800000,
                level: 2,
                message: "[field=1] [field=2]".into()
            },
            LogMessage {
                time: 1577836800000,
                level: 2,
                message: "continuation".into()
            },
            LogMessage {
                time: 1577836800001,
                level: 0,
                message: "[unknown]".into()
            },
        ]
    );
    // A complete batch is followed by an explicit empty batch, even with no LF.
    let line = "[2020/01/01 00:00:00.000 +00:00] [INFO] value\n";
    dir.file("pd.log", &line.repeat(1024));
    let batches = Mutex::new(Vec::new());
    search(
        &path,
        SearchLogRequest::default(),
        &CancellationToken::new(),
        &|b| {
            batches.lock().unwrap().push(b.unwrap().messages.len());
            true
        },
    )
    .unwrap();
    assert_eq!(*batches.lock().unwrap(), vec![1024, 0]);
}

#[test]
fn sa_014_resolution_and_error_cleanup() {
    let dir = Directory::new();
    let valid = "[2020/01/01 00:00:00.000 +00:00] [INFO] value\n";
    let path = dir.file("pd.log", valid);
    dir.file(
        "pd-first-invalid.log",
        &format!("{}{valid}", "bad\n".repeat(10)),
    );
    dir.file(
        "pd-tail-invalid.log",
        &format!(
            "{valid}{}",
            "invalid tail.....................................................\n".repeat(20)
        ),
    );
    dir.file("pd-wrong.log.gz", valid);
    dir.file("other.log", valid);
    let cancel = CancellationToken::new();
    assert_eq!(resolve(&path, 0, i64::MAX, &cancel).unwrap().len(), 1);
    let error = search(
        &path,
        SearchLogRequest {
            patterns: vec!["[".into()],
            ..Default::default()
        },
        &cancel,
        &|_| panic!("regex error must precede output"),
    )
    .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert_eq!(
        error.message(),
        "error parsing regexp: missing closing ]: `[`"
    );
    // Verify actual OS descriptors, including the early-return regex path.
    let out = std::process::Command::new("lsof")
        .args([
            "-a",
            "-p",
            &std::process::id().to_string(),
            "-t",
            "--",
            &path,
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "file leaked: {out:?}");
    cancel.cancel();
    assert_eq!(
        search(&path, SearchLogRequest::default(), &cancel, &|_| panic!(
            "cancelled search must not send"
        ))
        .unwrap_err()
        .code(),
        tonic::Code::Cancelled
    );
    let err = search(
        "",
        SearchLogRequest {
            patterns: vec!["[".into()],
            ..Default::default()
        },
        &CancellationToken::new(),
        &|_| true,
    )
    .unwrap_err();
    assert_eq!(err.message(), "empty log file location configuration");
}
