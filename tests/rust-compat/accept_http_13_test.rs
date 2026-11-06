// Copyright 2026 AsterSQL.
use crate::harness::Server;
fn source_routes() -> Vec<(&'static str, String)> {
    let routes = include_str!("../../server/apiv2/handlers/microservice.go")
        .lines()
        .filter_map(|line| {
            let (method, rest) = line.trim().strip_prefix("router.")?.split_once('(')?;
            let path = rest.strip_prefix('"')?.split('"').next()?;
            Some((method, format!("ms/{}", path.replace(":service", "tso"))))
        })
        .collect::<Vec<_>>();
    assert!(
        !routes.is_empty(),
        "Go route extraction returned no methods"
    );
    routes
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_013_behavior() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut g = Server::start(&go).unwrap();
    let mut r = Server::start_rust(&rust).unwrap();
    g.ready().unwrap();
    r.ready().unwrap();
    for (method, path) in source_routes() {
        let a = observe(&g.url, method, &path, "");
        let b = observe(&r.url, method, &path, "");
        println!("sa013 classic {method} {path}: go={a} rust={b}");
        for field in ["status", "content_type", "body", "location"] {
            assert_eq!(a[field], b[field], "{path} {field}");
        }
    }
    g.stop().unwrap();
    r.stop().unwrap();
}

use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
    tsopb,
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Mutation, Read};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    time::Duration,
};
fn observe(url: &str, method: &str, path: &str, body: &str) -> Value {
    use std::io::Write;
    let request_url = format!("{url}/pd/api/v2/{path}");
    let out = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "40",
            "-X",
            method,
            "--data-binary",
            body,
            "-H",
            "Content-Type: application/json",
            "-H",
            "Expect:",
            "-D",
            "-",
            "-w",
            "\n%{http_code}",
            &request_url,
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let raw = String::from_utf8(out.stdout).unwrap();
    let (headers, contents) = raw.split_once("\r\n\r\n").unwrap();
    let (contents, status) = contents.rsplit_once('\n').unwrap();
    let header = |name: &str| {
        headers.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
        })
    };
    let v = json!({"status":status.parse::<u16>().unwrap(),"content_type":header("content-type"),"location":header("location"),"body":serde_json::from_str::<Value>(contents).unwrap_or(json!(contents))});
    let dir = std::path::Path::new("/tmp/pd-sa013-implementation");
    std::fs::create_dir_all(dir).unwrap();
    writeln!(std::fs::OpenOptions::new().create(true).append(true).open(dir.join("http.jsonl")).unwrap(),"{}",json!({"url":request_url,"method":method,"request_body":body,"raw_response":raw,"observation":v})).unwrap();
    println!("sa013 HTTP {method} {path}: {v}");
    v
}

fn address() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", l.local_addr().unwrap())
}
struct TsoProcess {
    child: Child,
    log: PathBuf,
}
impl TsoProcess {
    fn start(binary: &Path, backend: &str, address: &str, name: &str) -> Self {
        let log = std::env::temp_dir().join(format!(
            "sa013-tso-{}-{name}-{}.log",
            std::process::id(),
            address.rsplit(':').next().unwrap()
        ));
        let file = std::fs::File::create(&log).unwrap();
        let child = Command::new(binary)
            .args([
                "services",
                "tso",
                "--backend-endpoints",
                backend,
                "--listen-addr",
                address,
                "--name",
                name,
            ])
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .unwrap();
        Self { child, log }
    }
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status()
                .unwrap();
            let end = std::time::Instant::now() + Duration::from_secs(10);
            while self.child.try_wait().unwrap().is_none() {
                if std::time::Instant::now() > end {
                    self.child.kill().unwrap();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let status = self.child.wait().unwrap();
        assert!(
            status.success(),
            "TSO exited {status}: {}",
            std::fs::read_to_string(&self.log).unwrap()
        );
    }
}
impl Drop for TsoProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status();
            let _ = self.child.wait();
        }
    }
}
async fn timestamp(address: &str, id: u64) -> Result<(i64, i64), String> {
    let work = async {
        let mut c = tsopb::tso_client::TsoClient::connect(address.to_owned())
            .await
            .map_err(|e| e.to_string())?;
        let request = tsopb::TsoRequest {
            header: Some(tsopb::RequestHeader {
                cluster_id: id,
                keyspace_group_id: 0,
                keyspace: Some(tsopb::request_header::Keyspace::KeyspaceId(0)),
                ..Default::default()
            }),
            count: 1,
            ..Default::default()
        };
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tx.send(request).await.unwrap();
        let reply = c
            .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .map_err(|e| e.to_string())?
            .into_inner()
            .message()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("no timestamp")?;
        drop(tx);
        let t = reply.timestamp.ok_or("timestamp missing")?;
        Ok((t.physical, t.logical))
    };
    tokio::time::timeout(Duration::from_secs(3), work)
        .await
        .map_err(|_| "TSO timeout".to_owned())?
}
async fn active_stream(
    address: &str,
    id: u64,
) -> (
    tokio::sync::mpsc::Sender<tsopb::TsoRequest>,
    tonic::Streaming<tsopb::TsoResponse>,
    tsopb::TsoRequest,
    (i64, i64),
) {
    let mut client = tsopb::tso_client::TsoClient::connect(address.to_owned())
        .await
        .unwrap();
    let request = tsopb::TsoRequest {
        header: Some(tsopb::RequestHeader {
            cluster_id: id,
            keyspace_group_id: 0,
            keyspace: Some(tsopb::request_header::Keyspace::KeyspaceId(0)),
            ..Default::default()
        }),
        count: 1,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tx.send(request.clone()).await.unwrap();
    let mut stream = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let t = stream.message().await.unwrap().unwrap().timestamp.unwrap();
    (tx, stream, request, (t.physical, t.logical))
}
async fn primary(s: &Server, expected: &[String]) -> String {
    for _ in 0..150 {
        let r = observe(&s.url, "GET", "ms/primary/tso", "");
        if let Some(a) = r["body"].as_str() {
            if expected.iter().any(|e| e == a) {
                return a.into();
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("primary never converged to {expected:?}")
}
async fn rows(store: &dyn MetaStore, prefix: &str) -> Value {
    json!(store.read(Read::prefix(prefix.as_bytes(),Consistency::Linearizable)).await.unwrap().entries.into_iter().map(|e|json!({"key":String::from_utf8(e.key).unwrap(),"bytes":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision,"lease":e.lease})).collect::<Vec<_>>())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn sa_013_dual_discovery_eviction_and_wal() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut compared = vec![];
    let mut reports = vec![];
    for is_rust in [false, true] {
        let binary = if is_rust { &rust } else { &go };
        let env = [("PD_SERVICE_MODE".into(), "api".into())];
        let mut s = if is_rust {
            Server::start_rust_with_environment(binary, &[], &env)
        } else {
            Server::start_with_environment(binary, &[], &env)
        }
        .unwrap();
        s.ready().unwrap();
        let mut pd = PdClient::connect(s.url.clone()).await.unwrap();
        let id = pd
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let store: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[s.metadata_url().to_owned()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let paths = ClusterPaths::new(id);
        let mut observations = vec![];
        let mut raw = vec![];
        for (method, path, body) in [
            ("GET", "ms/members/router", ""),
            ("GET", "ms/members/%72outer", ""),
            ("GET", "ms/members/resource_manager", ""),
            ("GET", "ms/members/unknown", ""),
            ("GET", "ms/primary/router", ""),
            ("POST", "ms/tso/primary/evict", ""),
            ("POST", "ms/tso/primary/evict?node=invalid", ""),
            ("GET", "ms/members/router/", ""),
            ("POST", "ms/tso/primary/evict/?node=invalid", ""),
        ] {
            let v = observe(&s.url, method, path, body);
            observations.push(v.clone());
            raw.push(json!({"stage":"empty","method":method,"path":path,"response":v}));
        }
        pd.bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap();
        for (index,value) in [r#"{"name":"one","service-addr":"http://one","version":"v1","git-hash":"hash","deploy-path":"/bin","start-timestamp":17}"#,"broken",r#"{"NAME":"case","version":null,"unknown":7}"#,"null",r#"{"name":"before","NAME":"after","name":null}"#,r#"{"start-timestamp":1.5}"#].iter().enumerate(){
            store.transact(vec![],vec![Mutation::put(paths.registry_path("router",&index.to_string()).as_bytes(),value.as_bytes())]).await.unwrap();
        }
        let prefix = paths.service_path("router");
        let before = rows(store.as_ref(), &prefix).await;
        let members = observe(&s.url, "GET", "ms/members/router", "");
        observations.push(members.clone());
        raw.push(json!({"stage":"decoded","response":members,"state":before}));
        let lease = store.grant(2).await.unwrap();
        let key = paths.registry_path("scheduling", "lease");
        store
            .transact(
                vec![],
                vec![Mutation::Put {
                    key: key.clone().into_bytes(),
                    value: br#"{"name":"leased","service-addr":"host"}"#.to_vec(),
                    lease: Some(lease.id),
                }],
            )
            .await
            .unwrap();
        observations.push(observe(&s.url, "GET", "ms/members/scheduling", ""));
        for _ in 0..100 {
            if store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .is_empty()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
                .is_empty()
        );
        observations.push(observe(&s.url, "GET", "ms/members/scheduling", ""));
        // The same durable registry bytes/revisions must survive a real backend WAL restart.
        assert_eq!(before, rows(store.as_ref(), &prefix).await);
        if is_rust {
            s.restart_rust(binary).unwrap();
        } else {
            s.restart_with_args(binary, &[]).unwrap();
        }
        s.ready().unwrap();
        assert_eq!(before, rows(store.as_ref(), &prefix).await);
        observations.push(observe(&s.url, "GET", "ms/members/router", ""));
        let a = address();
        let b = address();
        let mut one = TsoProcess::start(binary, &s.metadata_url(), &a, "one");
        // Rust's transition completion uses PD HTTP; no transitions occur in this case.
        let mut two = TsoProcess::start(binary, &s.metadata_url(), &b, "two");
        let nodes = vec![a.clone(), b.clone()];
        for _ in 0..150 {
            if observe(&s.url, "GET", "ms/members/tso", "")["body"]
                .as_array()
                .is_some_and(|m| m.len() == 2)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let live = observe(&s.url, "GET", "ms/members/tso", "");
        assert_eq!(live["body"].as_array().unwrap().len(), 2, "{live}");
        raw.push(json!({"stage":"live","response":live}));
        for member in live["body"].as_array().unwrap() {
            for field in ["name", "service-addr", "version", "git-hash", "deploy-path"] {
                assert!(member[field].is_string(), "{member}");
            }
            assert!(member["start-timestamp"].as_i64().unwrap() > 0);
        }
        let mut live_contract = live.clone();
        for member in live_contract["body"].as_array_mut().unwrap() {
            let addr = member["service-addr"].as_str().unwrap();
            member["service-addr"] = json!(if addr == a {
                "NODE_ONE"
            } else {
                assert_eq!(addr, b);
                "NODE_TWO"
            });
            member["version"] = json!("BINARY_VERSION");
            member["git-hash"] = json!("BINARY_GIT_HASH");
            member["deploy-path"] = json!("BINARY_PARENT");
            member["start-timestamp"] = json!("UNIX_START_TIME");
        }
        live_contract["body"]
            .as_array_mut()
            .unwrap()
            .sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        observations.push(live_contract);
        let registry_state = rows(store.as_ref(), &paths.service_path("tso")).await;
        assert!(
            registry_state
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["lease"].as_i64().is_some_and(|id| id > 0))
        );
        let mut assigned = false;
        for _ in 0..150 {
            let group = observe(&s.url, "GET", "tso/keyspace-groups/0", "");
            if group["body"]["members"]
                .as_array()
                .is_some_and(|m| m.len() == 2)
            {
                assigned = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            assigned,
            "default group never acquired both live registrations"
        );
        let mut old = primary(&s, &nodes).await;
        let mut stamp = None;
        for _ in 0..150 {
            if let Ok(t) = timestamp(&old, id).await {
                stamp = Some(t);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            old = primary(&s, &nodes).await;
        }
        let target = if old == a { &b } else { &a };
        let stamp = stamp.expect("serving primary timestamp");
        let election_key = paths.election_path(Some(&MsParam {
            service_name: "tso".into(),
            group_id: 0,
        }));
        let leader_before = rows(store.as_ref(), &election_key).await;
        let (old_tx, mut old_stream, old_request, stream_stamp) = active_stream(&old, id).await;
        let stamp = stamp.max(stream_stamp);
        let marker_before = rows(
            store.as_ref(),
            &paths.expected_primary_path(&MsParam {
                service_name: "tso".into(),
                group_id: 0,
            }),
        )
        .await;
        for body in [
            "bad",
            "{",
            "[]",
            "true",
            "1",
            r#"{"new_primary":1}"#,
            r#"{"new_primary":"missing","new_primary":null}"#,
            r#"{"new_primary":"missing","NEW_PRIMARY":"missing"}"#,
        ] {
            let v = observe(
                &s.url,
                "POST",
                &format!("ms/tso/primary/evict?node={old}"),
                body,
            );
            assert_eq!(v["status"], 400, "{v}");
            observations.push(v.clone());
            raw.push(json!({"stage":"invalid-body","input":body,"response":v}));
        }
        let huge = observe(
            &s.url,
            "POST",
            &format!("ms/tso/primary/evict?node={old}"),
            &" ".repeat(4097),
        );
        assert_eq!(huge["status"], 413);
        observations.push(huge);
        for target in [
            &old,
            &format!("https://{}", old.strip_prefix("http://").unwrap()),
        ] {
            let v = observe(
                &s.url,
                "POST",
                &format!("ms/tso/primary/evict?node={old}"),
                &json!({"new_primary":target}).to_string(),
            );
            assert_eq!(v["status"], 400);
            observations.push(v);
        }
        // A disconnected body must not reach the domain transfer operation.
        {
            use std::io::Write;
            let mut sock =
                std::net::TcpStream::connect(s.url.trim_start_matches("http://")).unwrap();
            write!(sock,"POST /pd/api/v2/ms/tso/primary/evict?node={old} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\nContent-Type: application/json\r\n\r\n{{").unwrap();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            marker_before,
            rows(
                store.as_ref(),
                &paths.expected_primary_path(&MsParam {
                    service_name: "tso".into(),
                    group_id: 0
                })
            )
            .await
        );
        let no_proxy = observe(
            &s.url,
            "POST",
            &format!("ms/tso/primary/evict?node={old}/unregistered"),
            "",
        );
        assert_eq!(no_proxy["status"], 400);
        let dead = address();
        let dead_key = paths.registry_path("tso", &dead);
        store
            .transact(
                vec![],
                vec![Mutation::put(
                    dead_key.as_bytes(),
                    &serde_json::to_vec(&json!({"name":"unreachable","service-addr":dead}))
                        .unwrap(),
                )],
            )
            .await
            .unwrap();
        let failed_proxy = observe(
            &s.url,
            "POST",
            &format!("ms/tso/primary/evict?node={dead}"),
            "",
        );
        assert_eq!(failed_proxy["status"], 500);
        observations.push(failed_proxy);
        store
            .transact(
                vec![],
                vec![Mutation::Delete {
                    key: dead_key.into_bytes(),
                }],
            )
            .await
            .unwrap();
        let invalid = observe(
            &s.url,
            "POST",
            &format!("ms/tso/primary/evict?node={old}"),
            r#"{"new_primary":"missing"}"#,
        );
        assert_eq!(invalid["status"], 400);
        observations.push(invalid.clone());
        assert_eq!(primary(&s, &nodes).await, old);
        let evicted = observe(
            &s.url,
            "POST",
            &format!(
                "ms/tso/primary/evict?node=https://{}",
                old.strip_prefix("http://").unwrap()
            ),
            &json!({"new_primary":target}).to_string(),
        );
        assert_eq!(evicted["status"], 200, "{evicted}");
        assert_eq!(evicted["body"]["0"], "success");
        observations.push(evicted.clone());
        assert_eq!(primary(&s, &[target.clone()]).await, *target);
        let mut next = None;
        for _ in 0..150 {
            if let Ok(t) = timestamp(target, id).await {
                next = Some(t);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let next = next.expect("elected replacement did not become serving");
        assert!(next > stamp);
        // An already open old-term allocation path is fenced by the revoked lease.
        assert!(
            timestamp(&old, id).await.is_err(),
            "old term continued serving"
        );
        old_tx.send(old_request).await.unwrap();
        let old_response = tokio::time::timeout(Duration::from_secs(3), old_stream.message())
            .await
            .unwrap();
        assert!(
            old_response.is_err() || old_response.as_ref().is_ok_and(|r| r.is_none()),
            "existing old stream returned timestamp: {old_response:?}"
        );
        drop(old_tx);
        drop(old_stream);
        let marker = paths.expected_primary_path(&MsParam {
            service_name: "tso".into(),
            group_id: 0,
        });
        for _ in 0..100 {
            if rows(store.as_ref(), &marker)
                .await
                .as_array()
                .unwrap()
                .is_empty()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            rows(store.as_ref(), &marker)
                .await
                .as_array()
                .unwrap()
                .is_empty()
        );
        let repeated = observe(
            &s.url,
            "POST",
            &format!("ms/tso/primary/evict?node={old}"),
            "",
        );
        assert_eq!(repeated["status"], 200);
        assert_eq!(repeated["body"], json!({}));
        observations.push(repeated.clone());
        raw.push(json!({"stage":"evicted","response":evicted,"repeated":repeated,"before_timestamp":stamp,"after_timestamp":next,"old_rejected":true,"old_stream_result":format!("{old_response:?}"),"registry":rows(store.as_ref(),&paths.service_path("tso")).await}));
        let leader_after = rows(store.as_ref(), &election_key).await;
        assert_ne!(leader_before[0]["lease"], leader_after[0]["lease"]);
        assert_ne!(leader_before[0]["bytes"], leader_after[0]["bytes"]);
        raw.push(json!({"stage":"election-state","before":leader_before,"after":leader_after,"marker":rows(store.as_ref(),&marker).await,"timestamp_window":rows(store.as_ref(),&paths.timestamp_path(0)).await}));
        one.stop();
        two.stop();
        for _ in 0..100 {
            if observe(&s.url, "GET", "ms/members/tso", "")["body"].is_null() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(observe(&s.url, "GET", "ms/members/tso", "")["body"].is_null());
        s.stop().unwrap();
        compared.push(observations);
        reports.push(json!({"rust":is_rust,"raw":raw,"wal_registry":before,"tso_logs":[std::fs::read_to_string(&one.log).unwrap(),std::fs::read_to_string(&two.log).unwrap()]}));
    }
    assert_eq!(compared[0], compared[1], "paired HTTP observations");
    std::fs::write(
        "/tmp/pd-sa013-implementation/dual-matrix.json",
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
}
