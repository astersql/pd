// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::diagnosticspb::{SearchLogRequest, diagnostics_client::DiagnosticsClient};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
const TIME: i64 = 1577836800000;
fn request() -> SearchLogRequest {
    SearchLogRequest {
        end_time: TIME + 3000,
        patterns: vec!["sa014".into()],
        ..Default::default()
    }
}
fn status(e: tonic::Status) -> Value {
    json!({"code":e.code() as i32,"message":e.message()})
}
async fn observe(
    client: &mut DiagnosticsClient<tonic::transport::Channel>,
    request: SearchLogRequest,
) -> Value {
    let mut rpc = tonic::Request::new(request);
    rpc.set_timeout(Duration::from_secs(10));
    let mut batches = Vec::new();
    let terminal = match client.search_log(rpc).await {
        Err(e) => status(e),
        Ok(response) => {
            let mut stream = response.into_inner();
            loop {
                match stream.message().await {
                    Ok(Some(batch)) => batches.push(json!(
                        batch
                            .messages
                            .iter()
                            .map(|m| json!({"time":m.time,"level":m.level,"message":m.message}))
                            .collect::<Vec<_>>()
                    )),
                    Ok(None) => break json!({"eof":true}),
                    Err(e) => break status(e),
                }
            }
        }
    };
    json!({"batches":batches,"terminal":terminal})
}
async fn case(
    client: &mut DiagnosticsClient<tonic::transport::Channel>,
    records: &mut Vec<Value>,
    name: &str,
    r: SearchLogRequest,
    count: usize,
) {
    let response = observe(client, r.clone()).await;
    println!(
        "sa014 {name}: batches={:?} terminal={}",
        response["batches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_array().unwrap().len())
            .collect::<Vec<_>>(),
        response["terminal"]
    );
    if count != usize::MAX {
        assert_eq!(
            response["terminal"],
            json!({"eof":true}),
            "{name}: {response}"
        );
        assert_eq!(
            response["batches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_array().unwrap().len())
                .sum::<usize>(),
            count,
            "{name}"
        );
    } else {
        assert_eq!(response["terminal"]["code"], 2, "{name}: {response}");
    }
    records.push(json!({"case":name,"request":{"start_time":r.start_time,"end_time":r.end_time,"levels":r.levels,"patterns":r.patterns,"target":r.target},"response":response}));
}
fn write(path: &Path, bytes: &str, fixtures: &mut Vec<Value>) {
    std::fs::write(path, bytes).unwrap();
    fixtures.push(json!({"name":path.file_name().unwrap().to_str().unwrap(),"bytes":bytes,"sha256":format!("{:x}",Sha256::digest(bytes.as_bytes()))}));
}
fn open_by_process(path: &Path) -> bool {
    let out = std::process::Command::new("lsof")
        .args(["-t", "--"])
        .arg(path)
        .output()
        .unwrap();
    assert!(matches!(out.status.code(), Some(0 | 1)), "lsof: {out:?}");
    !out.stdout.is_empty()
}
fn closed(path: &Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while open_by_process(path) {
        assert!(
            std::time::Instant::now() < deadline,
            "search leaked {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_014_log_stream() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut outputs = Vec::new();
    let mut receipts = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut s = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        s.ready().unwrap();
        let mut records = Vec::new();
        let mut client = DiagnosticsClient::connect(s.url.clone()).await.unwrap();
        case(
            &mut client,
            &mut records,
            "empty configuration",
            request(),
            usize::MAX,
        )
        .await;
        let log = s.dir.join("diagnostic.log");
        let arg = format!("--log-file={}", log.display());
        let endpoint = format!("--etcd-endpoints={}", s.metadata_url());
        let args = if is_rust {
            vec![endpoint.as_str(), arg.as_str()]
        } else {
            vec![arg.as_str()]
        };
        s.restart_with_args(binary, &args).unwrap();
        s.ready().unwrap();
        client = DiagnosticsClient::connect(s.url.clone()).await.unwrap();
        // Keep the host logger's active file intact. All fixed input lives in
        // rotated files, so real lifecycle logs cannot overwrite fixture bytes.
        let log = s.dir.join("diagnostic-current.log");
        let mut fixtures = Vec::new();
        // Timestamp order deliberately opposes filename order, with a final line
        // lacking a newline and CRLF in the middle rotated file.
        let rotated = s.dir.join("diagnostic-z.log");
        write(
            &rotated,
            concat!(
                "invalid header\n",
                "[2020/01/01 00:00:00.000 +00:00] [INFO] [sa014 first] [dup=1] [dup=2]\n",
                "sa014 continuation\n",
                "[2020/01/01 00:00:00.001 +00:00] [ERROR] [sa014 error]\n",
                "[2020/01/01 00:00:00.002 +00:00] [MiXeD] [sa014 unknown]\n",
                "[2020/01/01 00:00:00.003 +00:00] [debug] [sa014 debug]\n",
                "[2020/01/01 00:00:00.004 +00:00] [WARN] [sa014 warn]\n",
                "[2020/01/01 00:00:00.005 +00:00] [TRACE] [sa014 trace]\n",
                "[2020/01/01 00:00:00.006 +00:00] [CRITICAL] [sa014 critical]\n",
                "[2020/01/01 00:00:00.007 +00:00] [ERROR] [sa014 last]"
            ),
            &mut fixtures,
        );
        let rotated2 = s.dir.join("diagnostic-a.log");
        write(
            &rotated2,
            "[2020/01/01 00:00:01.000 +00:00] [INFO] [sa014 digits=123] [word=abc]\r\n[2020/01/01 00:00:01.001 +00:00] [INFO] [sa014 digits=１２３] [word=中文]\r\nsa014 tail\r\n",
            &mut fixtures,
        );
        write(
            &log,
            "[2020/01/01 00:00:02.000 +00:00] [INFO] [sa014 current]\n",
            &mut fixtures,
        );
        let ignored = "[2020/01/01 00:00:01.500 +00:00] [INFO] [sa014 excluded]\n";
        write(&s.dir.join("diagnostic.log.gz"), ignored, &mut fixtures);
        write(&s.dir.join("other.log"), ignored, &mut fixtures);
        write(
            &s.dir.join("diagnostic-invalid.log"),
            &format!("{}{ignored}", "bad\n".repeat(10)),
            &mut fixtures,
        );
        write(
            &s.dir.join("diagnostic-tail.log"),
            &format!(
                "{ignored}{}",
                "bad tail ..........................................................\n".repeat(15)
            ),
            &mut fixtures,
        );
        std::fs::create_dir(s.dir.join("diagnostic-directory.log")).unwrap();
        case(
            &mut client,
            &mut records,
            "rotation multiline duplicate fields",
            request(),
            13,
        )
        .await;
        closed(&rotated);
        closed(&rotated2);
        let mut r = request();
        r.start_time = TIME + 1;
        r.end_time = TIME + 4;
        case(&mut client, &mut records, "inclusive time", r, 4).await;
        let mut r = request();
        r.start_time = TIME + 3001;
        case(&mut client, &mut records, "no matching file", r, 0).await;
        let mut r = request();
        r.start_time = TIME + 5;
        r.end_time = TIME + 3;
        case(&mut client, &mut records, "reversed interval", r, 0).await;
        for (levels, count) in [
            (vec![2], 7),
            (vec![1], 2),
            (vec![3], 2),
            (vec![4], 2),
            (vec![5], 2),
            (vec![6], 3),
            (vec![0], 1),
            (vec![64], 13),
            (vec![2, 2, 6], 9),
        ] {
            let mut r = request();
            r.levels = levels.clone();
            case(
                &mut client,
                &mut records,
                &format!("levels {levels:?}"),
                r,
                count,
            )
            .await;
        }
        for (patterns, count) in [
            (vec!["sa014", "error"], 1),
            (vec!["sa014", "no-match"], 0),
            (vec![r"digits=\d+"], 1),
            (vec![r"word=\w+"], 1),
            (vec![r"\Q[dup=1]\E"], 1),
            (vec!["(?i)SA014", "current"], 1),
            (vec!["[a-z]+", "first|last"], 2),
        ] {
            let mut r = request();
            r.patterns = patterns.iter().map(|p| (*p).into()).collect();
            case(
                &mut client,
                &mut records,
                &format!("patterns {patterns:?}"),
                r,
                count,
            )
            .await;
        }
        let mut r = request();
        r.target = 1;
        case(
            &mut client,
            &mut records,
            "slow target ignored by sysutil",
            r,
            13,
        )
        .await;
        let mut r = request();
        r.end_time = 0;
        case(&mut client, &mut records, "zero end is unbounded", r, 13).await;
        // Diagnostics reads the receiving host's file even when it is a follower.
        let follower_path = s.dir.join("follower14.log");
        let mut args = if is_rust {
            vec![format!("--etcd-endpoints={}", s.metadata_url())]
        } else {
            vec![format!("--join={}", s.url), "--initial-cluster=".into()]
        };
        args.push("--name=follower14".into());
        args.push(format!("--log-file={}", follower_path.display()));
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let mut follower = Server::start_with_args(binary, &refs).unwrap();
        follower.ready().unwrap();
        let (membership_status, follower_membership) = follower.get("/pd/api/v1/members").unwrap();
        assert_eq!(membership_status, 200);
        assert_eq!(follower_membership["leader"]["name"], "compat");
        assert_ne!(
            follower_membership["leader"]["client_urls"],
            json!([follower.url])
        );
        write(
            &follower_path,
            "[2020/01/01 00:00:00.000 +00:00] [INFO] [sa014 follower local]\n",
            &mut fixtures,
        );
        let mut follower_client = DiagnosticsClient::connect(follower.url.clone())
            .await
            .unwrap();
        case(
            &mut follower_client,
            &mut records,
            "follower searches local file",
            SearchLogRequest {
                patterns: vec!["sa014 follower local".into()],
                ..request()
            },
            1,
        )
        .await;
        follower.stop().unwrap();
        // Repeat exact batch boundaries with identical fixed input on both sides.
        std::fs::remove_file(&rotated).unwrap();
        std::fs::remove_file(&rotated2).unwrap();
        let timestamps = concat!(
            "[2020/01/01 00:00:00.000 +00:00] [INFO] [sa014 valid]\n",
            "[2020/01/01 00:00:00.1 +00:00] [INFO] [sa014 short fraction]\n",
            "[2020/01/01 00:00:00.1234 +00:00] [INFO] [sa014 long fraction]\n",
            "[2020/1/01 00:00:00.000 +00:00] [INFO] [sa014 short month]\n",
            "[2020/01/01 00:00:00.001 +00:00] [INFO] [sa014 valid end]\n"
        );
        write(&log, timestamps, &mut fixtures);
        case(
            &mut client,
            &mut records,
            "timestamp syntax inheritance",
            request(),
            5,
        )
        .await;
        for n in [0, 1, 1023, 1024, 1025, 2048] {
            let bytes = "[2020/01/01 00:00:02.000 +00:00] [INFO] [sa014 batch]\n".repeat(n);
            write(&log, &bytes, &mut fixtures);
            case(
                &mut client,
                &mut records,
                &format!("batch boundary {n}"),
                request(),
                n,
            )
            .await;
            assert_eq!(
                records.last().unwrap()["response"]["batches"]
                    .as_array()
                    .unwrap()
                    .len(),
                n / 1024 + 1
            );
        }
        // A stream larger than transport flow control remains open until reset.
        // Inspect actual descriptors after the client drops it, before regex errors.
        let cancel_file = s.dir.join("diagnostic-cancel.log");
        let line = format!(
            "[2020/01/01 00:00:02.100 +00:00] [INFO] [sa014 cancel] {}\n",
            "x".repeat(1024)
        );
        let bytes = line.repeat(20000);
        write(&cancel_file, &bytes, &mut fixtures);
        let mut stream = client.search_log(request()).await.unwrap().into_inner();
        assert_eq!(
            stream.message().await.unwrap().unwrap().messages.len(),
            1024
        );
        drop(stream);
        closed(&cancel_file);
        case(
            &mut client,
            &mut records,
            "request after client cancellation",
            SearchLogRequest {
                patterns: vec!["batch".into()],
                ..request()
            },
            2048,
        )
        .await;
        std::fs::remove_file(&cancel_file).unwrap();
        for pattern in [
            "[", "(", "\\", r"\u1234", "a{1001}", "*", "(?=a)", r"(a)\1", "[z-a]",
        ] {
            let mut r = request();
            r.patterns = vec![pattern.into()];
            case(
                &mut client,
                &mut records,
                &format!("invalid regex {pattern}"),
                r,
                usize::MAX,
            )
            .await;
        }
        // Host cancellation must terminate an outstanding stream and release files.
        write(&cancel_file, &bytes, &mut fixtures);
        let mut pending = client.search_log(request()).await.unwrap().into_inner();
        assert_eq!(
            pending.message().await.unwrap().unwrap().messages.len(),
            1024
        );
        s.stop().unwrap();
        let terminal = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match pending.message().await {
                    Ok(Some(_)) => continue,
                    Ok(None) => break json!({"eof":true}),
                    Err(e) => break status(e),
                }
            }
        })
        .await
        .unwrap();
        assert!(
            terminal["code"].is_number(),
            "host shutdown must interrupt rather than report EOF: {terminal}"
        );
        closed(&cancel_file);
        receipts.push(json!({"implementation":if is_rust {"rust"} else {"go"},"fixtures":fixtures,"rpc":records,"follower_membership":follower_membership,"client_cancel_files_closed":true,"shutdown_terminal":terminal,"shutdown_files_closed":true,"logs":s.logs()}));
        std::fs::write(
            std::env::var_os("PD_SA014_RECEIPT").unwrap(),
            serde_json::to_vec(&receipts).unwrap(),
        )
        .unwrap();
        outputs.push(records);
    }
    assert_eq!(outputs[0].len(), outputs[1].len());
    for (go, rust) in outputs[0].iter().zip(&outputs[1]) {
        assert_eq!(go, rust, "raw RPC case {}", go["case"]);
    }
}
