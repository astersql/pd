// Copyright 2026 AsterSQL.
//! Real-process evidence for task 102 repairs; this does not grant workspace acceptance.
use crate::harness::Server;
use serde_json::{Value, json};
use std::{path::Path, process::Command};

pub(crate) fn maintenance(binary: &Path, rust: bool) -> Vec<Value> {
    let mut server = if rust {
        Server::start_rust(binary)
    } else {
        Server::start(binary)
    }
    .unwrap();
    server.ready().unwrap();
    let mut raw = Vec::new();
    let mut result = Vec::new();
    let mut timestamp = None;
    for (method, suffix, status) in [
        ("GET", "", 404),
        ("GET", "/upgrade", 404),
        ("PUT", "/upgrade/one", 200),
        ("GET", "", 200),
        ("GET", "/upgrade", 200),
        ("PUT", "/upgrade/one", 409),
        ("PUT", "/other/two", 409),
        ("DELETE", "/upgrade/wrong", 409),
        ("DELETE", "/missing/one", 404),
        ("DELETE", "/upgrade/one", 200),
        ("GET", "", 404),
    ] {
        let output = Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                method,
                "--data-binary",
                "维护 <>&",
                "-w",
                "\n%{http_code}\n%{content_type}",
                &format!("{}/pd/api/v2/maintenance{suffix}", server.url),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let mut parts = text.rsplitn(3, '\n');
        let content_type = parts.next().unwrap();
        let actual_status: u16 = parts.next().unwrap().parse().unwrap();
        let body = parts.next().unwrap();
        raw.push(json!({"method":method,"path":suffix,"status":actual_status,"content_type":content_type,"body":body}));
        let path =
            std::path::PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap()).join(format!(
                "task102-maintenance-{}-{}.json",
                if rust { "rust" } else { "go" },
                std::process::id()
            ));
        std::fs::write(path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
        assert_eq!(actual_status, status, "{method} {suffix}: {body}");
        let mut value: Value = serde_json::from_str(body).unwrap();
        normalize_timestamp(&mut value, &mut timestamp);
        result.push(json!({"method":method,"path":suffix,"status":actual_status,"content_type":content_type,"body":value}));
    }
    server.stop().unwrap();
    result
}

fn normalize_timestamp(value: &mut Value, timestamp: &mut Option<i64>) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_timestamp(item, timestamp);
            }
        }
        Value::Object(fields) => {
            if let Some(value) = fields.get_mut("start_timestamp") {
                let current = value.as_i64().unwrap();
                assert!(current > 0);
                assert_eq!(
                    *timestamp.get_or_insert(current),
                    current,
                    "all queries and conflicts must observe the same persisted task"
                );
                *value = json!(0);
            }
            for value in fields.values_mut() {
                normalize_timestamp(value, timestamp);
            }
        }
        _ => {}
    }
}
