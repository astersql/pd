// Copyright 2026 AsterSQL.
use astersql_pd_configutil::ConfigMetaData;

use super::*;

const KEYS: [&str; 3] = [
    "enable-scheduling-fallback",
    "enable-tso-dynamic-switching",
    "enable-resource-manager-fallback",
];

fn flags(config: &MicroserviceConfig) -> [bool; 3] {
    [
        config.is_scheduling_fallback_enabled(),
        config.is_tso_dynamic_switching_enabled(),
        config.is_resource_manager_fallback_enabled(),
    ]
}

#[test]
fn microservice_config_presence_all_combinations() {
    for presence in 0..8 {
        for values in 0..8 {
            let mut config = MicroserviceConfig {
                enable_scheduling_fallback: values & 1 != 0,
                enable_tso_dynamic_switching: values & 2 != 0,
                enable_resource_manager_fallback: values & 4 != 0,
            };
            let metadata = ConfigMetaData::from_defined_paths(
                KEYS.iter()
                    .enumerate()
                    .filter(|(i, _)| presence & (1 << i) != 0)
                    .map(|(_, key)| [*key]),
            );
            let mut expected = [true, false, true];
            for (i, flag) in expected.iter_mut().enumerate() {
                if presence & (1 << i) != 0 {
                    *flag = values & (1 << i) != 0;
                }
            }
            config.adjust(&metadata);
            assert_eq!(
                flags(&config),
                expected,
                "presence={presence} values={values}"
            );
            let before = config.clone();
            config.adjust(&metadata);
            assert_eq!(config, before);
        }
    }
}

#[test]
fn microservice_config_toml_presence_and_types() {
    for presence in 0..8 {
        for values in 0..8 {
            let input = KEYS
                .iter()
                .enumerate()
                .filter(|(i, _)| presence & (1 << i) != 0)
                .map(|(i, key)| format!("{key} = {}", values & (1 << i) != 0))
                .collect::<Vec<_>>()
                .join("\n");
            let (mut config, metadata) = decode_toml_microservice(&input).unwrap();
            config.adjust(&metadata);
            for (i, key) in KEYS.iter().enumerate() {
                assert_eq!(metadata.is_defined(key), presence & (1 << i) != 0);
                assert_eq!(
                    flags(&config)[i],
                    if presence & (1 << i) != 0 {
                        values & (1 << i) != 0
                    } else {
                        i != 1
                    }
                );
            }
        }
    }
    for key in KEYS {
        for value in ["'true'", "1", "[]", "{}"] {
            assert!(decode_toml_microservice(&format!("{key} = {value}")).is_err());
        }
    }
}

#[test]
fn microservice_config_nested_metadata() {
    let metadata = ConfigMetaData::from_defined_paths([
        ["micro-service", KEYS[0]],
        ["other", KEYS[1]],
        ["micro-service", KEYS[2]],
    ]);
    let mut config = MicroserviceConfig {
        enable_tso_dynamic_switching: true,
        ..Default::default()
    };
    config.adjust(&metadata.child(["micro-service"]));
    assert_eq!(flags(&config), [false, false, false]);
}

#[test]
fn microservice_config_clone_and_getters() {
    for values in 0..8 {
        let original = MicroserviceConfig {
            enable_scheduling_fallback: values & 1 != 0,
            enable_tso_dynamic_switching: values & 2 != 0,
            enable_resource_manager_fallback: values & 4 != 0,
        };
        let mut copy = original.clone();
        assert_eq!(copy, original);
        copy.enable_scheduling_fallback = !copy.enable_scheduling_fallback;
        copy.enable_tso_dynamic_switching = !copy.enable_tso_dynamic_switching;
        copy.enable_resource_manager_fallback = !copy.enable_resource_manager_fallback;
        assert_eq!(
            flags(&original),
            [values & 1 != 0, values & 2 != 0, values & 4 != 0]
        );
        assert_eq!(
            flags(&copy),
            [values & 1 == 0, values & 2 == 0, values & 4 == 0]
        );
    }
}

#[test]
fn microservice_config_existing_go_json_bytes() {
    let evidence = include_str!("../../docs/rust/theme3-服务配置契约.md");
    for label in ["missing", "explicit-zero"] {
        let marker = format!("<!-- batch84-{label} -->\n\n```json\n");
        let line = evidence
            .split_once(&marker)
            .unwrap()
            .1
            .lines()
            .next()
            .unwrap();
        let start = line.find("\"micro-service\":{").unwrap() + "\"micro-service\":".len();
        let end = line[start..].find(",\"controller\":").unwrap() + start;
        let go_bytes = &line[start..end];
        let mut config = MicroserviceConfig::default();
        config.adjust(&ConfigMetaData::default());
        assert_eq!(serde_json::to_string(&config).unwrap(), go_bytes);
        assert_eq!(
            serde_json::from_str::<MicroserviceConfig>(go_bytes).unwrap(),
            config
        );
    }
}

#[test]
fn microservice_config_json_roundtrip_and_types() {
    for values in 0..8 {
        let config = MicroserviceConfig {
            enable_scheduling_fallback: values & 1 != 0,
            enable_tso_dynamic_switching: values & 2 != 0,
            enable_resource_manager_fallback: values & 4 != 0,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(
            serde_json::from_str::<MicroserviceConfig>(&json).unwrap(),
            config
        );
        for key in KEYS {
            assert!(serde_json::from_str::<serde_json::Value>(&json).unwrap()[key].is_string());
        }
    }
    assert_eq!(
        serde_json::from_str::<MicroserviceConfig>("{}").unwrap(),
        MicroserviceConfig::default()
    );
    for key in KEYS {
        for value in ["true", "false", "1", "[]", "{}", "\"TRUE\"", "\"1\""] {
            assert!(
                serde_json::from_str::<MicroserviceConfig>(&format!("{{\"{key}\":{value}}}"))
                    .is_err()
            );
        }
        for value in ["null", "\"null\""] {
            assert_eq!(
                serde_json::from_str::<MicroserviceConfig>(&format!("{{\"{key}\":{value}}}"))
                    .unwrap(),
                MicroserviceConfig::default()
            );
        }
    }
}
