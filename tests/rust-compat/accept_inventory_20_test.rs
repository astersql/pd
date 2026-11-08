// Copyright 2026 AsterSQL.

use prost::Message;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use super::accept_inventory_19_test::{canonical, digest, read_command, sha256};

const WIRE: &str = "docs/rust/server-api/wire-inventory.md";
const COVERAGE: &str = "docs/rust/server-api/acceptance/coverage.json";
const RECEIPT: &str = "docs/rust/server-api/acceptance/task-20.json.gz";
const HISTORICAL: &str = "docs/rust/server-api/task-102-resume-evidence.json.gz";
const RETIRED: &str = ".plans/2026-10-27-pd-rust-服务端与API/plan.md";
const CLASSIC: &str = "## PD descriptor 全方法";
const EXTRA: &str = "## 额外服务与 descriptor 注册差异";
const HTTP: &str = "## HTTP 全量注册行";
const SOURCE_SCOPE: &[&str] = &[
    "pkg/proto/",
    "pkg/server/",
    "pkg/mcs/*/server/*.rs",
    "pkg/mcs/server/*.rs",
    "tests/rust-compat/accept_*_test.rs",
];

fn frozen_manifest(doc: &str) -> Result<BTreeMap<&str, &str>, &'static str> {
    let (_, tail) = doc
        .split_once("## 来源 SHA-256 manifest\n")
        .ok_or("missing manifest heading")?;
    let (_, tail) = tail
        .split_once("```text\n")
        .ok_or("missing manifest fence")?;
    let (body, _) = tail.split_once("```\n").ok_or("unclosed manifest")?;
    let mut entries = BTreeMap::new();
    for line in body.lines() {
        let (hash, path) = line.split_once("  ").ok_or("invalid manifest row")?;
        if !sha256(hash) || !canonical(path) || path.trim() != path {
            return Err("invalid manifest input");
        }
        if entries.insert(path, hash).is_some() {
            return Err("duplicate manifest input");
        }
    }
    let contents: String = entries
        .iter()
        .map(|(path, hash)| format!("{hash}  {path}\n"))
        .collect();
    if entries.is_empty() || !tail_header(doc)?.contains(&digest(contents.as_bytes())) {
        return Err("historical manifest set drift");
    }
    Ok(entries)
}

fn tail_header(doc: &str) -> Result<&str, &'static str> {
    doc.split_once("## 来源 SHA-256 manifest\n")
        .and_then(|(_, tail)| tail.split_once("```text\n"))
        .map(|(header, _)| header)
        .ok_or("missing manifest digest")
}

fn check_current(entry: &Value, hash: &str) -> Result<(), &'static str> {
    if entry["current_sha256"] != hash {
        return Err("wire source input drift");
    }
    Ok(())
}

fn check_receipt(run: &Value) -> Result<(), &'static str> {
    if run["kind"] != "dual"
        || run["exit"] != 0
        || !run["log"]
            .as_str()
            .is_some_and(|log| log.contains("test result: ok. 1 passed; 0 failed; 0 ignored;"))
        || !run["observations"]
            .as_array()
            .is_some_and(|items| items.len() >= 2)
    {
        return Err("missing passing dual implementation receipt");
    }
    Ok(())
}

// Decode the service and method fields of google.protobuf.FileDescriptorSet.
// The pinned build already validates the archive. Message fields are irrelevant here.
#[derive(Message)]
struct DescriptorSet {
    #[prost(message, repeated, tag = "1")]
    files: Vec<File>,
}

#[derive(Message)]
struct File {
    #[prost(string, tag = "2")]
    package: String,
    #[prost(message, repeated, tag = "6")]
    services: Vec<Service>,
}

#[derive(Message)]
struct Service {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    methods: Vec<Method>,
}

#[derive(Message)]
struct Method {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    input: String,
    #[prost(string, tag = "3")]
    output: String,
    #[prost(bool, tag = "5")]
    client_streaming: bool,
    #[prost(bool, tag = "6")]
    server_streaming: bool,
}

fn methods(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut methods = BTreeMap::new();
    for file in DescriptorSet::decode(bytes).unwrap().files {
        for service in file.services {
            for method in service.methods {
                let id = format!("grpc:/{}.{}/{}", file.package, service.name, method.name);
                let input = if method.client_streaming {
                    "stream "
                } else {
                    ""
                };
                let output = if method.server_streaming {
                    "stream "
                } else {
                    ""
                };
                let signature = format!("{input}{} → {output}{}", method.input, method.output);
                assert!(methods.insert(id, signature).is_none());
            }
        }
    }
    methods
}

fn stable_id(section: &str, cells: &[&str]) -> String {
    match section {
        CLASSIC => format!("grpc:/pdpb.PD/{}", cells[0]),
        EXTRA => format!("grpc:/{}", cells[0]),
        HTTP => {
            let mut id = format!("http:{} {}", cells[0], cells[1]);
            if let Some((_, scheduler)) = cells[3].rsplit_once("来源类型 ") {
                id.push_str(&format!(" @{scheduler}"));
            }
            id
        }
        _ => unreachable!(),
    }
}

fn wire_rows(doc: &str) -> BTreeMap<String, (&str, &str, Vec<&str>)> {
    let mut section = "";
    let mut rows = BTreeMap::new();
    for line in doc.lines() {
        if line.starts_with("## ") {
            section = line;
        }
        if !matches!(section, CLASSIC | EXTRA | HTTP) || !line.starts_with("| ") {
            continue;
        }
        let cells: Vec<_> = line.trim_matches('|').split('|').map(str::trim).collect();
        if matches!(cells[0], "方法" | "服务/方法" | "---") {
            continue;
        }
        let id = stable_id(section, &cells);
        assert!(rows.insert(id, (section, line, cells)).is_none(), "{line}");
    }
    rows
}

fn verify_rows(root: &Path, doc: &str, coverage: &Value, report: &Value) {
    use astersql_pd_proto::{FILE_DESCRIPTOR_SET, HEALTH_DESCRIPTOR_SET};

    let descriptors = methods(FILE_DESCRIPTOR_SET);
    let health = methods(HEALTH_DESCRIPTOR_SET);
    assert_eq!(report["descriptor_sha256"], digest(FILE_DESCRIPTOR_SET));
    assert_eq!(
        report["health_descriptor_sha256"],
        digest(HEALTH_DESCRIPTOR_SET)
    );
    let rows = wire_rows(doc);
    let mut frozen = BTreeMap::new();
    for row in coverage["rows"].as_array().unwrap() {
        let section = row["section"].as_str().unwrap();
        if row["inventory"] != WIRE || !matches!(section, CLASSIC | EXTRA | HTTP) {
            continue;
        }
        let cells: Vec<_> = row["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell.as_str().unwrap())
            .collect();
        assert!(frozen.insert(stable_id(section, &cells), row).is_none());
    }
    assert_eq!(
        frozen.keys().collect::<Vec<_>>(),
        rows.keys().collect::<Vec<_>>()
    );

    let catalog = super::http_regression::catalog();
    let mut http = BTreeMap::new();
    for case in &catalog {
        if case["inventory"] == "wire-inventory.md" {
            assert!(http.insert(case["id"].as_str().unwrap(), case).is_none());
        }
    }
    let http_cases: BTreeSet<_> = rows
        .values()
        .filter(|(section, _, _)| *section == HTTP)
        .map(|(_, _, cells)| cells[7].split_once('：').unwrap().0)
        .collect();
    assert_eq!(http.keys().copied().collect::<BTreeSet<_>>(), http_cases);

    let registrations = report["registrations"].as_object().unwrap();
    assert_eq!(
        registrations
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        rows.keys()
            .chain(health.keys())
            .map(String::as_str)
            .collect()
    );
    assert_eq!(
        descriptors.keys().collect::<BTreeSet<_>>(),
        rows.keys().filter(|id| id.starts_with("grpc:")).collect()
    );
    assert_eq!(
        descriptors
            .keys()
            .filter(|id| id.starts_with("grpc:/pdpb.PD/"))
            .count(),
        56
    );

    let mut counts = BTreeMap::new();
    for (id, row) in registrations {
        let bucket = if let Some((section, line, cells)) = rows.get(id) {
            let original = frozen[id];
            assert_eq!(row["current_row_sha256"], digest(line.as_bytes()), "{id}");
            let owner = cells[if *section == HTTP { 5 } else { 4 }];
            let owner = owner.parse::<u16>().unwrap();
            assert_eq!(row["historical_owner"], owner, "{id}");
            assert_eq!(original["owner102"], owner == 102, "{id}");
            assert_eq!(row["handoff_id"], original["id"], "{id}");
            assert_eq!(row["acceptance_task"], original["acceptance_task"], "{id}");
            if *section == HTTP {
                let case = cells[7].split_once('：').unwrap().0;
                assert_eq!(http[case]["row"], original["raw_row"], "{id}");
                "http"
            } else {
                assert_eq!(descriptors[id], cells[1], "{id}");
                if *section == CLASSIC {
                    "classic-pd"
                } else if [
                    "deadlock.Deadlock/",
                    "tracepb.TraceRecordPubSub/",
                    "keyspacepb.Namespace/",
                ]
                .iter()
                .any(|prefix| id.starts_with(&format!("grpc:/{prefix}")))
                {
                    "descriptor-only"
                } else {
                    "extra-grpc"
                }
            }
        } else {
            assert_eq!(row["signature"], health[id], "{id}");
            assert_eq!(row["acceptance_task"], 16, "{id}");
            "extra-grpc"
        };
        assert_eq!(row["surface"], bucket, "{id}");
        assert!(
            row["implementation_owner"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "{id}"
        );
        *counts.entry(bucket).or_insert(0) += 1;
        assert!(
            row["reason"].as_str().is_some_and(|s| !s.is_empty()),
            "{id}"
        );
        for kind in ["go", "rust", "tests"] {
            let paths = row[kind].as_array().unwrap();
            assert!(!paths.is_empty(), "{id}/{kind}");
            for path in paths {
                let path = path.as_str().unwrap();
                assert!(!report["sources"][path].is_null(), "{id}/{kind}/{path}");
            }
        }
        let prior = row["prior_receipt"].as_str().unwrap();
        assert!(
            canonical(prior) && root.join(prior).is_file(),
            "{id}: {prior}"
        );
        if let Some(run) = row["current_run"].as_str() {
            let run = &report["runs"][run];
            check_receipt(run).unwrap_or_else(|err| panic!("{id}: {err}"));
            assert!(run["rows"].as_array().unwrap().iter().any(|key| key == id));
        }
    }
    assert_eq!(report["counts"], serde_json::json!(counts));
}

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_report(root: &Path) -> Value {
    serde_json::from_slice(&read_command(root, "gzip", &["-dc", RECEIPT])).unwrap()
}

#[test]
fn sa_020_inventory() {
    let root = root();
    let doc = fs::read_to_string(root.join(WIRE)).unwrap();
    let original = frozen_manifest(&doc).unwrap();
    let coverage: Value = serde_json::from_slice(&fs::read(root.join(COVERAGE)).unwrap()).unwrap();
    let report = read_report(&root);
    let index = &coverage["wire_audit_20"];
    let surface = &coverage["surface_audit_21"]["source_transition"];
    assert_eq!(index["receipt"], RECEIPT);
    assert_eq!(index["head"], report["head"]);
    assert_eq!(index["counts"], report["counts"]);
    let head = report["head"].as_str().unwrap();
    let before = read_command(&root, "git", &["show", &format!("{head}:{WIRE}")]);
    assert_eq!(report["wire_before"], digest(&before));
    assert_eq!(report["wire_after"], digest(doc.as_bytes()));
    assert_eq!(
        report["historical_receipt_sha256"],
        digest(&fs::read(root.join(HISTORICAL)).unwrap())
    );

    let mut frozen = BTreeMap::new();
    for row in coverage["manifests"]["wire"].as_array().unwrap() {
        assert_eq!(row["acceptance_task"], 20);
        let path = row["path"]
            .as_str()
            .unwrap()
            .strip_prefix("../../../")
            .unwrap_or_else(|| row["path"].as_str().unwrap());
        assert!(
            frozen
                .insert(path, row["historical_sha256"].as_str().unwrap())
                .is_none()
        );
    }
    assert_eq!(
        original, frozen,
        "frozen manifest membership or byte hash changed"
    );
    assert_eq!(
        original.keys().copied().collect::<BTreeSet<_>>(),
        report["sources"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, row)| row["historical_sha256"].is_string())
            .map(|(path, _)| path.as_str())
            .collect()
    );
    let mut args = vec![
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
    ];
    args.extend(SOURCE_SCOPE);
    let files = read_command(&root, "git", &args);
    let mut expected: BTreeSet<_> = std::str::from_utf8(&files)
        .unwrap()
        .split_terminator('\0')
        .filter(|path| {
            !super::accept_inventory_21_test::owns_new_audit_path(path) || surface.is_null()
        })
        .chain(original.keys().copied())
        .collect();
    for row in report["registrations"].as_object().unwrap().values() {
        for kind in ["go", "rust", "tests"] {
            expected.extend(
                row[kind]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|path| path.as_str().unwrap()),
            );
        }
        expected.insert(row["prior_receipt"].as_str().unwrap());
    }
    expected.extend(
        index["source_transition"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str),
    );
    assert_eq!(
        expected,
        report["sources"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect(),
        "wire source set drift"
    );
    for (path, handoff) in index["source_transition"].as_object().unwrap() {
        let baseline = read_command(&root, "git", &["show", &format!("{head}:{path}")]);
        assert_eq!(handoff["before"], digest(&baseline), "{path}");
        assert_eq!(
            handoff["after"], report["sources"][path]["current_sha256"],
            "{path}"
        );
    }
    for (path, entry) in report["sources"].as_object().unwrap() {
        assert!(canonical(path));
        assert_eq!(
            entry["historical_sha256"].as_str(),
            original.get(path.as_str()).copied()
        );
        assert_eq!(entry["acceptance_task"], 20);
        assert!(
            entry["reason"].as_str().is_some_and(|s| !s.is_empty()),
            "{path}"
        );
        let bytes = if path == RETIRED {
            assert_eq!(entry["change"], "retired");
            let baseline = read_command(&root, "git", &["show", &format!("{head}:{path}")]);
            if let Ok(bytes) = fs::read(root.join(path)) {
                assert_eq!(bytes, baseline, "{path}");
            }
            baseline
        } else {
            fs::read(root.join(path)).unwrap_or_else(|err| panic!("{path}: {err}"))
        };
        let hash = digest(&bytes);
        if surface.get(path).is_some() {
            let expected = super::accept_inventory_21_test::successor_hash(
                surface,
                path,
                &entry["current_sha256"],
            );
            assert_eq!(expected, &hash, "wire source input drift: {path}");
        } else {
            check_current(entry, &hash).unwrap_or_else(|err| panic!("{path}: {err}"));
        }
        if path != RETIRED {
            let expected = match original.get(path.as_str()) {
                None => "added",
                Some(old) if entry["current_sha256"] == *old => "same",
                Some(_) => "changed",
            };
            assert_eq!(entry["change"], expected, "{path}");
        }
        if entry["change"] != "same" {
            let attribution = entry["last_touch"].as_str().unwrap();
            assert!(!attribution.is_empty(), "{path}");
        }
    }
    let counts =
        report["sources"]
            .as_object()
            .unwrap()
            .values()
            .fold(BTreeMap::new(), |mut counts, row| {
                *counts.entry(row["change"].as_str().unwrap()).or_insert(0) += 1;
                counts
            });
    assert_eq!(report["source_counts"], serde_json::json!(counts));
    assert_eq!(index["source_counts"], report["source_counts"]);
    verify_rows(&root, &doc, &coverage, &report);
    verify_runs(&root, &report);
    println!(
        "{} historical inputs, {} checked source paths; registration rows {}",
        original.len(),
        report["sources"].as_object().unwrap().len(),
        report["counts"]
    );
}

fn verify_runs(root: &Path, report: &Value) {
    let runs = report["runs"].as_object().unwrap();
    for (id, run) in runs {
        check_receipt(run).unwrap_or_else(|err| panic!("{id}: {err}"));
        let test = run["test"].as_str().unwrap();
        let source = run["test_source"].as_str().unwrap();
        assert_eq!(
            run["source_sha256"],
            report["sources"][source]["current_sha256"]
        );
        let test_name = test.split("::").last().unwrap();
        assert!(
            fs::read_to_string(root.join(source))
                .unwrap()
                .contains(&format!("fn {test_name}("))
        );
        assert!(
            run["log"]
                .as_str()
                .unwrap()
                .contains(&format!("test {test} ... ok"))
        );
        for row in run["rows"].as_array().unwrap() {
            assert_eq!(
                report["registrations"][row.as_str().unwrap()]["current_run"],
                *id
            );
        }
    }

    let unregistered = runs["unregistered"]["observations"].as_array().unwrap();
    assert_eq!(unregistered.len(), 10);
    for role in ["classic", "resource-manager", "router", "scheduling", "tso"] {
        let pair: Vec<_> = unregistered
            .iter()
            .filter(|obs| obs["role"] == role)
            .collect();
        let [go, rust] = pair.as_slice() else {
            panic!("{role}: missing listener observations")
        };
        assert_eq!(go["rust"], false);
        assert_eq!(rust["rust"], true);
        assert_eq!(go["http_unknown"]["status"], 404);
        assert_eq!(rust["http_unknown"]["status"], 404);
        let go_body = if role == "classic" {
            "404 page not found\n"
        } else {
            "404 page not found"
        };
        assert_eq!(go["http_unknown"]["body"], go_body);
        assert_eq!(rust["http_unknown"]["body"], "");
        assert_eq!(go["responses"], rust["responses"]);
        let responses = go["responses"].as_object().unwrap();
        assert_eq!(
            responses.len(),
            runs["unregistered"]["rows"].as_array().unwrap().len()
        );
        for (method, response) in responses {
            assert_eq!(response["code"], "Unimplemented");
            assert_eq!(
                report["registrations"][format!("grpc:/{method}")]["surface"],
                "descriptor-only"
            );
        }
    }

    let common = runs["common"]["observations"].as_array().unwrap();
    assert_eq!(common.len(), 8);
    for role in ["resource-manager", "router", "scheduling", "tso"] {
        let pair: Vec<_> = common.iter().filter(|obs| obs["service"] == role).collect();
        let [go, rust] = pair.as_slice() else {
            panic!("{role}: missing common service observations")
        };
        assert_eq!(go["binary"], "go");
        assert_eq!(rust["binary"], "rust");
        assert_eq!(go["result"], rust["result"]);
        assert!(
            !go["result"]["list"]["protobuf"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(go["result"]["checks"].as_array().unwrap().len(), 4);
    }

    let [go, rust] = runs["gc"]["observations"].as_array().unwrap().as_slice() else {
        panic!("missing GC pair");
    };
    assert_eq!(go["implementation"], "go");
    assert_eq!(rust["implementation"], "rust");
    assert_eq!(go["sample"], rust["sample"]);
    for obs in [go, rust] {
        assert!(!obs["physical_evidence"].as_array().unwrap().is_empty());
    }

    let known = &report["known_divergence"];
    let receipt = known["receipt"].as_str().unwrap();
    let prior: Value =
        serde_json::from_slice(&read_command(root, "gzip", &["-dc", receipt])).unwrap();
    let run = prior.pointer(known["pointer"].as_str().unwrap()).unwrap();
    assert_eq!(run["exit"], 101);
    assert_eq!(run["admission"], "failed");
    let rm = &report["registrations"][known["id"].as_str().unwrap()];
    assert_eq!(rm["prior_receipt"], receipt);
    assert!(rm["current_run"].is_null());

    let http = runs["http"]["observations"].as_array().unwrap();
    assert_eq!(http.len(), 4);
    for rust in [false, true] {
        for api in [false, true] {
            let obs = http
                .iter()
                .find(|obs| obs["rust"] == rust && obs["api"] == api)
                .unwrap();
            for row in runs["http"]["rows"].as_array().unwrap() {
                let (method, template) = row
                    .as_str()
                    .unwrap()
                    .strip_prefix("http:")
                    .unwrap()
                    .split_once(' ')
                    .unwrap();
                let template = template.strip_prefix("/pd/api/").unwrap();
                assert!(
                    obs["http"].as_array().unwrap().iter().any(|request| {
                        let Some(path) = request["path"].as_str() else {
                            return false;
                        };
                        let path = path.split('?').next().unwrap();
                        let expected: Vec<_> = template.split('/').collect();
                        let actual: Vec<_> = path.split('/').collect();
                        request["method"] == method
                            && expected.len() == actual.len()
                            && expected.iter().zip(actual).all(|(expected, actual)| {
                                *expected == actual
                                    || (expected.starts_with(':') && !actual.is_empty())
                            })
                    }),
                    "{row} missing from {rust}/{api}"
                );
            }
            assert!(
                !obs["after_restart"]["entries"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert!(!obs["physical"].is_null());
        }
    }
}

#[test]
fn sa_020_inventory_negative_controls() {
    let root = root();
    let doc = fs::read_to_string(root.join(WIRE)).unwrap();
    let (path, old) = frozen_manifest(&doc).unwrap().into_iter().next().unwrap();
    let line = format!("{old}  {path}\n");
    assert_eq!(
        frozen_manifest(&doc.replacen(&line, "", 1)),
        Err("historical manifest set drift")
    );
    assert_eq!(
        frozen_manifest(&doc.replacen(&line, &line.repeat(2), 1)),
        Err("duplicate manifest input")
    );
    let report = read_report(&root);
    let row = &report["sources"][path];
    let bytes = fs::read(root.join(path)).unwrap();
    assert_eq!(check_current(row, &digest(&bytes)), Ok(()));
    assert_eq!(
        check_current(row, &digest(&[bytes.as_slice(), b"\n"].concat())),
        Err("wire source input drift")
    );
    let mut receipt = report["runs"]["unregistered"].clone();
    assert_eq!(check_receipt(&receipt), Ok(()));
    receipt["kind"] = "metric-only".into();
    assert_eq!(
        check_receipt(&receipt),
        Err("missing passing dual implementation receipt")
    );
}
