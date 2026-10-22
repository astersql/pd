// Copyright 2026 AsterSQL.

use serde_json::Value;

/// Finds the first field named `tag` and returns its dot-separated JSON path.
///
/// Objects model the nested structs serialized by Serde. Non-object values are
/// leaf fields and are not traversed.
pub fn find_json_full_tag_by_child_tag(fields: &Value, tag: &str) -> String {
    find_path(fields, tag).unwrap_or_default()
}

/// Reports whether an input object names any top-level JSON field.
///
/// This mirrors Go's `FindSameFieldByJSON`: only field names are compared, so
/// the input values and the described field values do not need to be equal.
pub fn find_same_field_by_json(fields: &Value, input: &Value) -> bool {
    let (Some(fields), Some(input)) = (fields.as_object(), input.as_object()) else {
        return false;
    };

    input.keys().any(|field| fields.contains_key(field))
}

/// Finds the field type descriptor at a JSON tag path.
///
/// Each path component must name a field in an object. An empty path returns
/// the current descriptor, while an unknown field or a non-object
/// intermediate descriptor returns `None`.
pub fn find_field_by_json_tag<'a>(fields: &'a Value, tags: &[&str]) -> Option<&'a Value> {
    let mut field = fields;
    for tag in tags {
        field = field.as_object()?.get(*tag)?;
    }
    Some(field)
}

fn find_path(fields: &Value, tag: &str) -> Option<String> {
    let object = fields.as_object()?;
    for (field, value) in object {
        if field == tag {
            return Some(field.clone());
        }

        if let Some(path) = find_path(value, tag) {
            return Some(format!("{field}.{path}"));
        }
    }
    None
}

#[cfg(test)]
mod tag_test;
