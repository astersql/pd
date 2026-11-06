// Copyright 2026 AsterSQL.
//! Restore Go's JSON overlay into the host's already-adjusted controller values.
use crate::ControllerConfig;
use serde_json::{Map, Value};
fn set(root: &mut Value, path: &[String], value: Value) {
    if let Some((first, rest)) = path.split_first() {
        if !root.is_object() {
            *root = Value::Object(Map::new());
        }
        set(
            root.as_object_mut()
                .unwrap()
                .entry(first)
                .or_insert(Value::Null),
            rest,
            value,
        );
    } else {
        *root = value;
    }
}
fn at<'a>(root: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(root, |value, key| value.get(key))
}
fn apply(root: &mut Value, path: &mut Vec<String>, patch: Value) {
    if let Value::Object(fields) = patch {
        if at(root, path).is_none_or(|v| !v.is_object()) {
            let mut candidate = root.clone();
            set(&mut candidate, path, Value::Object(Map::new()));
            if serde_json::from_value::<ControllerConfig>(candidate.clone()).is_ok() {
                *root = candidate;
            }
        }
        for (key, value) in fields {
            path.push(key);
            apply(root, path, value);
            path.pop();
        }
        return;
    }
    // Null does not change Go scalars or value structs; it clears pointer/map fields.
    if patch.is_null()
        && !matches!(
            path.last().map(String::as_str),
            Some("ru-version-policy" | "overrides")
        )
    {
        return;
    }
    let mut candidate = root.clone();
    set(&mut candidate, path, patch);
    // encoding/json continues decoding other fields after an incompatible field type.
    if serde_json::from_value::<ControllerConfig>(candidate.clone()).is_ok() {
        *root = candidate;
    }
}
pub(crate) fn restore(
    initial: &ControllerConfig,
    bytes: &[u8],
) -> Result<Option<ControllerConfig>, String> {
    let Ok(patch) = serde_json::from_slice::<Value>(bytes) else {
        return Ok(Some(initial.clone()));
    };
    if patch.is_null() {
        return Ok(None);
    }
    if !patch.is_object() {
        return Ok(Some(initial.clone()));
    }
    let mut value = serde_json::to_value(initial).map_err(|e| e.to_string())?;
    apply(&mut value, &mut Vec::new(), patch);
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| e.to_string())
}
