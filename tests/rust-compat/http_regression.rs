// Copyright 2026 AsterSQL.
//! Exhaustive inventory accounting and real read-only differential entrypoints.
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

pub(crate) fn report_path(default: impl AsRef<Path>) -> std::path::PathBuf {
    let default = default.as_ref();
    match std::env::var_os("PD_COMPAT_HTTP_REPORT_DIR") {
        Some(dir) => Path::new(&dir).join(default.file_name().unwrap()),
        None => default.to_owned(),
    }
}

pub(crate) fn catalog() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/task-101-catalog.json")).expect("reviewed catalog")
}
pub(crate) fn inventory() -> BTreeMap<String, String> {
    let mut rows = BTreeMap::new();
    for text in [
        include_str!("../../docs/rust/server-api/wire-inventory.md"),
        include_str!("../../docs/rust/server-api/surface-inventory.md"),
    ] {
        for line in text.lines().filter(|line| line.starts_with("| ")) {
            let key = if let Some(start) = line.find("HTTP-") {
                Some(&line[start..start + 8])
            } else {
                line.split_whitespace().nth(1).filter(|key| {
                    ["C-", "M-", "F-"]
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
                })
            };
            if let Some(key) = key {
                assert!(
                    rows.insert(key.into(), line.into()).is_none(),
                    "duplicate matrix ID {key}"
                );
            }
        }
    }
    rows
}
// These route IDs were admitted by the named domain matrices. The old
// boundary replay remains independent coverage for malformed and absent routes.
fn domain_replay(key: &str) -> Option<&'static str> {
    if key == "M-037" {
        return Some("domain-maintenance");
    }
    match key.strip_prefix("HTTP-")?.parse::<u16>().ok()? {
        161 | 162 | 197 => Some("domain-gc-http"),
        179..=185 => Some("domain-keyspace"),
        186..=189 => Some("domain-maintenance"),
        190..=192 => Some("domain-meta-service-group"),
        193..=195 => Some("domain-mcs-discovery"),
        198..=204 => Some("domain-tso-directory"),
        205..=209 => Some("domain-tso-transition"),
        _ => None,
    }
}

pub(crate) fn validate(rows: &BTreeMap<String, String>, catalog: &[Value]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for entry in catalog {
        let key = entry["id"].as_str().ok_or("missing ID")?;
        if !seen.insert(key) {
            return Err(format!("duplicate ID {key}"));
        }
        let row = rows
            .get(key)
            .ok_or_else(|| format!("unregistered ID {key}"))?;
        if entry["row"].as_str() != Some(row) {
            return Err(format!("changed source contract {key}"));
        }
        let columns: Vec<_> = row.split('|').map(str::trim).collect();
        let owner = if key.starts_with("HTTP-") {
            columns[6]
        } else {
            columns[columns.len() - 2]
        };
        if owner.is_empty() || entry["owner"].as_str() != Some(owner) {
            return Err(format!("missing or changed owner {key}"));
        }
        let expected_scope = match key {
            "M-035" => {
                serde_json::json!([{"label":"type","values":["keyspace-idAlloc"],"dependency":"theme-8: keyspace allocator lifecycle"}])
            }
            "M-078" | "M-079" | "M-080" | "M-081" | "M-082" => {
                serde_json::json!([{"label":"runner_name","values":["log-async"],"dependency":"theme-9: asynchronous log runner; Rust logging currently uses its existing logging sink"}])
            }
            _ => Value::Null,
        };
        if entry["label_dependencies"] != expected_scope {
            return Err(format!("unreviewed partial label scope {key}"));
        }
        let expected_dynamic = if key == "M-154" {
            serde_json::json!({"type":"absolute backend endpoint URL; independent fixture addresses remain raw; literal endpoint is compared"})
        } else {
            Value::Null
        };
        if entry["dynamic_label_values"] != expected_dynamic {
            return Err(format!("unreviewed dynamic label scope {key}"));
        }
        let replays = entry["replays"].as_array().ok_or("missing replay list")?;
        let dependency = entry["dependency"].as_str().ok_or("missing disposition")?;
        if replays.is_empty() && dependency.is_empty() {
            return Err(format!("no witness or explicit dependency {key}"));
        }
        let mut witnesses = BTreeSet::new();
        for replay in replays {
            let name = replay.as_str().ok_or("invalid witness")?;
            if !witnesses.insert(name) {
                return Err(format!("duplicate witness {key}: {name}"));
            }
            if name.starts_with("domain-") {
                if Some(name) != domain_replay(key) {
                    return Err(format!("unexpected domain replay {key}: {name}"));
                }
                continue;
            }
            if name.starts_with('@')
                && !["@core", "@cluster-metrics", "@shared-metrics"].contains(&name)
            {
                return Err(format!("unknown witness {name}"));
            }
            if !name.starts_with('@')
                && !Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../scripts/{name}-oracle.py"))
                    .is_file()
            {
                return Err(format!("missing supervisor {name}"));
            }
        }
        if let Some(expected) = domain_replay(key)
            && (!witnesses.contains(expected) || !dependency.is_empty())
        {
            return Err(format!("missing domain replay {key}: {expected}"));
        }
    }
    if seen != rows.keys().map(String::as_str).collect() {
        return Err("matrix contains an unknown/unowned row".into());
    }
    Ok(())
}
pub(crate) fn replay_cases(rows: &[Value]) -> BTreeSet<String> {
    let mut scripts: BTreeSet<String> = rows
        .iter()
        .flat_map(|row| row["replays"].as_array().unwrap().iter())
        .map(|name| name.as_str().unwrap().into())
        .filter(|name: &String| !name.starts_with('@'))
        .collect();
    // Public middleware is exercised in addition to each route owner's trace.
    scripts.extend(
        [
            "http-contract",
            "http-v2-boundary",
            "request-context",
            "http-redirect",
            "service-audit",
            "service-auth",
            "tls-listener",
            "mtls-listener",
            "tls-reload",
            "shared-listener",
            "service-metrics",
        ]
        .map(String::from),
    );
    // Include the real health worker's full-duration trace before comparing
    // timer-driven metric families across independently timed business cases.
    scripts.insert("cluster-metrics".into());
    scripts
}

pub(crate) fn validate_replay_reports(
    expected: &BTreeSet<String>,
    reports: &[Value],
) -> Result<(), String> {
    let mut actual = BTreeSet::new();
    let mut failures = Vec::new();
    for report in reports {
        let case = report["case"].as_str().ok_or("missing report case")?;
        if !actual.insert(case.to_owned()) {
            return Err(format!("duplicate report case {case}"));
        }
        if report["exit"].as_i64() != Some(0) {
            failures.push(case);
        }
    }
    if &actual != expected || actual.is_empty() {
        return Err(format!(
            "missing report cases {:?}; unexpected report cases {:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual.difference(expected).collect::<Vec<_>>(),
        ));
    }
    if !failures.is_empty() {
        return Err(format!("failed business reports: {failures:?}"));
    }
    Ok(())
}

pub(crate) fn run(root: &Path) {
    let rows = catalog();
    validate(&inventory(), &rows).unwrap();
    let etcd=std::path::PathBuf::from(std::env::var_os("PD_ETCD_BIN").expect("set PD_ETCD_BIN to the existing etcd binary; member/leader oracles require adjacent etcdctl"));
    assert!(
        etcd.is_absolute() && etcd.is_file() && etcd.with_file_name("etcdctl").is_file(),
        "existing etcd and adjacent etcdctl are required before starting the matrix"
    );
    let target = std::env::var_os("CARGO_TARGET_DIR").expect("claim a Cargo slot");
    let rust = std::env::var_os("PD_COMPAT_RUST_BIN")
        .unwrap_or_else(|| Path::new(&target).join("debug/pd-server").into_os_string());
    let go = crate::readonly_oracle::pinned_go_binary().expect("pinned Go PD binary");
    let scripts = replay_cases(&rows);
    let mut report_paths = Vec::new();
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for name in &scripts {
        let output = Command::new("python3")
            .arg(root.join("scripts/http-regression-oracle.py"))
            .args(["--case", name])
            .current_dir(root)
            .env("PD_COMPAT_MODE", "dual")
            .env("PD_COMPAT_GO_BIN", &go)
            .env("PD_COMPAT_RUST_BIN", &rust)
            .env(
                "PD_CLI_EXIT_REPORT",
                Path::new(&target).join(format!(
                    "http-regression-cli-exit-{}.json",
                    std::process::id()
                )),
            )
            .env(
                "PD_CLI_LOGGING_REPORT",
                Path::new(&target).join(format!(
                    "http-regression-cli-logging-{}.json",
                    std::process::id()
                )),
            )
            .output()
            .expect("read-only supervisor");
        let log =
            Path::new(&target).join(format!("http-regression-{name}-{}.log", std::process::id()));
        let mut bytes = output.stdout.clone();
        bytes.extend_from_slice(&output.stderr);
        std::fs::write(&log, bytes).unwrap();
        println!(
            "task101 supervisor {name}: {}; log={}",
            output.status,
            log.display()
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let report = stdout
            .lines()
            .find_map(|line| line.strip_prefix("HTTP_METRICS_REPORT="))
            .ok_or_else(|| format!("missing observation report: {}", log.display()))
            .and_then(|path| {
                let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
                let report: Value =
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                if report["case"].as_str() != Some(name.as_str()) {
                    return Err(format!("wrong case in report {path}"));
                }
                report_paths.push(path.to_owned());
                Ok(report)
            });
        match report {
            Ok(report) => reports.push(report),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
        if !output.status.success() {
            failures.push(format!("{name}: {}; log={}", output.status, log.display()));
        }
    }
    if let Err(error) = validate_replay_reports(&scripts, &reports) {
        failures.push(error);
    }
    if let Err(error) = validate_live_metrics(&rows, &reports) {
        failures.push(error);
    }
    assert!(
        failures.is_empty(),
        "HTTP/metrics regression failures:\n{}",
        failures.join("\n")
    );
    let index = Path::new(&target).join(format!(
        "http-regression-reports-{}.json",
        std::process::id()
    ));
    std::fs::write(&index, serde_json::to_vec(&report_paths).unwrap()).unwrap();
    println!("HTTP_REGRESSION_REPORTS={}", index.display());
}

/// Live values remain unnormalized in the reports. Compare schemas only for
/// families actually reached by the same business supervisors; timer values,
/// dynamic endpoint/store labels and process-local counters are not equalities.
pub(crate) fn validate_live_metrics(rows: &[Value], reports: &[Value]) -> Result<(), String> {
    let mut live: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut samples: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for report in reports {
        for observation in report["observations"]
            .as_array()
            .ok_or("missing observations")?
        {
            for snapshot in observation["schemas"].as_array().ok_or("missing schemas")? {
                let kind = snapshot["implementation"]
                    .as_str()
                    .ok_or("missing implementation")?;
                let text = snapshot["exposition"]
                    .as_str()
                    .ok_or("missing exposition")?;
                for (family, schema) in snapshot["sample_schemas"]
                    .as_object()
                    .ok_or("missing live sample schemas")?
                {
                    samples
                        .entry((kind.into(), family.clone()))
                        .or_default()
                        .insert(serde_json::to_string(schema).unwrap());
                }
                for line in text
                    .lines()
                    .filter(|line| line.starts_with("# HELP ") || line.starts_with("# TYPE "))
                {
                    let family = line.split_whitespace().nth(2).ok_or("bad exposition")?;
                    live.entry((kind.into(), family.into()))
                        .or_default()
                        .insert(line.into());
                }
            }
        }
    }
    let mut reached = 0;
    for row in rows
        .iter()
        .filter(|row| row.get("metric_contract").is_some())
    {
        let contract = &row["metric_contract"];
        let family = contract["name"].as_str().unwrap();
        let expected: BTreeSet<String> = [
            format!("# HELP {family} {}", contract["help"].as_str().unwrap()),
            format!("# TYPE {family} {}", contract["kind"].as_str().unwrap()),
        ]
        .into_iter()
        .collect();
        if let Some(go) = live.get(&("GO".into(), family.into())) {
            reached += 1;
            if go != &expected {
                return Err(format!("{} Go contract changed: {go:?}", row["id"]));
            }
            let rust = live
                .get(&("RUST".into(), family.into()))
                .ok_or_else(|| format!("{} missing live Rust {family}", row["id"]))?;
            let go_samples = samples
                .get(&("GO".into(), family.into()))
                .ok_or("missing Go samples")?;
            let rust_samples = samples
                .get(&("RUST".into(), family.into()))
                .ok_or("missing Rust samples")?;
            let parse_samples = |values: &BTreeSet<String>| -> Result<
                (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>),
                String,
            > {
                let mut schemas = BTreeSet::new();
                let mut enums: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                for text in values {
                    let mut schema: Value =
                        serde_json::from_str(text).map_err(|error| error.to_string())?;
                    let dependencies = schema
                        .as_object_mut()
                        .unwrap()
                        .remove("scope_dependencies")
                        .ok_or("missing scope dependency evidence")?;
                    for excluded in dependencies
                        .as_array()
                        .ok_or("bad scope dependency evidence")?
                    {
                        let allowed = row["label_dependencies"].as_array().is_some_and(|scopes| {
                            scopes.iter().any(|scope| {
                                excluded["dependency"] == scope["dependency"]
                                    && scope["values"].as_array().unwrap().contains(
                                        &excluded["labels"][scope["label"].as_str().unwrap()],
                                    )
                            })
                        });
                        if !allowed {
                            return Err(format!(
                                "{} unreviewed label exclusion {excluded}",
                                row["id"]
                            ));
                        }
                    }
                    let finite = schema
                        .as_object_mut()
                        .unwrap()
                        .remove("enum_labels")
                        .ok_or("missing finite label observations")?;
                    for (key, values) in finite.as_object().ok_or("bad finite labels")? {
                        enums.entry(key.clone()).or_default().extend(
                            values
                                .as_array()
                                .ok_or("bad finite values")?
                                .iter()
                                .map(|value| value.as_str().unwrap().to_owned()),
                        );
                    }
                    schemas.insert(serde_json::to_string(&schema).unwrap());
                }
                Ok((schemas, enums))
            };
            let (go_shapes, go_enums) = parse_samples(go_samples)?;
            let (rust_shapes, rust_enums) = parse_samples(rust_samples)?;
            if go_shapes != rust_shapes {
                return Err(format!(
                    "{} label names/bucket boundaries differ: {go_shapes:?} vs {rust_shapes:?}",
                    row["id"]
                ));
            }
            for (key, expected) in go_enums {
                if rust_enums
                    .get(&key)
                    .is_none_or(|actual| !expected.is_subset(actual))
                {
                    return Err(format!(
                        "{} missing live finite labels {key}: Go {expected:?}, Rust {:?}",
                        row["id"],
                        rust_enums.get(&key)
                    ));
                }
            }
            for schema in go_samples {
                let schema: Value = serde_json::from_str(schema).unwrap();
                if schema["labels"] != serde_json::json!([contract["labels"]]) {
                    return Err(format!(
                        "{} source label contract differs: {schema}",
                        row["id"]
                    ));
                }
            }
            if rust != go {
                return Err(format!(
                    "{} live schema differs: {rust:?} vs {go:?}",
                    row["id"]
                ));
            }
        }
    }
    if reached == 0 {
        return Err("no real shared metric observations".into());
    }
    println!("task101 observed {reached} shared metric family schemas in dual business replays");
    Ok(())
}
