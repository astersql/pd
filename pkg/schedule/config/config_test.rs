// Copyright 2026 AsterSQL.
use super::*;
fn configured() -> ScheduleConfig {
    let mut c = ScheduleConfig::default();
    c.adjust(
        &ConfigMetaData::default(),
        false,
        &mut ScheduleDefaults::default(),
    )
    .unwrap();
    c
}
#[test]
fn schedule_defaults_presence_reload_and_snapshot() {
    let mut c = configured();
    assert_eq!(c.max_snapshot_count, 64);
    assert_eq!(c.max_pending_peer_count, 64);
    assert_eq!(c.default_store_limit.transfer_leader_in, 100_000_000.0);
    assert_eq!(c.schedulers.as_ref().unwrap().len(), 5);
    c.schedulers.as_mut().unwrap()[0].r#type = "changed".into();
    assert_eq!(
        ScheduleDefaults::default().schedulers[0].r#type,
        "balance-region"
    );
    let mut c = ScheduleConfig::default();
    let meta = ConfigMetaData::from_defined_paths([
        ["max-snapshot-count"],
        ["enable-joint-consensus"],
        ["max-store-preparing-time"],
    ]);
    c.adjust(&meta, true, &mut ScheduleDefaults::default())
        .unwrap();
    assert_eq!(c.max_snapshot_count, 0);
    assert!(!c.enable_joint_consensus);
    assert_eq!(c.max_store_preparing_time.as_nanos(), 0);
    assert_eq!(c.region_score_formula_version, "");
}
#[test]
fn schedule_all_validation_branches() {
    for (field, expected) in [
        (
            0,
            "default-store-limit.add-peer should be finite and non-negative",
        ),
        (
            1,
            "default-store-limit.remove-peer should be finite and non-negative",
        ),
        (
            2,
            "default-store-limit.transfer-leader-in should be finite and non-negative",
        ),
        (3, "tolerant-size-ratio should be non-negative"),
        (4, "low-space-ratio should between 0 and 1"),
        (5, "high-space-ratio should between 0 and 1"),
        (6, "low-space-ratio should be larger than high-space-ratio"),
        (7, "leader-schedule-policy invalid is invalid"),
        (
            8,
            "slow-store-evicting-affected-store-ratio-threshold is not set",
        ),
        (9, "patrol-region-worker-count should be between 1 and 8"),
    ] {
        let mut c = configured();
        match field {
            0 => c.default_store_limit.add_peer = f64::NAN,
            1 => c.default_store_limit.remove_peer = f64::INFINITY,
            2 => c.default_store_limit.transfer_leader_in = -1.0,
            3 => c.tolerant_size_ratio = -1.0,
            4 => c.low_space_ratio = 1.1,
            5 => c.high_space_ratio = -0.1,
            6 => c.low_space_ratio = c.high_space_ratio,
            7 => c.leader_schedule_policy = "invalid".into(),
            8 => c.slow_store_evicting_affected_store_ratio_threshold = 0.0,
            _ => c.patrol_region_worker_count = 0,
        }
        assert_eq!(c.validate().unwrap_err(), expected);
    }
    for key in 0..3 {
        let mut c = configured();
        let mut limit = StoreLimitConfig::default();
        match key {
            0 => limit.add_peer = -1.0,
            1 => limit.remove_peer = f64::NAN,
            _ => limit.transfer_leader_in = f64::INFINITY,
        };
        c.store_limit.as_mut().unwrap().insert(9, limit);
        assert!(c.validate().unwrap_err().starts_with("store-limit[9]."));
    }
    let mut c = configured();
    c.tolerant_size_ratio = f64::NAN;
    c.low_space_ratio = f64::NAN;
    c.high_space_ratio = f64::NAN;
    c.slow_store_evicting_affected_store_ratio_threshold = f64::NAN;
    assert!(c.validate().is_ok());
    assert!(serde_json::to_string(&c).is_err());
}
#[test]
fn schedule_five_deprecated_flag_truth_tables() {
    for name in [
        "remove-down-replica",
        "replace-offline-replica",
        "make-up-replica",
        "remove-extra-replica",
        "location-replacement",
    ] {
        for old_defined in [false, true] {
            for new_defined in [false, true] {
                for old in [false, true] {
                    for new in [false, true] {
                        let mut paths = Vec::new();
                        if old_defined {
                            paths.push([format!("disable-{name}")]);
                        }
                        if new_defined {
                            paths.push([format!("enable-{name}")]);
                        }
                        let meta = ConfigMetaData::from_defined_paths(paths);
                        let result = deprecated_flag(&meta, name, old, new);
                        if old_defined && new_defined && old == new {
                            assert!(result.unwrap_err().contains("are conflict"));
                        } else {
                            assert_eq!(
                                result.unwrap(),
                                if new_defined {
                                    new
                                } else if old_defined {
                                    !old
                                } else {
                                    true
                                }
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn schedule_store_migration_and_json_inheritance() {
    let mut defaults = ScheduleDefaults::default();
    let mut c = ScheduleConfig {
        store_balance_rate: 7.0,
        ..Default::default()
    };
    c.adjust(&ConfigMetaData::default(), false, &mut defaults)
        .unwrap();
    assert_eq!(c.store_balance_rate, 0.0);
    assert_eq!(defaults.store_limit.add_peer, 7.0);
    let mut other = ScheduleConfig::default();
    other
        .adjust(&ConfigMetaData::default(), false, &mut defaults)
        .unwrap();
    assert_eq!(other.default_store_limit.remove_peer, 7.0);
    let c=decode_json(r#"{"default-store-limit":{"add-peer":9,"remove-peer":3,"transfer-leader-in":5},"store-limit":{"2":{"add-peer":0,"remove-peer":null},"9":null}}"#).unwrap();
    let stores = c.store_limit.unwrap();
    assert_eq!(stores[&2].add_peer, 0.0);
    assert_eq!(stores[&2].remove_peer, 3.0);
    assert_eq!(stores[&2].transfer_leader_in, 5.0);
    assert_eq!(stores[&9].add_peer, 9.0);
    assert!(decode_json(r#"{"default-store-limit":null}"#).is_ok());
}
#[test]
fn replication_labels_and_isolation() {
    for label in ["zone", "$rack/a", "a.b_c-d", "1"] {
        validate_label_key(label).unwrap();
    }
    for label in ["", "$", "_a", "a/", "a b", "é", "$-a"] {
        assert!(validate_label_key(label).is_err(), "{label}");
    }
    let mut c = ReplicationConfig {
        location_labels: ["zone"].into(),
        isolation_level: "zone".into(),
        ..Default::default()
    };
    let meta =
        ConfigMetaData::from_defined_paths([["location-labels"], ["enable-placement-rules"]]);
    c.adjust(&meta).unwrap();
    assert_eq!(c.max_replicas, 3);
    assert!(!c.enable_placement_rules);
    c.isolation_level = "rack".into();
    assert_eq!(
        c.validate().unwrap_err(),
        "isolation-level must be one of location-labels or empty"
    );
}
