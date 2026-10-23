// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn controller_presence_costs_and_push() {
    let mut c = ControllerConfig::default();
    c.push_metrics_address = "push".into();
    let meta = ConfigMetaData::from_defined_paths([
        vec!["ltb-max-wait-duration"],
        vec!["request-unit", "read-base-cost"],
    ]);
    c.adjust(&meta);
    assert_eq!(c.ltb_max_wait_duration.as_nanos(), 0);
    assert_eq!(c.request_unit.read_base_cost, 0.0);
    assert_eq!(c.request_unit.cpu_ms_cost, 1.0 / 3.0);
    assert!(c.push_metrics_address.is_empty());
    c.push_metrics_address = "push".into();
    c.push_metrics_interval = Duration::from_nanos(1);
    c.adjust(&ConfigMetaData::default());
    assert_eq!(c.push_metrics_address, "push");
    assert_eq!(c.ltb_max_wait_duration.as_nanos(), 30000000000);
}
#[test]
fn ru_policy_validation_lookup_clone_and_omission() {
    let mut c = ControllerConfig::default();
    assert_eq!(c.keyspace_ru_version(3), 1);
    assert!(
        !serde_json::to_string(&c)
            .unwrap()
            .contains("ru-version-policy")
    );
    let mut p = RUVersionPolicy {
        default: 2,
        overrides: BTreeMap::from([(3, 4)]),
    };
    p.validate().unwrap();
    assert_eq!(p.keyspace_ru_version(3), 4);
    assert_eq!(p.keyspace_ru_version(9), 2);
    let clone = p.clone();
    p.overrides.insert(3, 0);
    assert_eq!(clone.overrides[&3], 4);
    assert_eq!(
        p.validate().unwrap_err(),
        "ru-version-policy override for keyspace 3 must be positive, got 0"
    );
    p.default = 0;
    assert_eq!(p.keyspace_ru_version(9), 1);
    assert_eq!(
        p.validate().unwrap_err(),
        "ru-version-policy default must be positive, got 0"
    );
    c.ru_version_policy = Some(p);
    c.adjust(&ConfigMetaData::default());
    assert_eq!(c.keyspace_ru_version(3), 0); // Adjust deliberately does not call policy.validate.
}
