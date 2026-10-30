// Copyright 2026 AsterSQL.
//! Bulk placement routes use the same host queue and rule manager as single rules.
use crate::PdServices;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
pub(crate) fn router(services: Arc<dyn PdServices>, cancel: CancellationToken) -> Router {
    Router::new()
        .route(
            "/pd/api/v1/config/rules",
            get(crate::http_rule::handle).post(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/rules/batch",
            post(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/rules/region/{region}/detail",
            get(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/rule_group",
            post(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/rule_group/{id}",
            get(crate::http_rule::handle).delete(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/rule_groups",
            get(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/placement-rule",
            get(crate::http_rule::handle).post(crate::http_rule::handle),
        )
        .route(
            "/pd/api/v1/config/placement-rule/{group}",
            get(crate::http_rule::handle)
                .post(crate::http_rule::handle)
                .delete(crate::http_rule::handle),
        )
        .with_state((services, cancel))
}

/// Decode bulk inputs with Go's field types and byte offsets before any mutation.
pub fn parse(bytes: &[u8], schema: &str) -> Result<serde_json::Value, serde_json::Value> {
    use serde_json::{Value, json};
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return Err(crate::http_rule::parse(bytes).unwrap_err()),
    };
    fn validate(
        bytes: &[u8],
        base: usize,
        value: &Value,
        typ: &str,
        structure: &str,
        field: &str,
    ) -> Result<(), Value> {
        if value.is_null() {
            return Ok(());
        }
        let valid = match typ {
            "string"
            | "placement.PeerRoleType"
            | "placement.RuleOpType"
            | "placement.LabelConstraintOp" => value.is_string(),
            "bool" => value.is_boolean(),
            "int" => value.as_i64().is_some(),
            "uint64" => value.as_u64().is_some(),
            t if t.starts_with("[]") => value.is_array(),
            _ => value.is_object(),
        };
        if !valid {
            let mut kind = crate::schedule_admin_balance_leader::json_type(value).to_owned();
            if value.is_number() && matches!(typ, "int" | "uint64") {
                kind = format!("number {value}");
            }
            let start = bytes
                .iter()
                .position(|c| !c.is_ascii_whitespace())
                .unwrap_or(0);
            let end = bytes
                .iter()
                .rposition(|c| !c.is_ascii_whitespace())
                .map_or(0, |i| i + 1);
            let offset = base
                + if value.is_array() || value.is_object() {
                    start + 1
                } else {
                    end
                };
            let msg = if field.is_empty() {
                format!("json: cannot unmarshal {kind} into Go value of type {typ}")
            } else {
                format!(
                    "json: cannot unmarshal {kind} into Go struct field {structure}.{field} of type {typ}"
                )
            };
            return Err(
                json!({"code":"input","msg":msg,"data":{"Value":kind,"Type":{},"Offset":offset,"Struct":structure,"Field":field}}),
            );
        }
        if let Some(element_type) = typ.strip_prefix("[]") {
            let element_type = element_type.trim_start_matches('*');
            let mut pos = bytes.iter().position(|c| *c == b'[').unwrap() + 1;
            for element in value.as_array().unwrap() {
                while bytes[pos].is_ascii_whitespace() || bytes[pos] == b',' {
                    pos += 1;
                }
                let mut stream =
                    serde_json::Deserializer::from_slice(&bytes[pos..]).into_iter::<Value>();
                stream.next().unwrap().unwrap();
                let end = pos + stream.byte_offset();
                validate(
                    &bytes[pos..end],
                    base + pos,
                    element,
                    element_type,
                    structure,
                    field,
                )?;
                pos = end;
            }
        } else if value.is_object() {
            let current = typ.trim_start_matches("placement.");
            // Keep duplicate fields in input order: a later null does not clear a scalar.
            let mut ordered = Vec::new();
            let mut cursor = bytes.iter().position(|c| *c == b'{').unwrap() + 1;
            loop {
                while bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b',' {
                    cursor += 1;
                }
                if bytes[cursor] == b'}' {
                    break;
                }
                let mut stream =
                    serde_json::Deserializer::from_slice(&bytes[cursor..]).into_iter::<String>();
                let key = stream.next().unwrap().unwrap();
                cursor += stream.byte_offset();
                while bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b':' {
                    cursor += 1;
                }
                let start = cursor;
                let mut stream =
                    serde_json::Deserializer::from_slice(&bytes[cursor..]).into_iter::<Value>();
                let field_value = stream.next().unwrap().unwrap();
                cursor += stream.byte_offset();
                ordered.push((start, cursor, key, field_value));
            }
            for (a, b, key, v) in ordered {
                let key = key.to_lowercase();
                let expected = match (current, key.as_str()) {
                    ("RuleGroup", "id") => "string",
                    ("RuleGroup", "index") => "int",
                    ("RuleGroup", "override") => "bool",
                    ("GroupBundle", "group_id") => "string",
                    ("GroupBundle", "group_index") => "int",
                    ("GroupBundle", "group_override") => "bool",
                    ("GroupBundle", "rules") => "[]*placement.Rule",
                    (
                        "Rule" | "RuleOp",
                        "group_id" | "id" | "start_key" | "end_key" | "isolation_level",
                    ) => "string",
                    ("Rule" | "RuleOp", "role") => "placement.PeerRoleType",
                    ("Rule" | "RuleOp", "count" | "index") => "int",
                    ("Rule" | "RuleOp", "override" | "is_witness") => "bool",
                    ("Rule" | "RuleOp", "version" | "create_timestamp") => "uint64",
                    ("Rule" | "RuleOp", "location_labels") => "[]string",
                    ("Rule" | "RuleOp", "label_constraints") => "[]placement.LabelConstraint",
                    ("RuleOp", "action") => "placement.RuleOpType",
                    ("RuleOp", "delete_by_id_prefix") => "bool",
                    ("LabelConstraint", "key") => "string",
                    ("LabelConstraint", "op") => "placement.LabelConstraintOp",
                    ("LabelConstraint", "values") => "[]string",
                    _ => continue,
                };
                let key = if current == "RuleOp" && key != "action" && key != "delete_by_id_prefix"
                {
                    format!("Rule.{key}")
                } else {
                    key
                };
                let nested = if field.is_empty() {
                    key
                } else {
                    format!("{field}.{key}")
                };
                validate(&bytes[a..b], base + a, &v, expected, current, &nested)?;
            }
        }
        Ok(())
    }
    validate(bytes, 0, &value, schema, "", "")?;
    Ok(value)
}

/// Compile bundle group selectors with the existing Go-compatible HTTP regexp parser.
pub fn group_regexp(pattern: &str) -> Result<regex::Regex, String> {
    crate::http_store_read::go_pattern(pattern)
}

/// Operation metadata decoded in original field order, like encoding/json's embedded Rule.
#[derive(Default)]
pub struct OperationFields {
    pub action: String,
    pub prefix: bool,
}
impl<'de> serde::Deserialize<'de> for OperationFields {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = OperationFields;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("operation object or null")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(OperationFields::default())
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut fields = OperationFields::default();
                while let Some(key) = map.next_key::<String>()? {
                    match key.to_ascii_lowercase().as_str() {
                        "action" => {
                            if let Some(v) = map.next_value::<Option<String>>()? {
                                fields.action = v;
                            }
                        }
                        "delete_by_id_prefix" => {
                            if let Some(v) = map.next_value::<Option<bool>>()? {
                                fields.prefix = v;
                            }
                        }
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(fields)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
