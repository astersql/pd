// Copyright 2026 AsterSQL.
//! ADR-001 audit of real metadata and source inputs; no service admission implied.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path, process::Command};

fn audit(metadata: &Value, sources: &[(String, String)]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    let mut locations = BTreeSet::new();
    let mut library_sources = BTreeSet::new();
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        let manifest = package["manifest_path"].as_str().unwrap();
        if !names.insert(name) || !locations.insert(manifest) {
            return Err(format!("duplicate canonical package: {name}"));
        }
        for target in package["targets"].as_array().unwrap() {
            if target["kind"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "lib")
                && !library_sources.insert(target["src_path"].as_str().unwrap())
            {
                return Err(format!("duplicate canonical library: {name}"));
            }
        }
        let host = matches!(
            name,
            "astersql-pd"
                | "astersql-pd-runtime"
                | "astersql-pd-cmd-pd-server"
                | "astersql-pd-compat-tests"
        );
        let adapter = host
            || matches!(
                name,
                "astersql-pd-server"
                    | "astersql-pd-proto"
                    | "astersql-pd-storage-endpoint"
                    | "astersql-pd-mcs-tso-server"
            );
        for dep in package["dependencies"].as_array().unwrap() {
            if dep["kind"] == "dev" {
                continue;
            }
            let dependency = dep["name"].as_str().unwrap();
            if (!host && matches!(dependency, "astersql-pd-runtime" | "astersql-pd-server"))
                || (name != "astersql-pd-compat-tests" && dependency == "astersql-pd-compat-tests")
                || (!adapter && dependency == "astersql-pd-proto")
                || (name == "astersql-pd-core"
                    && matches!(
                        dependency,
                        "tonic" | "prost" | "axum" | "etcd-client" | "raft"
                    ))
            {
                return Err(format!("reverse/transport edge: {name} -> {dependency}"));
            }
        }
    }
    for (path, source) in sources {
        if path == "pkg/lib.rs" {
            for line in source.lines().map(str::trim) {
                if !line.is_empty() && !line.starts_with("//") && !line.starts_with("pub use ") {
                    return Err("facade repeats implementation".into());
                }
            }
        }
        // Adapter paths are explicit, never inferred from the substring "server".
        let adapter = [
            "pkg/server/",
            "pkg/mcs/tso/server/",
            "pkg/runtime/",
            "pkg/proto/",
            "pkg/storage/endpoint/",
            "pkg/storage/kv/",
            "cmd/",
            "tests/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix));
        if !adapter
            && (source.contains("astersql_pd_proto")
                || source.contains("prost::")
                || source.contains("tonic::"))
        {
            return Err(format!("domain protocol reference: {path}"));
        }
        if !path.starts_with("cmd/") && !path.starts_with("tests/") {
            for forbidden in [
                "process::exit(",
                "Runtime::new(",
                "Builder::new_multi_thread(",
                "Builder::new_current_thread(",
                "signal::ctrl_c(",
                "set_global_default(",
            ] {
                if source.contains(forbidden) {
                    return Err(format!("library lifecycle ownership: {path}: {forbidden}"));
                }
            }
        }
    }
    Ok(())
}

fn collect(root: &Path, dir: &Path, sources: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(root, &path, sources);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let name = path.file_name().unwrap().to_str().unwrap();
            if name.ends_with("_test.rs") || name == "build.rs" {
                continue;
            }
            sources.push((
                path.strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
                std::fs::read_to_string(path).unwrap(),
            ));
        }
    }
}

#[test]
fn sa_002_dependency_direction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let target = std::env::var("CARGO_TARGET_DIR").expect("claimed Cargo slot required");
    assert!(Path::new(&target).is_absolute());
    let output = Command::new("cargo")
        .current_dir(&root)
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut sources = Vec::new();
    for dir in ["pkg", "server", "cmd"] {
        collect(&root, &root.join(dir), &mut sources);
    }
    sources.sort();
    assert!(sources.len() > 100);
    audit(&metadata, &sources).unwrap();
    // Each mutation starts from this round's real graph/source snapshot.
    let mut reverse = metadata.clone();
    let core = reverse["packages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["name"] == "astersql-pd-core")
        .unwrap();
    core["dependencies"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"astersql-pd-runtime","kind":null}));
    assert!(
        audit(&reverse, &sources)
            .unwrap_err()
            .contains("reverse/transport edge")
    );
    let mut duplicate = metadata.clone();
    let package = duplicate["packages"][0].clone();
    duplicate["packages"].as_array_mut().unwrap().push(package);
    assert!(
        audit(&duplicate, &sources)
            .unwrap_err()
            .contains("duplicate canonical")
    );
    let mut leaked = sources.clone();
    leaked
        .iter_mut()
        .find(|(p, _)| p == "pkg/storage/domain/lib.rs")
        .unwrap()
        .1
        .push_str("\npub use astersql_pd_proto::pdpb::Region;\n");
    assert!(
        audit(&metadata, &leaked)
            .unwrap_err()
            .contains("domain protocol reference")
    );
    let mut lifecycle = sources.clone();
    lifecycle
        .iter_mut()
        .find(|(p, _)| p == "pkg/core/lib.rs")
        .unwrap()
        .1
        .push_str("\nfn hidden() { tokio::runtime::Runtime::new(); }\n");
    assert!(
        audit(&metadata, &lifecycle)
            .unwrap_err()
            .contains("library lifecycle")
    );
    let mut facade = sources.clone();
    facade
        .iter_mut()
        .find(|(p, _)| p == "pkg/lib.rs")
        .unwrap()
        .1
        .push_str("\npub fn duplicate_domain() {}\n");
    assert!(
        audit(&metadata, &facade)
            .unwrap_err()
            .contains("facade repeats")
    );
    let mut disguised = sources.clone();
    disguised.push((
        "pkg/mcs/tso/domain_server/leak.rs".into(),
        "use astersql_pd_proto::tsopb;".into(),
    ));
    assert!(
        audit(&metadata, &disguised)
            .unwrap_err()
            .contains("domain protocol reference")
    );
    let hashes: Vec<_> = sources
        .iter()
        .map(|(p, s)| json!({"path":p,"sha256":format!("{:x}",Sha256::digest(s.as_bytes()))}))
        .collect();
    println!(
        "packages={} sources={} negative_controls=6 metadata_sha256={:x}",
        metadata["packages"].as_array().unwrap().len(),
        sources.len(),
        Sha256::digest(&output.stdout)
    );
    if let Some(path) = std::env::var_os("SA_002_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"metadata":metadata,"sources":hashes,"negative_controls":6}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
