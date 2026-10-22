// Copyright 2026 AsterSQL.

use super::*;

#[test]
fn string_slice_json() {
    let strings = StringSlice::from(["zone", "rack"]);
    assert_eq!(serde_json::to_string(&strings).unwrap(), r#""zone,rack""#);
    assert_eq!(
        serde_json::from_str::<StringSlice>(r#""zone,rack""#).unwrap(),
        strings
    );

    let empty = StringSlice::default();
    assert_eq!(serde_json::to_string(&empty).unwrap(), r#""""#);
    assert_eq!(serde_json::from_str::<StringSlice>(r#""""#).unwrap(), empty);

    assert!(serde_json::from_str::<StringSlice>(r#"["zone","rack"]"#).is_err());
    assert!(serde_json::from_str::<StringSlice>("null").is_err());
}
