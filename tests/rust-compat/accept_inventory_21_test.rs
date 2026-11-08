// Copyright 2026 AsterSQL.

use crate::{harness::Server, readonly_oracle};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn request(url: &str, path: &str) -> (u16, String) {
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into())
}

fn cli_receipts(binary: &Path, implementation: &str, config: &str, missing: &str, work: &Path) {
    for (case, args, exit, marker) in [
        (
            "config-check",
            vec!["--config-check", config],
            0,
            "config check successful",
        ),
        ("version", vec!["-V", config], 0, "Release Version:"),
        ("parse-before-version", vec!["-V", missing], 0, ""),
        ("help", vec!["-h"], 0, "--config-check"),
        ("unknown", vec!["--sa21-unknown"], 1, "--sa21-unknown"),
    ] {
        let output = Command::new(binary)
            .args(&args)
            .current_dir(work)
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        println!(
            "SA21 {}",
            json!({"kind":"command","implementation":implementation,"case":case,
            "argv":args,"exit":output.status.code(),"stdout":stdout,"stderr":stderr})
        );
        assert_eq!(output.status.code(), Some(exit), "{case}");
        assert!(!stdout.is_empty() || !stderr.is_empty(), "{case}");
        assert!(stdout.contains(marker) || stderr.contains(marker), "{case}");
        if case == "parse-before-version" {
            assert!(!stdout.contains("Release Version:"), "{case}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires pinned Go PD, the current Rust PD binary and pinned etcd"]
async fn surface_21_live() {
    let go = readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mode = std::env::var("PD_COMPAT_MODE").unwrap();
    assert_eq!(mode, "dual");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/health-member.json");
    readonly_oracle::validate(&mode, &go, Some(&rust), &fixture).unwrap();
    let work = super::accept_extra_transport_config_test::Work(
        std::env::temp_dir().join(format!("pd-sa21-{}", std::process::id())),
    );
    fs::create_dir(&work.0).unwrap();
    let config = work.0.join("pd.toml");
    let file_log = work.0.join("file-default.log");
    let toml = format!(
        "name = \"file-default\"\n[log]\nlevel = \"error\"\n[log.file]\nfilename = {:?}\n[replication]\nmax-replicas = 2\n",
        file_log.to_str().unwrap()
    );
    fs::write(&config, &toml).unwrap();
    let config_arg = format!("--config={}", config.display());
    let missing = format!("--config={}/missing.toml", work.0.display());
    for (binary, implementation) in [(&go, "go"), (&rust, "rust")] {
        cli_receipts(binary, implementation, &config_arg, &missing, &work.0);
        for (role, env_mode) in [("classic", ""), ("api", "ApI")] {
            let cli_log = work.0.join(format!("{implementation}-{role}.log"));
            let log_arg = format!("--log-file={}", cli_log.display());
            let args = [&*config_arg, "-L", "debug", &*log_arg];
            let env = [("PD_SERVICE_MODE".into(), env_mode.into())];
            let mut server = if implementation == "rust" {
                Server::start_rust_with_environment(binary, &args, &env)
            } else {
                Server::start_with_environment(binary, &args, &env)
            }
            .unwrap();
            server.ready().unwrap();
            let (config_status, config_body) = request(&server.url, "/pd/api/v1/config");
            let values: Value = serde_json::from_str(&config_body).unwrap();
            let (mode_status, mode_body) = request(&server.url, "/pd/api/v2/ms/members/tso");
            let mut client = PdClient::connect(server.url.clone()).await.unwrap();
            let member_request = pdpb::GetMembersRequest::default();
            let members = client
                .get_members(member_request.clone())
                .await
                .unwrap()
                .into_inner();
            let cluster = members.header.as_ref().unwrap().cluster_id;
            let alloc = pdpb::AllocIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: cluster,
                    ..Default::default()
                }),
                count: 1001,
            };
            let allocated = client.alloc_id(alloc.clone()).await.unwrap().into_inner();
            let backend =
                EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(2))
                    .await
                    .unwrap();
            let key = format!("/pd/{cluster}/alloc_id");
            let entries = backend
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap()
                .entries;
            let persisted = u64::from_be_bytes(entries[0].value.as_slice().try_into().unwrap());
            let (metric_status, metrics) = request(&server.url, "/metrics");
            let families: Vec<_> = metrics
                .lines()
                .filter(|line| {
                    ["pd_cluster_id", "pd_tso_role", "pd_tso_events"]
                        .iter()
                        .any(|family| {
                            line.starts_with(&format!("# HELP {family} "))
                                || line.starts_with(&format!("# TYPE {family} "))
                                || line.starts_with(&format!("{family}{{"))
                        })
                })
                .collect();
            drop(client);
            drop(backend);
            server.stop().unwrap();
            let log = fs::read_to_string(&cli_log).unwrap();
            println!(
                "SA21 {}",
                json!({"kind":"process","implementation":implementation,"mode":role,
                "env":{"PD_SERVICE_MODE":env_mode},"extra_argv":args,"fixture":toml,
                "http":[{"path":"/pd/api/v1/config","status":config_status,"body":config_body},
                        {"path":"/pd/api/v2/ms/members/tso","status":mode_status,"body":mode_body}],
                "rpc":[{"method":"GetMembers","request":member_request.encode_to_vec(),"response":members.encode_to_vec()},
                       {"method":"AllocID","request":alloc.encode_to_vec(),"response":allocated.encode_to_vec(),
                        "id":allocated.id,"error":allocated.header.as_ref().unwrap().error.is_some()}],
                "persisted":{"key":key,"bytes":entries[0].value,"mod_revision":entries[0].mod_revision},
                "metrics":{"path":"/metrics","status":metric_status,"lines":families},"log":log,
                "process_stdout_stderr":server.logs(),"file_default_log_exists":file_log.exists(),"shutdown_exit":0})
            );
            assert_eq!(config_status, 200);
            assert_eq!(mode_status, if role == "api" { 200 } else { 404 });
            assert_eq!(
                serde_json::from_str::<Value>(&mode_body).unwrap(),
                if role == "api" {
                    Value::Null
                } else {
                    json!("not support microservice")
                }
            );
            assert_eq!(values["name"], "compat");
            assert_eq!(values["replication"]["max-replicas"], 2);
            assert_eq!(values["log"]["level"], "debug");
            assert_eq!(values["log"]["file"]["filename"], cli_log.to_str().unwrap());
            assert!(!file_log.exists());
            assert!(log.contains("INFO") || log.contains("[info]"), "{log}");
            assert!(allocated.header.unwrap().error.is_none());
            assert!(allocated.id > 0 && persisted >= allocated.id);
            assert_eq!(metric_status, 200);
            assert!(families.contains(&"# HELP pd_cluster_id Record of id allocator."));
            assert!(families.contains(&"# TYPE pd_cluster_id gauge"));
            assert!(
                families
                    .contains(&format!("pd_cluster_id{{type=\"idalloc\"}} {persisted}").as_str())
            );
        }
    }
}

const SURFACE: &str = "docs/rust/server-api/surface-inventory.md";
const COVERAGE: &str = "docs/rust/server-api/acceptance/coverage.json";
const RECEIPT: &str = "docs/rust/server-api/acceptance/task-21.json.gz";
const NOTES: &str = "docs/rust/server-api/acceptance/task-21.md";
const TEST: &str = "tests/rust-compat/accept_inventory_21_test.rs";
const HISTORICAL: &str = "docs/rust/server-api/task-102-resume-evidence.json.gz";
const MANIFEST: &str = "## 来源集合 SHA-256";
const SCOPE: &[&str] = &[
    "cmd/pd-server/",
    "server/config/",
    ":(glob)pkg/**/metrics*.rs",
    ":(glob)pkg/mcs/*/server/*.rs",
    ":(glob)tests/rust-compat/accept*.rs",
    ":(glob)docs/rust/server-api/acceptance/task-0[3-9]*",
    ":(glob)docs/rust/server-api/acceptance/task-1[0-8]*",
];

pub(super) fn owns_new_audit_path(path: &str) -> bool {
    matches!(path, TEST | NOTES | RECEIPT)
}

pub(super) fn successor_hash<'a>(transition: &'a Value, path: &str, prior: &'a Value) -> &'a Value {
    if let Some(entry) = transition.get(path) {
        assert_eq!(entry["before"], *prior, "{path}: prior audit input drift");
        &entry["after"]
    } else {
        prior
    }
}

use super::accept_inventory_19_test::{canonical, digest, read_command, sha256};
use std::collections::{BTreeMap, BTreeSet};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_json(root: &Path, path: &str) -> Value {
    if path.ends_with(".gz") {
        serde_json::from_slice(&read_command(root, "gzip", &["-dc", path])).unwrap()
    } else {
        serde_json::from_slice(&fs::read(root.join(path)).unwrap()).unwrap()
    }
}

fn manifest(doc: &str) -> Result<BTreeMap<&str, (&str, &str)>, &'static str> {
    let (_, body) = doc
        .split_once(&format!("{MANIFEST}\n"))
        .ok_or("missing surface manifest heading")?;
    let (_, body) = body
        .split_once("| --- | --- | --- |\n")
        .ok_or("missing surface manifest header")?;
    let mut entries = BTreeMap::new();
    for line in body.lines().take_while(|line| !line.is_empty()) {
        let row = line
            .strip_prefix("| [")
            .and_then(|s| s.strip_suffix(" |"))
            .ok_or("invalid manifest row")?;
        let (path, tail) = row
            .split_once("](../../../")
            .ok_or("invalid manifest link")?;
        let (target, tail) = tail.split_once(") | ").ok_or("invalid manifest link")?;
        let (hash, owner) = tail.split_once(" | ").ok_or("invalid manifest cells")?;
        if path != target || !canonical(path) || !sha256(hash) || owner.is_empty() {
            return Err("invalid manifest input");
        }
        if entries.insert(path, (hash, owner)).is_some() {
            return Err("duplicate manifest input");
        }
    }
    if entries.is_empty() {
        return Err("missing manifest rows");
    }
    Ok(entries)
}

fn verify_manifest<'a>(
    doc: &'a str,
    frozen: &Value,
) -> Result<BTreeMap<&'a str, (&'a str, &'a str)>, &'static str> {
    let entries = manifest(doc)?;
    let mut expected = BTreeMap::new();
    for row in frozen.as_array().ok_or("missing frozen surface manifest")? {
        if row["acceptance_task"] != 21 {
            return Err("unexpected manifest owner");
        }
        let path = row["path"].as_str().unwrap();
        let hash = row["historical_sha256"].as_str().unwrap();
        let [owner] = row["historical_owner"].as_array().unwrap().as_slice() else {
            return Err("missing original manifest owner");
        };
        if expected
            .insert(path, (hash, owner.as_str().unwrap()))
            .is_some()
        {
            return Err("duplicate frozen manifest input");
        }
    }
    if entries != expected {
        return Err("historical manifest set drift");
    }
    Ok(entries)
}

fn cells(line: &str) -> Vec<&str> {
    let line = line.trim_matches('|');
    let mut start = 0;
    let mut cells = Vec::new();
    for (pos, byte) in line.bytes().enumerate() {
        if byte == b'|' && (pos == 0 || line.as_bytes()[pos - 1] != b'\\') {
            cells.push(line[start..pos].trim());
            start = pos + 1;
        }
    }
    cells.push(line[start..].trim());
    cells
}

fn separator(line: &str) -> bool {
    line.starts_with('|') && line.bytes().all(|b| b"| -:".contains(&b))
}

fn owned_rows(doc: &str) -> BTreeMap<String, (&str, &str, Vec<&str>)> {
    let mut section = "";
    let mut header = Vec::new();
    let mut rows = BTreeMap::new();
    let mut lines = doc.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("## ") {
            section = line;
        }
        if section == MANIFEST || !line.starts_with('|') {
            header.clear();
            continue;
        }
        if lines.peek().is_some_and(|next| separator(next)) {
            header = cells(line);
            lines.next();
            continue;
        }
        let owned: Vec<_> = header
            .iter()
            .enumerate()
            .filter(|(_, name)| {
                name.to_ascii_lowercase().contains("owner") || name.contains("责任")
            })
            .map(|(i, _)| i)
            .collect();
        if owned.is_empty() {
            continue;
        }
        let values = cells(line);
        assert_eq!(header.len(), values.len(), "{line}");
        let owners: Vec<_> = owned.iter().map(|&i| values[i]).collect();
        let id = digest(format!("{section}\n{line}").as_bytes());
        assert!(rows.insert(id, (section, line, owners)).is_none(), "{line}");
    }
    rows
}

fn sources(line: &str) -> BTreeSet<String> {
    line.split("](")
        .skip(1)
        .filter_map(|text| text.split_once(')'))
        .map(|(target, _)| target.split('#').next().unwrap())
        .filter(|target| !target.is_empty() && !target.contains("://"))
        .map(|target| {
            target
                .strip_prefix("../../../")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("docs/rust/server-api/{target}"))
        })
        .inspect(|path| assert!(canonical(path), "{path}"))
        .collect()
}

fn check_input(entry: &Value, actual: &str) -> Result<(), &'static str> {
    if entry["current_sha256"] != actual {
        return Err("surface source input drift");
    }
    Ok(())
}

fn check_business(run: &Value) -> Result<(), &'static str> {
    if run["kind"] != "dual-business"
        || run["exit"] != 0
        || !run["log"].as_str().is_some_and(|log| {
            log.contains("test accept_inventory_21_test::surface_21_live ... ok")
                && log.contains("test result: ok. 1 passed; 0 failed; 0 ignored;")
                && log.contains("read-only supervisor: process group cleaned")
        })
    {
        return Err("missing passing dual business evidence");
    }
    Ok(())
}

fn verify_observations(run: &Value) {
    check_business(run).unwrap();
    let observations = run["observations"].as_array().unwrap();
    let records: BTreeMap<_, _> = observations
        .iter()
        .map(|obs| {
            (
                (
                    obs["implementation"].as_str().unwrap(),
                    obs["case"]
                        .as_str()
                        .or_else(|| obs["mode"].as_str())
                        .unwrap(),
                ),
                obs,
            )
        })
        .collect();
    assert_eq!(records.len(), observations.len());
    for implementation in ["go", "rust"] {
        for (case, exit) in [
            ("config-check", 0),
            ("version", 0),
            ("parse-before-version", 0),
            ("help", 0),
            ("unknown", 1),
        ] {
            let cmd = records[&(implementation, case)];
            assert_eq!(cmd["kind"], "command");
            assert_eq!(cmd["exit"], exit);
            assert!(!cmd["stdout"].as_str().unwrap().is_empty());
            assert_eq!(cmd["stderr"], "");
        }
        for (mode, status, body) in [
            ("classic", 404, json!("not support microservice")),
            ("api", 200, Value::Null),
        ] {
            let obs = records[&(implementation, mode)];
            assert_eq!(obs["kind"], "process");
            assert_eq!(
                obs["env"]["PD_SERVICE_MODE"],
                if mode == "api" { "ApI" } else { "" }
            );
            assert_eq!(obs["shutdown_exit"], 0);
            assert_eq!(obs["file_default_log_exists"], false);
            let [config, mode] = obs["http"].as_array().unwrap().as_slice() else {
                panic!("missing HTTP pair");
            };
            assert_eq!(config["path"], "/pd/api/v1/config");
            assert_eq!(config["status"], 200);
            let config: Value = serde_json::from_str(config["body"].as_str().unwrap()).unwrap();
            assert_eq!(config["name"], "compat");
            assert_eq!(config["replication"]["max-replicas"], 2);
            assert_eq!(config["log"]["level"], "debug");
            let path = obs["extra_argv"][3]
                .as_str()
                .unwrap()
                .strip_prefix("--log-file=")
                .unwrap();
            assert_eq!(config["log"]["file"]["filename"], path);
            assert_eq!(mode["path"], "/pd/api/v2/ms/members/tso");
            assert_eq!(mode["status"], status);
            assert_eq!(
                serde_json::from_str::<Value>(mode["body"].as_str().unwrap()).unwrap(),
                body
            );
            let alloc = &obs["rpc"][1];
            assert_eq!(alloc["method"], "AllocID");
            let request: Vec<u8> = serde_json::from_value(alloc["request"].clone()).unwrap();
            let request = pdpb::AllocIdRequest::decode(request.as_slice()).unwrap();
            assert_eq!(request.count, 1001);
            let response: Vec<u8> = serde_json::from_value(alloc["response"].clone()).unwrap();
            let response = pdpb::AllocIdResponse::decode(response.as_slice()).unwrap();
            assert!(response.header.unwrap().error.is_none() && response.id > 0);
            let bytes: Vec<u8> = serde_json::from_value(obs["persisted"]["bytes"].clone()).unwrap();
            let persisted = u64::from_be_bytes(bytes.try_into().unwrap());
            assert!(persisted >= response.id);
            assert!(obs["persisted"]["mod_revision"].as_i64().unwrap() > 0);
            assert_eq!(
                obs["persisted"]["key"],
                format!("/pd/{}/alloc_id", request.header.unwrap().cluster_id)
            );
            let metric = &obs["metrics"];
            assert_eq!(metric["path"], "/metrics");
            assert_eq!(metric["status"], 200);
            let lines = metric["lines"].as_array().unwrap();
            assert!(lines.contains(&json!("# HELP pd_cluster_id Record of id allocator.")));
            assert!(lines.contains(&json!("# TYPE pd_cluster_id gauge")));
            assert!(lines.contains(&json!(format!(
                "pd_cluster_id{{type=\"idalloc\"}} {persisted}"
            ))));
        }
    }
    assert_eq!(records.len(), 14);
}

fn scenario(line: &str) -> Option<&'static str> {
    let first = line.split_whitespace().nth(1).unwrap_or("");
    match first {
        "F-045" | "F-046" | "F-047" => Some("cli-exit"),
        "C-117" | "C-151" | "C-160" | "F-048" | "F-057" | "F-058" => Some("config-and-file-log"),
        "M-035" => Some("idalloc-sample"),
        "M-146" | "M-150" => Some("tso-metric-boundary"),
        _ if line.starts_with("| [cmd/pd-server/main.go:55]")
            || line.starts_with("| [cmd/pd-server/main.go:211]") =>
        {
            Some("mode-http")
        }
        _ if line.starts_with("| [server/config/config.go:314]")
            || line.starts_with("| [server/config/config.go:315]")
            || line.starts_with("| [server/config/config.go:316]") =>
        {
            Some("config-and-file-log")
        }
        _ if line.starts_with("| [pkg/id/metrics.go:30]") => Some("idalloc-sample"),
        _ => None,
    }
}

#[test]
fn sa_021_inventory() {
    let root = root();
    let doc = fs::read_to_string(root.join(SURFACE)).unwrap();
    let coverage = load_json(&root, COVERAGE);
    let index = &coverage["surface_audit_21"];
    assert_eq!(index["receipt"], RECEIPT, "missing current surface audit");
    let report = load_json(&root, RECEIPT);
    let head = report["head"].as_str().unwrap();
    assert_eq!(index["head"], head);
    let before = read_command(&root, "git", &["show", &format!("{head}:{SURFACE}")]);
    assert_eq!(report["surface_before"], digest(&before));
    assert_eq!(report["surface_after"], digest(doc.as_bytes()));
    assert_eq!(
        report["historical_receipt_sha256"],
        digest(&fs::read(root.join(HISTORICAL)).unwrap())
    );
    let manifest = verify_manifest(&doc, &coverage["manifests"]["surface"]).unwrap();
    assert_eq!(
        manifest,
        self::manifest(std::str::from_utf8(&before).unwrap()).unwrap()
    );

    let transition = index["source_transition"].as_object().unwrap();
    let transition_paths = [
        SURFACE,
        "pkg/runtime/member_role_worker.rs",
        "tests/rust-compat/accept_inventory_19_test.rs",
        "tests/rust-compat/accept_inventory_20_test.rs",
        "tests/rust-compat/lib.rs",
    ];
    assert_eq!(
        transition
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        transition_paths.into_iter().collect()
    );
    for path in transition_paths {
        let old = read_command(&root, "git", &["show", &format!("{head}:{path}")]);
        let new = fs::read(root.join(path)).unwrap();
        assert_eq!(transition[path]["before"], digest(&old), "{path}");
        assert_eq!(transition[path]["after"], digest(&new), "{path}");
        assert_ne!(
            transition[path]["before"], transition[path]["after"],
            "{path}"
        );
    }

    let rows = owned_rows(&doc);
    let recorded_rows = report["rows"].as_object().unwrap();
    assert!(!rows.is_empty());
    assert_eq!(
        rows.keys().collect::<Vec<_>>(),
        recorded_rows.keys().collect::<Vec<_>>()
    );
    let frozen_rows: BTreeMap<_, _> = coverage["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["inventory"] == SURFACE && row["section"] != MANIFEST)
        .map(|row| (row["row_sha256"].as_str().unwrap(), row))
        .collect();
    let mut expected_paths: BTreeSet<String> = manifest.keys().map(ToString::to_string).collect();
    let mut bound = 0;
    let mut joined_handoffs = 0;
    for (id, (section, line, owners)) in rows {
        let entry = &recorded_rows[&id];
        assert_eq!(entry["acceptance_task"], 21, "{line}");
        assert_eq!(entry["section"], section);
        assert_eq!(entry["row_sha256"], digest(line.as_bytes()));
        assert_eq!(entry["owners"], json!(owners));
        if let Some(frozen) = frozen_rows.get(digest(line.as_bytes()).as_str()) {
            assert_eq!(entry["handoff_id"], frozen["id"], "{line}");
            assert_eq!(line, frozen["raw_row"]);
            joined_handoffs += 1;
        } else {
            assert!(entry["handoff_id"].is_null());
        }
        let paths = sources(line);
        assert_eq!(entry["sources"], json!(paths), "{line}");
        expected_paths.extend(paths);
        let case = scenario(line);
        assert_eq!(entry["scenario"].as_str(), case, "{line}");
        assert_eq!(entry["business_admitted"], false, "{line}");
        assert!(
            entry["reason"].as_str().is_some_and(|s| !s.is_empty()),
            "{line}"
        );
        bound += usize::from(case.is_some());
    }
    assert_eq!(joined_handoffs, frozen_rows.len());
    assert_eq!(index["bound_rows"], bound);
    assert_eq!(index["owned_rows"], recorded_rows.len());

    let live = &report["live"];
    verify_observations(live);
    let inputs = live["inputs"].as_object().unwrap();
    assert!(inputs.contains_key(TEST));
    assert!(inputs.contains_key("pkg/runtime/member_role_worker.rs"));
    for (path, hash) in inputs {
        assert_eq!(hash, &report["sources"][path]["current_sha256"], "{path}");
    }
    expected_paths.extend(inputs.keys().cloned());
    expected_paths.extend(transition.keys().cloned());
    expected_paths.extend(
        [
            HISTORICAL,
            "docs/rust/server-api/acceptance/task-19.json.gz",
            "docs/rust/server-api/acceptance/task-20.json.gz",
        ]
        .map(str::to_owned),
    );
    let mut args = vec![
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
    ];
    args.extend(SCOPE);
    let files = read_command(&root, "git", &args);
    expected_paths.extend(
        std::str::from_utf8(&files)
            .unwrap()
            .split_terminator('\0')
            .map(str::to_owned),
    );
    let entries = report["sources"].as_object().unwrap();
    assert_eq!(
        entries.keys().cloned().collect::<BTreeSet<_>>(),
        expected_paths
    );
    let mut counts = BTreeMap::new();
    for (path, entry) in entries {
        assert!(canonical(path), "{path}");
        assert_eq!(entry["acceptance_task"], 21, "{path}");
        let original = manifest.get(path.as_str());
        assert_eq!(
            entry["historical_sha256"].as_str(),
            original.map(|v| v.0),
            "{path}"
        );
        assert_eq!(
            entry["historical_owner"].as_str(),
            original.map(|v| v.1),
            "{path}"
        );
        let bytes = fs::read(root.join(path)).unwrap_or_else(|err| panic!("{path}: {err}"));
        let actual = digest(&bytes);
        check_input(entry, &actual).unwrap_or_else(|err| panic!("{err}: {path}"));
        let change = match original {
            None => "added",
            Some((old, _)) if *old == actual => "same",
            _ => "changed",
        };
        assert_eq!(entry["change"], change, "{path}");
        assert!(
            entry["reason"].as_str().is_some_and(|s| !s.is_empty()),
            "{path}"
        );
        if change != "same" {
            assert!(
                entry["last_touch"].as_str().is_some_and(|s| !s.is_empty()),
                "{path}"
            );
        }
        *counts.entry(change).or_insert(0) += 1;
    }
    assert_eq!(index["source_counts"], json!(counts));
}

#[test]
fn sa_021_inventory_negative_controls() {
    let root = root();
    let doc = fs::read_to_string(root.join(SURFACE)).unwrap();
    let coverage = load_json(&root, COVERAGE);
    let frozen = &coverage["manifests"]["surface"];
    let entries = verify_manifest(&doc, frozen).unwrap();
    let (path, (hash, owner)) = entries.into_iter().next().unwrap();
    let row = format!("| [{path}](../../../{path}) | {hash} | {owner} |\n");
    assert_eq!(
        verify_manifest(&doc.replacen(&row, "", 1), frozen),
        Err("historical manifest set drift")
    );
    assert_eq!(
        verify_manifest(&doc.replacen(&row, &row.repeat(2), 1), frozen),
        Err("duplicate manifest input")
    );

    let report = load_json(&root, RECEIPT);
    let bytes = fs::read(root.join(path)).unwrap();
    let entry = &report["sources"][path];
    assert_eq!(check_input(entry, &digest(&bytes)), Ok(()));
    assert_eq!(
        check_input(entry, &digest(&[bytes.as_slice(), b"\n"].concat())),
        Err("surface source input drift")
    );

    let mut run = report["live"].clone();
    assert_eq!(check_business(&run), Ok(()));
    run["kind"] = "metric-only".into();
    assert_eq!(
        check_business(&run),
        Err("missing passing dual business evidence")
    );
}
