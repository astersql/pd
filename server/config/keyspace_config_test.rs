// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;

use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_typeutil::Duration;

use super::*;

fn groups(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn keyspace_config_defaults_and_presence() {
    let mut config = KeyspaceConfig::default();
    config.adjust(&ConfigMetaData::default()).unwrap();
    assert!(config.wait_region_split);
    assert_eq!(
        config.wait_region_split_timeout,
        Duration::from_nanos(30_000_000_000)
    );
    assert_eq!(
        config.check_region_split_interval,
        Duration::from_nanos(50_000_000)
    );
    assert!(config.pre_alloc.is_none() && config.meta_service_groups.is_none());
    config.validate().unwrap();
    let saved = config.clone();
    config.adjust(&ConfigMetaData::default()).unwrap();
    assert_eq!(config, saved);

    let (mut explicit, metadata) = decode_toml_keyspace("wait-region-split = false\nwait-region-split-timeout = '-1s'\ncheck-region-split-interval = '0s'\npre-alloc = ['a,b', '', 'a,b']").unwrap();
    explicit.adjust(&metadata).unwrap();
    assert!(!explicit.wait_region_split);
    assert_eq!(
        explicit.wait_region_split_timeout,
        Duration::from_nanos(-1_000_000_000)
    );
    assert_eq!(
        explicit.check_region_split_interval,
        Duration::from_nanos(0)
    );
    assert_eq!(explicit.get_pre_alloc(), ["a,b", "", "a,b"]);
    // Missing fields overwrite even nonzero values; this is not AdjustDuration.
    explicit.adjust(&ConfigMetaData::default()).unwrap();
    assert!(explicit.wait_region_split);
    assert_eq!(
        explicit.wait_region_split_timeout,
        saved.wait_region_split_timeout
    );
    assert_eq!(
        explicit.check_region_split_interval,
        saved.check_region_split_interval
    );
}

#[test]
fn keyspace_config_split_validation_boundaries() {
    for nanos in [
        i64::MIN,
        -1,
        0,
        999_999,
        1_000_000,
        50_000_000,
        100_000_000,
        100_000_001,
        i64::MAX,
    ] {
        let mut config = KeyspaceConfig {
            check_region_split_interval: Duration::from_nanos(nanos),
            wait_region_split_timeout: Duration::from_nanos(i64::MAX),
            ..Default::default()
        };
        let before = config.clone();
        let result = config.validate();
        assert_eq!(config, before);
        if !(1_000_000..=100_000_000).contains(&nanos) {
            assert_eq!(
                result.unwrap_err(),
                "[keyspace] check-region-split-interval should between 1ms and 100ms"
            );
        } else {
            result.unwrap();
            for timeout in [i64::MIN, 0, nanos - 1, nanos] {
                config.wait_region_split_timeout = Duration::from_nanos(timeout);
                assert_eq!(
                    config.validate().unwrap_err(),
                    "[keyspace] check-region-split-interval should be less than wait-region-split-timeout"
                );
            }
            config.wait_region_split_timeout = Duration::from_nanos(nanos + 1);
            config.validate().unwrap();
        }
    }
}

#[test]
fn keyspace_config_group_ids_and_atomic_errors() {
    for id in ["", " ", ".", "..", "group.1:8080", "雪", "\\", "\0"] {
        assert!(is_valid_meta_service_group_id(id), "{id:?}");
    }
    for id in ["/", "group/1", " / "] {
        assert!(!is_valid_meta_service_group_id(id));
    }
    for (entries, message) in [
        (
            vec![(" \t", "http://a")],
            "[keyspace] meta-service group ID cannot be empty",
        ),
        (
            vec![(" group/1 ", "")],
            "[keyspace] meta-service group ID cannot contain '/': group/1",
        ),
        (
            vec![("group-1", " \n")],
            "[keyspace] meta-service group addresses cannot be empty",
        ),
        (
            vec![("group-1", "a"), (" group-1 ", "b")],
            "[keyspace] meta-service group ID cannot be duplicated: group-1",
        ),
    ] {
        let mut map = groups(&entries);
        map.insert("valid".into(), " endpoint ".into());
        let before = map.clone();
        assert_eq!(adjust_meta_service_groups(&mut map).unwrap_err(), message);
        assert_eq!(map, before);
    }
}

#[test]
fn keyspace_config_group_mapping_and_adjust_failure() {
    let mut map = groups(&[
        (
            "\u{85}\u{a0} group-1 \u{3000}",
            " \nhttp://127.0.0.1:2379\t",
        ),
        ("group.1:8080", "not a URL"),
    ]);
    adjust_meta_service_groups(&mut map).unwrap();
    assert_eq!(
        map,
        groups(&[
            ("group-1", "http://127.0.0.1:2379"),
            ("group.1:8080", "not a URL")
        ])
    );
    let before = map.clone();
    adjust_meta_service_groups(&mut map).unwrap();
    assert_eq!(map, before);
    let mut empty = BTreeMap::new();
    adjust_meta_service_groups(&mut empty).unwrap();
    assert!(empty.is_empty());
    let mut config = KeyspaceConfig {
        meta_service_groups: Some(groups(&[("/", "a")])),
        ..Default::default()
    };
    assert!(config.adjust(&ConfigMetaData::default()).is_err());
    // Go applies scalar defaults before checking groups, retaining them on error.
    assert!(config.wait_region_split);
    assert_eq!(
        config.wait_region_split_timeout,
        Duration::from_nanos(30_000_000_000)
    );
    assert_eq!(config.meta_service_groups.unwrap(), groups(&[("/", "a")]));
}

#[test]
fn keyspace_config_clone_and_accessors() {
    let original = KeyspaceConfig {
        pre_alloc: Some(vec!["a".into()]),
        meta_service_groups: Some(groups(&[("id", "endpoint")])),
        ..Default::default()
    };
    let mut cloned = original.clone();
    cloned.pre_alloc.as_mut().unwrap()[0] = "changed".into();
    cloned
        .meta_service_groups
        .as_mut()
        .unwrap()
        .insert("id".into(), "changed".into());
    assert_eq!(original.get_pre_alloc(), ["a"]);
    assert_eq!(
        original.get_meta_service_groups(),
        groups(&[("id", "endpoint")])
    );
    let mut list = original.get_pre_alloc();
    list.clear();
    let mut map = original.get_meta_service_groups();
    map.clear();
    assert_eq!(original.get_pre_alloc(), ["a"]);
    assert_eq!(original.get_meta_service_groups().len(), 1);
    cloned.set_meta_service_groups(None);
    assert!(cloned.get_meta_service_groups().is_empty());
    assert!(KeyspaceConfig::default().clone().pre_alloc.is_none());
}

#[test]
fn keyspace_config_existing_go_json_bytes() {
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
        let start = line.find("\"keyspace\":").unwrap() + "\"keyspace\":".len();
        let end = line[start..].find(",\"micro-service\":").unwrap() + start;
        let go_bytes = &line[start..end];
        let mut config = KeyspaceConfig::default();
        config.adjust(&ConfigMetaData::default()).unwrap();
        assert_eq!(serde_json::to_string(&config).unwrap(), go_bytes);
        assert_eq!(
            serde_json::from_str::<KeyspaceConfig>(go_bytes).unwrap(),
            config
        );
    }
    let (mut config, metadata) =
        decode_toml_keyspace("pre-alloc = []\n[meta-service-groups]\n' id ' = ' endpoint '")
            .unwrap();
    config.adjust(&metadata).unwrap();
    assert_eq!(config.pre_alloc, Some(vec![]));
    assert_eq!(
        config.get_meta_service_groups(),
        groups(&[("id", "endpoint")])
    );
    for input in [
        "pre-alloc = 'a'",
        "pre-alloc = [1]",
        "wait-region-split = 'true'",
        "wait-region-split-timeout = 1",
        "[meta-service-groups]\nx = 1",
    ] {
        assert!(decode_toml_keyspace(input).is_err(), "{input}");
    }
}
