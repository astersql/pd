// Copyright 2026 AsterSQL.
use crate::{HttpKeyRangesError, decode_http_key_ranges};
use serde_json::{Value, json};

#[test]
fn decode_http_key_ranges_success() {
    for (start, end, expected) in [
        ("", "", vec![vec![], vec![]]),
        (
            "a+b,%2c,",
            "%FF,%00,%2B",
            vec![
                b"a b".to_vec(),
                vec![255],
                b",".to_vec(),
                vec![0],
                vec![],
                b"+".to_vec(),
            ],
        ),
        (
            "6162",
            "中文",
            vec![b"6162".to_vec(), "中文".as_bytes().to_vec()],
        ),
    ] {
        let input = json!({"start-key": start, "end-key": end, "ignored": false});
        assert_eq!(
            decode_http_key_ranges(input.as_object().unwrap()).unwrap(),
            expected
        );
    }
}

#[test]
fn decode_http_key_ranges_invalid_arguments() {
    for invalid in [Value::Null, json!(1), json!(true), json!([]), json!({})] {
        for field in ["start-key", "end-key"] {
            let mut input = json!({"start-key":"", "end-key":""})
                .as_object()
                .unwrap()
                .clone();
            input.insert(field.into(), invalid.clone());
            let HttpKeyRangesError::InvalidArgument(error) =
                decode_http_key_ranges(&input).unwrap_err()
            else {
                panic!("wrong error class")
            };
            assert_eq!(error.code(), "PD:common:ErrInvalidArgument");
            assert_eq!(error.args(), &[field]);
        }
    }
    let input = serde_json::Map::new();
    let HttpKeyRangesError::InvalidArgument(error) = decode_http_key_ranges(&input).unwrap_err()
    else {
        panic!("wrong error class")
    };
    assert_eq!(error.args(), &["start-key"]);
    let input = json!({"start-key":"%,b", "end-key":"z"});
    let HttpKeyRangesError::InvalidArgument(error) =
        decode_http_key_ranges(input.as_object().unwrap()).unwrap_err()
    else {
        panic!("count checked before escape")
    };
    assert_eq!(error.args(), &["%,b", "z"]);
}

#[test]
fn decode_http_key_ranges_escape_errors() {
    for (start, end, bad) in [
        ("%", "%gg", "%"),
        ("a", "%0", "%0"),
        ("%G1", "", "%G1"),
        ("%aZtail", "", "%aZ"),
        ("a,%ff", "b,%z0", "%z0"),
    ] {
        let input = json!({"start-key":start,"end-key":end});
        let error = decode_http_key_ranges(input.as_object().unwrap()).unwrap_err();
        assert!(matches!(error, HttpKeyRangesError::Escape(_)));
        assert_eq!(error.to_string(), format!("invalid URL escape {bad:?}"));
    }
}
