// Copyright 2026 AsterSQL.
use super::*;

fn fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../server/config/fixtures/nested_config_go.json"
    ))
    .unwrap()
}

#[test]
fn config_values_json_document_boundary() {
    for input in ["{} {}", "null false", "{} trailing", "{}\u{0}"] {
        assert!(
            decode_json(input).is_err(),
            "accepted trailing input: {input:?}"
        );
    }
    assert!(decode_json(" {} \n\t").is_ok());
}

#[test]
fn config_values_json_null_numbers() {
    let config = decode_json(r#"{"default-store-limit":{"add-peer":null,"remove-peer":null,"transfer-leader-in":null},"low-space-ratio":null,"high-space-ratio":null,"tolerant-size-ratio":null,"store-balance-rate":null,"slow-store-evicting-affected-store-ratio-threshold":null}"#).unwrap();
    assert_eq!(config, ScheduleConfig::default());
}

#[test]
fn config_values_go_snapshots_round_trip() {
    let mut count = 0;
    for case in fixtures()["cases"].as_array().unwrap() {
        let Some(expected) = case.get("config") else {
            continue;
        };
        let schedule = decode_json(&expected["schedule"].to_string()).unwrap();
        let replication: ReplicationConfig =
            serde_json::from_value(expected["replication"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(schedule).unwrap(),
            expected["schedule"],
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::to_value(replication).unwrap(),
            expected["replication"],
            "{}",
            case["name"]
        );
        count += 1;
    }
    assert_eq!(count, 5);
}

#[test]
fn config_values_go_toml_defaults_presence_and_errors() {
    let mut count = 0;
    for case in fixtures()["cases"].as_array().unwrap() {
        let table: toml::Table = toml::from_str(case["toml"].as_str().unwrap()).unwrap();
        let schedule_input = table
            .get("schedule")
            .map(|v| v.to_string())
            .unwrap_or_default();
        // Value::Display uses inline table syntax; serialize the table itself.
        let schedule_input = if schedule_input.is_empty() {
            String::new()
        } else {
            toml::to_string(table["schedule"].as_table().unwrap()).unwrap()
        };
        let replication_input = table
            .get("replication")
            .map(|v| toml::to_string(v.as_table().unwrap()).unwrap())
            .unwrap_or_default();
        if let Some(expected) = case.get("config") {
            let (mut schedule, meta) = decode_toml(&schedule_input).unwrap();
            schedule
                .adjust(&meta, false, &mut ScheduleDefaults::default())
                .unwrap();
            assert_eq!(
                serde_json::to_value(schedule).unwrap(),
                expected["schedule"],
                "{}",
                case["name"]
            );
            let (mut replication, meta) = decode_toml_replication(&replication_input).unwrap();
            replication.adjust(&meta).unwrap();
            assert_eq!(
                serde_json::to_value(replication).unwrap(),
                expected["replication"],
                "{}",
                case["name"]
            );
            count += 1;
        } else if [
            "store-map",
            "space",
            "worker",
            "negative-limit",
            "flag-conflict",
            "isolation",
        ]
        .contains(&case["name"].as_str().unwrap())
        {
            let result = decode_toml(&schedule_input)
                .and_then(|(mut c, m)| c.adjust(&m, false, &mut ScheduleDefaults::default()))
                .and_then(|()| {
                    decode_toml_replication(&replication_input).and_then(|(mut c, m)| c.adjust(&m))
                });
            assert_eq!(result.unwrap_err(), case["output"].as_str().unwrap().trim());
        }
    }
    assert_eq!(count, 5);
}

#[test]
fn config_values_clone_owned_collections() {
    let mut original = decode_json(r#"{"schedulers-v2":[{"type":"balance-region","args":["a"],"args-payload":"p"}],"store-limit":{"7":{"add-peer":3}}}"#).unwrap();
    let mut cloned = original.clone();
    let scheduler = &mut cloned.schedulers.as_mut().unwrap()[0];
    scheduler.r#type = "changed".into();
    scheduler.args.as_mut().unwrap()[0] = "changed".into();
    scheduler.args_payload.clear();
    cloned
        .store_limit
        .as_mut()
        .unwrap()
        .get_mut(&7)
        .unwrap()
        .add_peer = 99.0;
    assert_eq!(
        original.schedulers.as_ref().unwrap()[0].r#type,
        "balance-region"
    );
    assert_eq!(
        original.schedulers.as_ref().unwrap()[0]
            .args
            .as_ref()
            .unwrap()[0],
        "a"
    );
    assert_eq!(original.store_limit.as_ref().unwrap()[&7].add_peer, 3.0);
    original.store_limit.as_mut().unwrap().clear();
    assert_eq!(cloned.store_limit.as_ref().unwrap().len(), 1);
    let original = ReplicationConfig {
        location_labels: ["zone", "rack"].into(),
        ..Default::default()
    };
    let mut cloned = original.clone();
    cloned.location_labels = ["host"].into();
    assert_eq!(original.location_labels.as_slice(), &["zone", "rack"]);
}

#[test]
fn config_values_label_keys_go_table() {
    for (key, valid) in [
        ("z1", true),
        ("z-1", true),
        ("h1;", false),
        ("z_1", true),
        ("z_1&", false),
        ("cn", true),
        ("Zo^ne", false),
        ("z_", false),
        ("hos&t-15", false),
        ("_test1", false),
        ("-test1", false),
        ("127.0.0.1", true),
        ("www.pingcap.com", true),
        ("h_127.0.0.1", true),
        ("a", true),
        ("a/b", true),
        ("ab/", false),
        ("/ab", false),
        ("$abc", true),
        ("$", false),
        ("a$b", false),
        ("$$", false),
        ("é", false),
        ("a\n", false),
    ] {
        assert_eq!(validate_label_key(key).is_ok(), valid, "{key:?}");
    }
}

#[test]
fn config_values_label_values_and_first_error() {
    for value in ["", "z1", "-_./", "127.0.0.1", "a/b"] {
        validate_labels([("zone", value)]).unwrap();
    }
    for value in ["$abc", "a b", "é", "a\n", "a&b"] {
        assert_eq!(
            validate_labels([("zone", value)]).unwrap_err(),
            format!("{value} does not match format \"^[-A-Za-z0-9_./]*$\"")
        );
    }
    assert_eq!(
        validate_labels([("_bad", "$bad")]).unwrap_err(),
        validate_label_key("_bad").unwrap_err()
    );
    assert!(validate_labels([]).is_ok());
}
