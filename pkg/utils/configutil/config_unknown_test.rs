// Copyright 2026 AsterSQL.

use crate::ConfigMetaData;

#[test]
fn config_unknown_absent_and_empty_metadata() {
    assert_eq!(ConfigMetaData::default().check_undecoded(), None);
    let meta = ConfigMetaData::from_defined_paths([["known"]]);
    assert_eq!(meta.check_undecoded(), None);
    assert_eq!(
        meta.with_undecoded_keys(["ignored"])
            .with_undecoded_keys(Vec::<String>::new())
            .check_undecoded(),
        None
    );
}

#[test]
fn config_unknown_nested_paths_and_document_order() {
    let meta = ConfigMetaData::default().with_undecoded_keys([
        "z-last",
        "schedule.unknown",
        "a-first",
        "schedule.unknown",
    ]);
    let expected = Some(
        "Config contains undefined item: z-last, schedule.unknown, a-first, schedule.unknown"
            .to_owned(),
    );
    assert_eq!(meta.check_undecoded(), expected);
    assert_eq!(meta.check_undecoded(), expected);
}

#[test]
fn config_unknown_preserves_parser_display_verbatim() {
    let meta = ConfigMetaData::default().with_undecoded_keys([
        r#"table."literal.dot""#,
        r#""""#,
        r#""未知"."a\"b""#,
    ]);
    assert_eq!(
        meta.check_undecoded().as_deref(),
        Some(r#"Config contains undefined item: table."literal.dot", "", "未知"."a\"b""#)
    );
}

#[test]
fn config_unknown_child_uses_global_metadata() {
    let parent = ConfigMetaData::from_defined_paths([["schedule", "known"]])
        .with_undecoded_keys(["other.unknown", "schedule.unknown"]);
    let child = parent.child(["schedule"]);
    assert_eq!(
        child.check_undecoded().as_deref(),
        Some("Config contains undefined item: other.unknown, schedule.unknown")
    );
    assert_eq!(child.check_undecoded(), parent.check_undecoded());
    assert!(child.is_defined("known"));
    let clone = child.clone();
    drop(parent);
    drop(child);
    assert_eq!(
        clone.check_undecoded().as_deref(),
        Some("Config contains undefined item: other.unknown, schedule.unknown")
    );
}

#[test]
fn config_unknown_warning_leaves_values_and_views_usable() {
    let meta = ConfigMetaData::from_defined_paths([["enabled"]]).with_undecoded_keys(["typo"]);
    let mut warnings = Vec::new();
    if let Some(warning) = meta.check_undecoded() {
        warnings.push(warning);
    }
    assert_eq!(warnings, ["Config contains undefined item: typo"]);
    assert!(meta.is_defined("enabled"));
    let other = meta.clone().with_undecoded_keys(["replacement"]);
    assert_eq!(
        other.check_undecoded().as_deref(),
        Some("Config contains undefined item: replacement")
    );
    assert_eq!(
        meta.check_undecoded().as_deref(),
        Some("Config contains undefined item: typo")
    );
}
