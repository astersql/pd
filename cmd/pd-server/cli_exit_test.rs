// Copyright 2026 AsterSQL.
use std::process::{Command, Output};
fn run(args: &[&str]) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "cli_exit_test::task_091_process_entry",
            "--nocapture",
            "--quiet",
        ])
        .env("PD_TASK091_CHILD_ARGS", args.join("\u{1f}"))
        .output()
        .map(|mut output| {
            let prefix = b"\nrunning 1 test\n";
            if output.stdout.starts_with(prefix) {
                output.stdout.drain(..prefix.len());
            }
            output
        })
        .unwrap()
}
#[test]
fn task_091_behavior() {
    for args in [
        vec!["--version"],
        vec!["-V"],
        vec!["--version=true", "--config-check"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("Release Version:"));
    }
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["help"],
        vec!["--help=true", "--config=/missing"],
    ] {
        let output = run(&args);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8_lossy(&output.stdout);
        for required in [
            "--version",
            "--config-check",
            "--etcd-endpoints",
            "external etcd",
        ] {
            assert!(text.contains(required), "{text}");
        }
    }
}
#[test]
fn task_091_error_channels() {
    for args in [
        vec!["--bad"],
        vec!["--name"],
        vec!["--config-check=no"],
        vec!["--help", "--bad"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        assert!(!output.stdout.is_empty());
    }
    for args in [
        vec!["--config=/does-not-exist"],
        vec!["--config-check", "--join=x", "--initial-cluster=y"],
        vec!["--version", "--config=/does-not-exist"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Release Version:"));
    }
    for args in [
        vec!["--version=false", "--config-check"],
        vec!["--version", "--version=false", "--config-check"],
        vec!["-h=false", "--config-check"],
    ] {
        let output = run(&args);
        assert!(output.status.success());
        assert_eq!(output.stdout, b"config check successful\n");
        assert!(output.stderr.is_empty());
    }
    let output = run(&["--config-check"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"config check successful\n");
    assert!(output.stderr.is_empty());
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--etcd-endpoints"));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let output = run(&[
        &format!("--client-urls=http://{address}"),
        "--etcd-endpoints=http://127.0.0.1:1",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Address already in use"));
}

#[test]
fn task_091_process_entry() {
    if std::env::var_os("PD_TASK091_CHILD_ARGS").is_none() {
        return;
    }
    let code = crate::main();
    std::process::exit(if code == std::process::ExitCode::SUCCESS {
        0
    } else if code == std::process::ExitCode::from(2) {
        2
    } else {
        1
    });
}
