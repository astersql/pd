// Copyright 2026 AsterSQL.

use serde_json::{Value, json};

/// Updates a top-level field in a JSON field model, returning `(updated, found)`.
///
/// Like reflectutil's field model, objects describe existing struct fields.
/// Unknown fields are ignored, including inside nested objects; partial object
/// updates retain fields that were not supplied. Values are already JSON, so
/// Go's serialization errors cannot occur at this boundary. This API does not
/// perform Go reflection or validate Go scalar field types.
pub fn add_key_value(old: &mut Value, key: &str, value: Value) -> (bool, bool) {
    let found = astersql_pd_reflectutil::find_same_field_by_json(old, &json!({key: null}));
    let Some(field) = old.as_object_mut().and_then(|fields| fields.get_mut(key)) else {
        return (false, false);
    };
    let before = field.clone();
    update_field(field, value);
    (*field != before, found)
}

/// Merges JSON bytes into existing fields, returning `(updated, found)`.
///
/// Objects use reflectutil's serialized field model: unknown fields are ignored
/// and nested object updates preserve omitted fields. JSON null is a no-op.
/// Unlike Go's ignored marshal errors, serialization failures are returned.
/// Parsing, serialization and typed deserialization errors leave `old` intact;
/// Go's unmarshal may partially update a struct on a type error. Scalar type
/// validation follows the destination's Serde implementation (Value is untyped).
pub fn merge_json_object<T>(old: &mut T, data: &[u8]) -> Result<(bool, bool), serde_json::Error>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let before = serde_json::to_value(&*old)?;
    if !before.is_object() {
        return Err(<serde_json::Error as serde::de::Error>::custom(
            "merge destination must serialize as a JSON object",
        ));
    }
    let input: Value = serde_json::from_slice(data)?;
    if input.is_null() {
        return Ok((false, false));
    }
    if !input.is_object() {
        return Err(<serde_json::Error as serde::de::Error>::custom(
            "merge input must be a JSON object",
        ));
    }
    let found = astersql_pd_reflectutil::find_same_field_by_json(&before, &input);
    let mut merged = before.clone();
    update_field(&mut merged, input);
    let candidate: T = serde_json::from_value(merged)?;
    let after = serde_json::to_value(&candidate)?;
    let updated = before != after;
    *old = candidate;
    Ok((updated, updated || found))
}

fn update_field(field: &mut Value, value: Value) {
    if let (Some(fields), Some(updates)) = (field.as_object_mut(), value.as_object()) {
        for (key, value) in updates {
            if let Some(field) = fields.get_mut(key) {
                update_field(field, value.clone());
            }
        }
    } else {
        *field = value;
    }
}

#[cfg(test)]
mod jsonutil_test;
