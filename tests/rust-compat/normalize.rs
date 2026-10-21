// Copyright 2026 AsterSQL.
use serde_json::Value;

/// Replace only explicitly observed ephemeral values. IDs retain equality relations;
/// unknown fields, timestamps and array ordering remain semantic unless registered.
#[derive(Default)]
pub struct Rules(pub Vec<(Value, Value)>);
impl Rules {
    pub fn apply(&self, value: &Value) -> Value {
        fn visit(rules: &Rules, value: &Value, field: &str) -> Value {
            if matches!(
                field,
                "id" | "member_id" | "cluster_id" | "client_urls" | "peer_urls"
            ) {
                if let Some((_, replacement)) = rules.0.iter().find(|(raw, _)| raw == value) {
                    return replacement.clone();
                }
            }
            match value {
                Value::Array(items) => {
                    Value::Array(items.iter().map(|v| visit(rules, v, field)).collect())
                }
                Value::Object(items) => Value::Object(
                    items
                        .iter()
                        .map(|(k, v)| (k.clone(), visit(rules, v, k)))
                        .collect(),
                ),
                _ => value.clone(),
            }
        }
        visit(self, value, "")
    }
}

/// JSON objects are unordered; arrays and all unregistered scalar values are exact.
pub fn differences(left: &Value, right: &Value) -> Vec<String> {
    fn visit(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
                for key in keys {
                    match (a.get(key), b.get(key)) {
                        (Some(a), Some(b)) => visit(a, b, &format!("{path}/{key}"), out),
                        (a, b) => out.push(format!("{path}/{key}: {a:?} != {b:?}")),
                    }
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    visit(a, b, &format!("{path}/{i}"), out);
                }
            }
            _ if a != b => out.push(format!("{path}: {a} != {b}")),
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(left, right, "$", &mut out);
    out
}

/// Metrics sample order is irrelevant. Label spelling and order are kept exact. Values, help, type and
/// timestamps remain exact; caller must explicitly normalize nondeterministic samples.
pub fn metrics(text: &str) -> Vec<String> {
    let mut lines: Vec<_> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect();
    lines.sort();
    lines
}

/// Decode TSO wire messages before comparing them; field order on the wire is not
/// semantic. An injected physical origin removes only wall-clock offset. Logical
/// time, suffix bits, count, header presence and error codes/messages remain exact.
pub fn tso(bytes: &[u8], physical_origin: i64) -> Result<Value, prost::DecodeError> {
    use prost::Message;
    let response = astersql_pd_proto::pdpb::TsoResponse::decode(bytes)?;
    Ok(serde_json::json!({
        "header": response.header.map(|h| serde_json::json!({"cluster_id":h.cluster_id,"error":h.error.map(|e|serde_json::json!({"type":e.r#type,"message":e.message}))})),
        "count": response.count,
        "timestamp":response.timestamp.map(|t|serde_json::json!({"physical": i128::from(t.physical)-i128::from(physical_origin),"logical":t.logical,"suffix_bits":t.suffix_bits}))
    }))
}

/// Canonicalize only absent protobuf defaults and explicit build/deployment metadata.
/// Unknown fields fail closed so a new semantic field cannot silently escape the MVP comparison.
pub fn minimal_member(value: &Value) -> Result<Value, String> {
    let mut member = value.as_object().ok_or("member must be an object")?.clone();
    for key in member.keys() {
        if ![
            "name",
            "member_id",
            "client_urls",
            "peer_urls",
            "leader_priority",
            "dc_location",
            "binary_version",
            "git_hash",
            "deploy_path",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unreviewed member field: {key}"));
        }
    }
    for key in ["binary_version", "git_hash", "deploy_path"] {
        member.remove(key);
    }
    member
        .entry("leader_priority")
        .or_insert(serde_json::json!(0));
    member.entry("dc_location").or_insert(serde_json::json!(""));
    Ok(Value::Object(member))
}
