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
