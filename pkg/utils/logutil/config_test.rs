// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn log_defaults_and_presence() {
    let mut c = LogConfig::default();
    c.adjust(false);
    assert_eq!(c.level, "info");
    assert_eq!(c.format, "text");
    assert!(c.disable_error_verbose);
    assert_eq!(c.file.max_size, 0);
    c.disable_error_verbose = false;
    c.level = "warn".into();
    c.adjust(true);
    assert!(!c.disable_error_verbose);
    assert_eq!(c.level, "warn");
    let c: LogConfig = serde_json::from_str(
        r#"{"level":null,"file":null,"sampling":{"initial":3,"thereafter":9}}"#,
    )
    .unwrap();
    assert_eq!(c.level, "");
    assert_eq!(c.sampling.unwrap().initial, 3);
}
#[test]
fn redact_accepted_types_and_errors() {
    for (text, expected) in [
        ("false", RedactInfoLog::Off),
        ("true", RedactInfoLog::On),
        ("null", RedactInfoLog::Off),
        (r#""marker""#, RedactInfoLog::Marker),
    ] {
        assert_eq!(
            serde_json::from_str::<RedactInfoLog>(text).unwrap(),
            expected
        );
    }
    for text in ["0", r#""true""#, r#""false""#, r#""bad""#] {
        assert!(
            serde_json::from_str::<RedactInfoLog>(text)
                .unwrap_err()
                .to_string()
                .contains("value is invalid")
        );
    }
    assert_eq!(
        serde_json::to_string(&RedactInfoLog::Marker).unwrap(),
        r#""MARKER""#
    );
}
