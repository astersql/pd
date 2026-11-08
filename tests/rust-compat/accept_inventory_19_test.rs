// Copyright 2026 AsterSQL.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};

const INVENTORY: &str = "docs/rust/server-api/source-inventory.md";
const ACCEPTANCE: &str = "docs/rust/server-api/acceptance/";
const SNAPSHOT: &str = "docs/rust/server-api/acceptance/task-19.json.gz";
const REPORT: &str = "docs/rust/server-api/acceptance/task-19.md";
const BASE: &str = "bdec774cc277fc2bb65ffa44cfbfb1849bfbe0e2";

type Manifest<'a> = BTreeMap<&'a str, (&'a str, &'a str)>;

fn canonical(path: &str) -> bool {
    !path.contains('\\') && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn manifest(document: &str) -> Result<Manifest<'_>, &'static str> {
    let (_, body) = document
        .split_once("## 完整来源SHA-256 manifest\n")
        .ok_or("missing manifest section")?;
    let body = body.split("\n## ").next().unwrap();
    let (_, table) = body
        .split_once("| 来源文件 | owner | SHA-256 |\n| --- | --- | --- |\n")
        .ok_or("missing manifest header")?;
    let mut rows = BTreeMap::new();
    for line in table.lines().filter(|line| !line.is_empty()) {
        let line = line
            .strip_prefix("| [`")
            .and_then(|line| line.strip_suffix("` |"))
            .ok_or("invalid manifest row")?;
        let (path, link) = line
            .split_once("`](../../../")
            .ok_or("invalid source link")?;
        let (target, rest) = link.split_once(") | ").ok_or("invalid source link")?;
        let (owner, hash) = rest.split_once(" | `").ok_or("invalid manifest fields")?;
        if !canonical(path) || target != path || owner.is_empty() || !sha256(hash) {
            return Err("invalid manifest fields");
        }
        if rows.insert(path, (hash, owner)).is_some() {
            return Err("duplicate source row");
        }
    }
    if rows.is_empty() {
        return Err("missing source rows");
    }
    Ok(rows)
}

fn historical_digest(rows: &Manifest<'_>) -> String {
    let text: String = rows
        .iter()
        .map(|(path, (hash, _))| format!("{hash}  {path}\n"))
        .collect();
    digest(text.as_bytes())
}

fn check_historical_set(rows: &Manifest<'_>, expected: &str) -> Result<(), &'static str> {
    if historical_digest(rows) != expected {
        return Err("historical source set drift");
    }
    Ok(())
}

fn read_command(root: &Path, command: &str, args: &[&str]) -> Vec<u8> {
    let output = Command::new(command)
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{command} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn source_addition(path: &str) -> bool {
    !matches!(
        path,
        SNAPSHOT | REPORT | "docs/rust/server-api/acceptance/coverage.json"
    ) && [
        "cmd/",
        "pkg/",
        "server/",
        "client/",
        "tests/",
        "tools/",
        "scripts/",
        "docs/rust/",
        "src/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

fn audit_paths<'a>(
    baseline: &Manifest<'a>,
    base_tree: &str,
    current_tree: &'a str,
    observed: &BTreeSet<&'a str>,
) -> BTreeSet<&'a str> {
    let base: BTreeSet<_> = base_tree.split_terminator('\0').collect();
    baseline
        .keys()
        .copied()
        .chain(observed.iter().copied())
        .chain(
            current_tree
                .split_terminator('\0')
                .filter(|path| source_addition(path) && !base.contains(path)),
        )
        .collect()
}

fn frozen_path(row: &Value) -> &str {
    let path = row["path"].as_str().unwrap();
    // The handoff retained unresolved Markdown links for two retired plans.
    path.strip_prefix("../../../").map_or(path, |canonical| {
        assert_eq!(row["reference"], path);
        canonical
    })
}

fn check_input(expected: &Value, current: Option<&str>) -> Result<(), &'static str> {
    if expected.is_null() && current.is_none() || expected.as_str() == current && current.is_some()
    {
        Ok(())
    } else {
        Err("source input drift")
    }
}

fn check_run<'a>(
    receipt: &'a Value,
    path: &str,
    current: Option<&str>,
) -> Result<&'a str, &'static str> {
    check_input(&receipt["inputs"][path], current)?;
    if current.is_none()
        || receipt["kind"] != "business"
        || receipt["exit"] != 0
        || receipt["observations"]["go"].is_null()
        || receipt["observations"]["rust"].is_null()
    {
        return Err("missing business evidence");
    }
    let test = receipt["test"].as_str().ok_or("missing test name")?;
    let log = receipt["log"].as_str().ok_or("missing test log")?;
    if !log.contains("test result: ok. 1 passed; 0 failed; 0 ignored;")
        || !log
            .lines()
            .any(|line| line.starts_with(&format!("test {test} ... ")))
    {
        return Err("test did not pass");
    }
    Ok(test)
}

fn check_rm_divergence(run: &Value) {
    use astersql_pd_proto::resource_manager::TokenBucketsResponse;
    use prost::Message;

    assert_eq!(run["exit"], 101);
    assert_eq!(run["admission"], "failed");
    assert!(
        run["log"]
            .as_str()
            .unwrap()
            .contains("0 passed; 1 failed; 0 ignored")
    );
    let [rust, go] = run["observations"].as_array().unwrap().as_slice() else {
        panic!("RM divergence requires observations from both implementations");
    };
    assert_eq!(rust["rust"], true);
    assert_eq!(go["rust"], false);
    for input in ["put", "requests", "initial_responses"] {
        assert!(!rust[input].as_array().unwrap().is_empty());
        assert_eq!(rust[input], go[input], "{input}");
    }
    for observation in [rust, go] {
        assert_eq!(observation["graceful_cleanup"], true);
        let before = &observation["etcd_before"]["setting"];
        assert_eq!(before.as_array().unwrap().len(), 1);
        assert_eq!(*before, observation["etcd_after"]["setting"]);
    }
    let wire: Vec<u8> = serde_json::from_value(rust["other_response"].clone()).unwrap();
    let response = TokenBucketsResponse::decode(wire.as_slice()).unwrap();
    assert_eq!(
        response.responses[0].granted_r_u_tokens[0]
            .granted_tokens
            .as_ref()
            .unwrap()
            .tokens,
        9.0
    );
    assert!(go["other_response"].is_null());
    assert_eq!(go["wait_for_go_close"], true);
    let close = go["delegate_closed_before_cleanup"].as_str().unwrap();
    assert!(close.contains("close delegate client connection"));
    assert!(close.contains(go["sidecar"].as_str().unwrap()));
    assert!(go["other_stream"].as_str().unwrap().starts_with("Ok(Err("));
}

#[test]
fn sa_019_inventory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let doc = fs::read_to_string(root.join(INVENTORY)).unwrap();
    let historical = manifest(&doc).unwrap();
    let coverage: Value =
        serde_json::from_slice(&fs::read(root.join(ACCEPTANCE).join("coverage.json")).unwrap())
            .unwrap();
    let frozen = coverage["manifests"]["source"].as_array().unwrap();
    let frozen_paths: BTreeSet<_> = frozen.iter().map(frozen_path).collect();
    assert_eq!(
        frozen.len(),
        frozen_paths.len(),
        "duplicate historical source row"
    );
    assert_eq!(frozen_paths, historical.keys().copied().collect());
    for row in frozen {
        let path = frozen_path(row);
        let &(hash, owner) = historical.get(path).expect("missing historical source row");
        assert_eq!(row["historical_sha256"], hash, "{path}");
        assert_eq!(
            row["historical_owner"],
            serde_json::json!([owner]),
            "{path}"
        );
    }
    let index = &coverage["source_audit_19"];
    assert_eq!(index["receipt"], SNAPSHOT);
    let snapshot: Value =
        serde_json::from_slice(&read_command(&root, "gzip", &["-dc", SNAPSHOT])).unwrap();
    assert_eq!(index["head"], snapshot["head"]);
    assert_eq!(index["latest_inputs_head"], snapshot["rm_followup"]["head"]);
    let head = index["head"].as_str().unwrap();
    let frozen_sha = index["historical_sha256"].as_str().unwrap();
    check_historical_set(&historical, frozen_sha).unwrap();
    assert!(doc.contains(frozen_sha));
    assert_eq!(snapshot["inventory_after"], digest(doc.as_bytes()));
    let before = read_command(&root, "git", &["show", &format!("{head}:{INVENTORY}")]);
    assert_eq!(snapshot["inventory_before"], digest(&before));

    let base = read_command(&root, "git", &["ls-tree", "-r", "--name-only", "-z", BASE]);
    let files = read_command(
        &root,
        "git",
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    );
    let runs = snapshot["runs"].as_object().unwrap();
    let divergences = snapshot["observed_divergences"].as_object().unwrap();
    let observed: BTreeSet<_> = runs
        .values()
        .chain(divergences.values())
        .flat_map(|run| run["paths"].as_array().unwrap())
        .map(|path| path.as_str().unwrap())
        .collect();
    let paths = audit_paths(
        &historical,
        std::str::from_utf8(&base).unwrap(),
        std::str::from_utf8(&files).unwrap(),
        &observed,
    );
    let base_files: BTreeSet<_> = std::str::from_utf8(&base)
        .unwrap()
        .split_terminator('\0')
        .collect();
    let rows = snapshot["rows"]
        .as_object()
        .expect("missing current source audit rows");
    assert_eq!(
        rows.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        paths,
        "source path set drift"
    );

    let mut counts = BTreeMap::new();
    let mut matched = 0;
    let mut checked_runs = BTreeSet::new();
    for (id, run) in runs {
        for path in run["paths"].as_array().unwrap() {
            let row = &rows[path.as_str().unwrap()];
            assert!(
                row["runs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|link| link == id),
                "missing source-to-run link: {path} / {id}"
            );
        }
    }
    let rm_divergence = "rm-proxy-independent-stream";
    check_rm_divergence(&divergences[rm_divergence]);
    for (id, run) in divergences {
        for path in run["paths"].as_array().unwrap() {
            let path = path.as_str().unwrap();
            assert_eq!(rows[path]["observed_divergence"], *id, "{path}");
            assert!(rows[path]["runs"].as_array().unwrap().is_empty(), "{path}");
            assert_eq!(rows[path]["current_sha256"], run["inputs"][path], "{path}");
        }
    }
    for (path, row) in rows {
        assert!(canonical(path), "{path}");
        assert_eq!(row["acceptance_task"], 19, "{path}");
        assert_eq!(
            row["historical_sha256"].as_str(),
            historical.get(path.as_str()).map(|v| v.0)
        );
        let current = match fs::read(root.join(path)) {
            Ok(bytes) => Some(digest(&bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => panic!("{path}: {err}"),
        };
        // The old plan and prompt had local deletions at audit time. A clean
        // checkout can retain the recorded Git versions of these two files.
        if row["change"] == "retired" {
            assert!(
                current.is_none() || current.as_deref() == row["git_sha256"].as_str(),
                "unexpected bytes at retired path {path}"
            );
        } else {
            check_input(&row["current_sha256"], current.as_deref())
                .unwrap_or_else(|err| panic!("{err}: {path}"));
        }
        let successor = row["successor"].as_str().unwrap();
        assert!(
            canonical(successor) && root.join(successor).is_file(),
            "{path}: {successor}"
        );
        let change = if row["change"] == "retired" {
            assert!(
                historical.contains_key(path.as_str()) && row["current_sha256"].is_null(),
                "{path}"
            );
            let git_bytes = read_command(&root, "git", &["show", &format!("{head}:{path}")]);
            assert_eq!(row["git_sha256"], digest(&git_bytes), "{path}");
            assert_ne!(path, successor);
            "retired"
        } else {
            assert!(current.is_some(), "missing source input: {path}");
            match row["historical_sha256"].as_str() {
                None if base_files.contains(path.as_str()) => "scope-added",
                None => "added",
                hash if hash == current.as_deref() => "same",
                Some(_) => "changed",
            }
        };
        assert_eq!(row["change"], change, "{path}");
        *counts.entry(change).or_insert(0usize) += 1;
        assert!(
            row["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty()),
            "{path}"
        );
        for claim in row["runs"].as_array().unwrap() {
            let id = claim.as_str().unwrap();
            let receipt = &snapshot["runs"][id];
            check_run(receipt, path, current.as_deref())
                .unwrap_or_else(|err| panic!("{err}: {path} / {id}"));
            if checked_runs.insert(id) {
                for (input, expected) in receipt["inputs"].as_object().unwrap() {
                    assert!(canonical(input), "{input}");
                    let bytes = fs::read(root.join(input)).unwrap();
                    check_input(expected, Some(&digest(&bytes)))
                        .unwrap_or_else(|err| panic!("{err}: {input} / {id}"));
                }
            }
            matched += 1;
        }
    }
    assert!(
        matched > 0,
        "no live business receipts attached to current rows"
    );
    assert_eq!(index["counts"], serde_json::json!(counts));
    println!(
        "{} historical, {} current, {} retired, {matched} source-to-run links",
        historical.len(),
        rows.len() - counts["retired"],
        counts["retired"]
    );
}

#[test]
fn sa_019_inventory_negative_controls() {
    use serde_json::json;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let doc = fs::read_to_string(root.join(INVENTORY)).unwrap();
    let rows = manifest(&doc).unwrap();
    let path = "pkg/server/grpc_gc_legacy.rs";
    let line = doc
        .lines()
        .find(|line| {
            line.starts_with("| [`pkg/server/grpc.rs`](../../../pkg/server/grpc.rs) |")
                && line.ends_with("` |")
        })
        .unwrap();
    let missing = doc.replacen(&format!("{line}\n"), "", 1);
    assert_eq!(
        check_historical_set(&manifest(&missing).unwrap(), &historical_digest(&rows)),
        Err("historical source set drift")
    );
    let duplicated = doc.replacen(line, &format!("{line}\n{line}"), 1);
    assert_eq!(manifest(&duplicated), Err("duplicate source row"));
    assert_eq!(
        manifest(&doc.replacen(line, &line.replace("../../../pkg/", "../../../../pkg/"), 1)),
        Err("invalid manifest fields")
    );

    let data: Value =
        serde_json::from_slice(&read_command(&root, "gzip", &["-dc", SNAPSHOT])).unwrap();
    let id = data["rows"][path]["runs"][0]
        .as_str()
        .expect("real GC source receipt");
    let mut receipt = data["runs"][id].clone();
    let hash = digest(&fs::read(root.join(path)).unwrap());
    assert!(check_run(&receipt, path, Some(&hash)).is_ok());
    assert_eq!(
        check_run(&receipt, path, Some(rows["pkg/server/grpc.rs"].0)),
        Err("source input drift")
    );
    receipt["kind"] = json!("metric-only");
    assert_eq!(
        check_run(&receipt, path, Some(&hash)),
        Err("missing business evidence")
    );
}
