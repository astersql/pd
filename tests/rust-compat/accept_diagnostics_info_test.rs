// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::diagnosticspb::{ServerInfoRequest, diagnostics_client::DiagnosticsClient};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};
type Client = DiagnosticsClient<tonic::transport::Channel>;
async fn observe(client: &mut Client, tp: i32) -> Value {
    let mut request = tonic::Request::new(ServerInfoRequest { tp });
    request.set_timeout(Duration::from_secs(20));
    match client.server_info(request).await {
        Ok(r) => {
            json!({"items":r.into_inner().items.iter().map(|i|json!({"tp":i.tp,"name":i.name,"pairs":i.pairs.iter().map(|p|json!({"key":p.key,"value":p.value})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
        }
        Err(e) => json!({"code":e.code() as i32,"message":e.message()}),
    }
}
fn save(records: &[Value]) {
    std::fs::write(
        std::env::var_os("PD_SA015_RECEIPT").unwrap(),
        serde_json::to_vec(records).unwrap(),
    )
    .unwrap();
}
async fn record(
    client: &mut Client,
    records: &mut Vec<Value>,
    rust: bool,
    scenario: &str,
    tp: i32,
) -> Value {
    let sample = scenario != "unavailable" && (tp == 0 || tp == 3);
    let before = sample.then(memory_sample);
    let response = observe(client, tp).await;
    let after = sample.then(memory_sample);
    if let (Some(before), Some(after)) = (&before, &after) {
        let free = response["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tp"] == "memory" && i["name"] == "virtual")
            .unwrap()["pairs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["key"] == "free")
            .unwrap()["value"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let low = before["available_bytes"]
            .as_u64()
            .unwrap()
            .min(after["available_bytes"].as_u64().unwrap())
            .saturating_sub(256 * 1024 * 1024);
        let high = before["available_bytes"]
            .as_u64()
            .unwrap()
            .max(after["available_bytes"].as_u64().unwrap())
            + 256 * 1024 * 1024;
        assert!(
            (low..=high).contains(&free),
            "memory byte units: observed {free}, independent interval {low}..={high}"
        );
    }
    println!(
        "sa015 rust={rust} {scenario} tp={tp} items={}",
        response["items"].as_array().map_or(0, Vec::len)
    );
    records.push(json!({"rust":rust,"scenario":scenario,"request":tp,"response":response,"memory_sample_before":before,"memory_sample_after":after}));
    save(records);
    assert!(
        response["items"].is_array(),
        "ServerInfo {scenario} tp={tp}: {response}"
    );
    let items = response["items"].as_array().unwrap();
    assert!(
        items
            .windows(2)
            .all(|w| (w[0]["tp"].as_str(), w[0]["name"].as_str())
                <= (w[1]["tp"].as_str(), w[1]["name"].as_str())),
        "unsorted {response}"
    );
    if !(0..=3).contains(&tp) {
        assert!(items.is_empty());
    } else if scenario != "unavailable" {
        assert!(!items.is_empty(), "{scenario} tp={tp}");
    }
    response
}
fn memory_sample() -> Value {
    let run = |command: &str, args: &[&str]| {
        let out = std::process::Command::new("/usr/bin/arch")
            .arg("-x86_64")
            .arg(command)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    let page = run("/usr/bin/getconf", &["PAGESIZE"])
        .trim()
        .parse::<u64>()
        .unwrap();
    assert_eq!(page, 4096, "the admitted oracle is Darwin amd64");
    let text = run("/usr/bin/vm_stat", &[]);
    let count = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap()
            .trim()
            .trim_end_matches('.')
            .parse::<u64>()
            .unwrap()
    };
    json!({"process_page_bytes":page,"vm_stat":text,"available_bytes":(count("Pages free:")+count("Pages inactive:"))*page})
}
struct Commands(PathBuf);
impl Commands {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("pd-sa015-commands-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let result = Self(path);
        for (name, real) in [
            ("vm_stat", "/usr/bin/vm_stat"),
            ("sysctl", "/usr/sbin/sysctl"),
            ("netstat", "/usr/sbin/netstat"),
        ] {
            let script = result.0.join(name);
            std::fs::write(&script,format!("#!/bin/sh\nif [ -e \"$0.fail\" ]; then\n echo 'partial: must not leak'\n echo 'permission denied' >&2\n exit 1\nfi\nif [ -e \"$0.timeout\" ]; then exec /bin/sleep 6; fi\nif [ -e \"$0.stderr\" ]; then\n {real} \"$@\"\n rc=$?\n echo 'sa015-stderr: kept: ignored' >&2\n exit \"$rc\"\nfi\nexec {real} \"$@\"\n")).unwrap();
            std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        result
    }
    fn environment(&self) -> Vec<(String, String)> {
        vec![(
            "PATH".into(),
            format!("{}:{}", self.0.display(), std::env::var("PATH").unwrap()),
        )]
    }
    fn fail(&self, enabled: bool) {
        for name in ["vm_stat.fail", "sysctl.fail", "netstat.fail"] {
            if enabled {
                std::fs::write(self.0.join(name), b"").unwrap();
            } else {
                std::fs::remove_file(self.0.join(name)).unwrap();
            }
        }
    }
}
impl Drop for Commands {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_015_server_info() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut receipts = Vec::new();
    let mut servers = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = DiagnosticsClient::connect(server.url.clone())
            .await
            .unwrap();
        for tp in [1, 2, 3, 0, -1, 4, i32::MIN, i32::MAX] {
            record(&mut client, &mut receipts, is_rust, "normal", tp).await;
        }
        let commands = Commands::new();
        let mut args = if is_rust {
            vec![format!("--etcd-endpoints={}", server.metadata_url())]
        } else {
            vec![
                format!("--join={}", server.url),
                "--initial-cluster=".into(),
            ]
        };
        args.push("--name=follower15".into());
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let mut follower =
            Server::start_with_environment(binary, &refs, &commands.environment()).unwrap();
        follower.ready().unwrap();
        let (status, members) = follower.get("/pd/api/v1/members").unwrap();
        assert_eq!(status, 200);
        assert_eq!(members["leader"]["name"], "compat");
        assert_ne!(members["leader"]["client_urls"], json!([follower.url]));
        receipts.push(json!({"rust":is_rust,"membership":members}));
        let mut local = DiagnosticsClient::connect(follower.url.clone())
            .await
            .unwrap();
        commands.fail(true);
        for tp in [1, 2, 3, 0, -1, 4] {
            let r = record(&mut local, &mut receipts, is_rust, "unavailable", tp).await;
            let items = r["items"].as_array().unwrap();
            assert!(!items.iter().any(|i| i["tp"] == "system"
                || (i["tp"] == "memory" && (i["name"] == "virtual" || i["name"] == "memory"))));
            if tp == 3 {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0]["tp"], "cpu");
                assert_eq!(items[1]["name"], "swap");
            }
        }
        let leader = record(&mut client, &mut receipts, is_rust, "leader unaffected", 2).await;
        assert_eq!(leader["items"].as_array().unwrap().len(), 1);
        commands.fail(false);
        for tp in [1, 2, 3, 0] {
            record(&mut local, &mut receipts, is_rust, "recovered", tp).await;
        }
        std::fs::write(commands.0.join("sysctl.stderr"), b"").unwrap();
        record(&mut local, &mut receipts, is_rust, "success stderr", 2).await;
        std::fs::remove_file(commands.0.join("sysctl.stderr")).unwrap();
        std::fs::write(commands.0.join("vm_stat.timeout"), b"").unwrap();
        let started = std::time::Instant::now();
        record(&mut local, &mut receipts, is_rust, "command timeout", 1).await;
        let elapsed = started.elapsed();
        receipts.push(json!({"rust":is_rust,"command_timeout_ms":elapsed.as_millis()}));
        save(&receipts);
        assert!(
            (Duration::from_millis(2500)..Duration::from_millis(5500)).contains(&elapsed),
            "collector command timeout: {elapsed:?}"
        );
        std::fs::remove_file(commands.0.join("vm_stat.timeout")).unwrap();
        follower.stop().unwrap();
        servers.push(server);
        save(&receipts);
    }
    let mut go_client = DiagnosticsClient::connect(servers[0].url.clone())
        .await
        .unwrap();
    for _ in 0..2 {
        record(&mut go_client, &mut receipts, false, "volatility probe", 2).await;
    }
    for mut server in servers {
        server.stop().unwrap();
    }
    let classification = compare(&receipts);
    receipts.push(json!({"same_run_go_volatility":classification,"hosts_stopped":true}));
    save(&receipts);
}
// Compare every retained entry and pair; equal sort keys in Go's unstable
// sort.Slice have no tie order. Match those ties by their pair-key signature.
fn compare(records: &[Value]) -> Value {
    let dynamic: Value =
        serde_json::from_str(include_str!("fixtures/diagnostics-info-darwin-amd64.json")).unwrap();
    let mut dynamic_keys: Vec<String> = dynamic["dynamic_sysctl"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let mut go_values: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for r in records
        .iter()
        .filter(|r| r["rust"] == false && r.get("request").is_some())
    {
        for item in r["response"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["tp"] == "system")
        {
            for pair in item["pairs"].as_array().unwrap() {
                go_values
                    .entry(pair["key"].as_str().unwrap().into())
                    .or_default()
                    .insert(pair["value"].as_str().unwrap().into());
            }
        }
    }
    let live: std::collections::BTreeMap<_, _> =
        go_values.into_iter().filter(|(_, v)| v.len() > 1).collect();
    dynamic_keys.extend(live.keys().cloned());
    let go: Vec<_> = records
        .iter()
        .filter(|r| {
            r["rust"] == false && r.get("request").is_some() && r["scenario"] != "volatility probe"
        })
        .collect();
    let rust: Vec<_> = records
        .iter()
        .filter(|r| r["rust"] == true && r.get("request").is_some())
        .collect();
    assert_eq!(go.len(), 21);
    assert_eq!(go.len(), rust.len());
    for (g, r) in go.iter().zip(rust) {
        assert_eq!(g["scenario"], r["scenario"]);
        assert_eq!(g["request"], r["request"]);
        let signature = |v: &Value| {
            format!(
                "{}:{}:{}",
                v["tp"],
                v["name"],
                v["pairs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["key"].as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let mut a = g["response"]["items"].as_array().unwrap().clone();
        let mut b = r["response"]["items"].as_array().unwrap().clone();
        a.sort_by_key(&signature);
        b.sort_by_key(&signature);
        assert_eq!(
            a.len(),
            b.len(),
            "scenario {} tp={}",
            g["scenario"],
            g["request"]
        );
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(
                signature(a),
                signature(b),
                "scenario {} tp={}",
                g["scenario"],
                g["request"]
            );
            validate(a);
            validate(b);
            for (gp, rp) in a["pairs"]
                .as_array()
                .unwrap()
                .iter()
                .zip(b["pairs"].as_array().unwrap())
            {
                compare_value(a, gp, rp, &dynamic_keys);
            }
        }
    }
    json!(live)
}

fn compare_value(item: &Value, go: &Value, rust: &Value, dynamic_keys: &[String]) {
    let key = go["key"].as_str().unwrap();
    assert_eq!(go["key"], rust["key"]);
    let g = go["value"].as_str().unwrap();
    let r = rust["value"].as_str().unwrap();
    let tp = item["tp"].as_str().unwrap();
    let name = item["name"].as_str().unwrap();
    if tp == "system" && dynamic_keys.iter().any(|s| s == key) {
        // Explicitly observed changing keys only. Preserve token count, unit,
        // punctuation and ordering; every sampled number must be nonnegative.
        let shape = |text: &str| {
            let mut result = String::new();
            let mut chars = text.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch.is_ascii_digit() {
                    let mut number = ch.to_string();
                    while chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
                    {
                        number.push(chars.next().unwrap());
                    }
                    let n: f64 = number.parse().unwrap();
                    assert!(
                        n.is_finite() && n >= 0.0 && n <= u64::MAX as f64,
                        "{key} {text}"
                    );
                    result.push('#');
                } else {
                    result.push(ch);
                }
            }
            result
        };
        assert_eq!(shape(g), shape(r), "sysctl {key}");
    } else if tp == "disk" && ["free", "used", "free-percent", "used-percent"].contains(&key) {
        // Independently checked byte accounting and rounded fractions below.
    } else if tp == "memory" && name != "memory" && key != "total" {
        // Independently checked byte accounting and rounded fractions below.
    } else if tp == "memory" && name == "swap" && key == "total" {
        // The OS may resize swap between observations; validate its full tuple.
    } else if tp == "cpu" && key.starts_with("load") {
        for v in [g, r] {
            let n: f64 = v.parse().unwrap();
            assert!((0.0..=u32::MAX as f64).contains(&n));
            assert_eq!(v, format!("{n:.2}"));
        }
    } else if tp == "net"
        && [
            "bytes-ent",
            "bytes-recv",
            "packets-sent",
            "packets-recv",
            "errin",
            "errout",
            "dropin",
            "dropout",
            "fifoin",
            "fifoout",
        ]
        .contains(&key)
    {
        let g: u64 = g.parse().unwrap();
        let r: u64 = r.parse().unwrap();
        assert!(r >= g, "counter reset {name}/{key}: {g} -> {r}");
    } else {
        assert_eq!(g, r, "static {tp}/{name}/{key}");
    }
}
fn validate(item: &Value) {
    let pairs = item["pairs"].as_array().unwrap();
    for p in pairs {
        assert!(p["key"].is_string());
        assert!(p["value"].is_string());
    }
    let get = |key: &str| {
        pairs.iter().find(|p| p["key"] == key).unwrap()["value"]
            .as_str()
            .unwrap()
    };
    if item["tp"] == "disk" || (item["tp"] == "memory" && item["name"] != "memory") {
        let total: u64 = get("total").parse().unwrap();
        let free: u64 = get("free").parse().unwrap();
        let used: u64 = get("used").parse().unwrap();
        assert!(free <= total && used <= total);
        if item["tp"] == "memory" {
            assert_eq!(used + free, total);
        } else {
            assert!(used + free <= total);
        }
        let denominator = if item["tp"] == "disk" {
            used + free
        } else {
            total
        };
        let ratio = if denominator == 0 {
            if item["tp"] == "disk" { 0.0 } else { f64::NAN }
        } else {
            used as f64 / denominator as f64
        };
        assert_eq!(
            get("used-percent"),
            if ratio.is_nan() {
                "NaN".into()
            } else {
                format!("{ratio:.2}")
            }
        );
        let free_ratio = if item["tp"] == "disk" || item["name"] == "virtual" {
            1.0 - ratio
        } else {
            free as f64 / total as f64
        };
        assert_eq!(
            get("free-percent"),
            if free_ratio.is_nan() {
                "NaN".into()
            } else {
                format!("{free_ratio:.2}")
            }
        );
        if item["tp"] == "disk" || item["name"] == "virtual" {
            assert!(total > 0);
        }
    }
}
