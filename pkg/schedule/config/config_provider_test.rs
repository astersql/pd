// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_core::StoreLabel;
use astersql_pd_schedule_types::{BALANCE_LEADER_SCHEDULER, CheckerSchedulerType};
use astersql_pd_storelimit::Type;

fn label(key: &str, value: &str) -> StoreLabel {
    StoreLabel {
        key: key.into(),
        value: value.into(),
    }
}
#[test]
fn config_provider_shared_generation_and_store_delegation() {
    let mut snapshot = ProviderSnapshot::<String>::default();
    snapshot.schedule.region_schedule_limit = 27;
    snapshot.schedule.max_merge_region_size = 20;
    snapshot.replication.max_replicas = 5;
    snapshot.cluster_version = "8.5.1".into();
    snapshot.store.coprocessor.region_max_keys = 123;
    snapshot.schedule.default_store_limit.add_peer = 17.0;
    let scheduler: &dyn SchedulerConfigProvider<Version = String> = &snapshot;
    let checker: &dyn CheckerConfigProvider<Version = String> = &snapshot;
    let shared: &dyn SharedConfigProvider<Version = String> = &snapshot;
    let store: &dyn StoreConfigProvider = &snapshot;
    assert_eq!(scheduler.get_region_schedule_limit(), 27);
    assert_eq!(checker.get_region_schedule_limit(), 27);
    assert_eq!(shared.get_max_replicas(), 5);
    assert_eq!(checker.get_max_replicas(), 5);
    assert_eq!(store.get_region_max_keys(), 123);
    assert_eq!(shared.get_cluster_version(), "8.5.1");
    assert_eq!(shared.get_store_limit_by_type(1, Type::ADD_PEER), 17.0);
    assert_eq!(checker.get_max_merge_region_keys(), 200_000);
    assert!(std::ptr::eq(scheduler.snapshot(), checker.snapshot()));
    assert!(store.check_region_size(116, 20).is_ok());
    let mut next = snapshot.clone();
    next.schedule.region_schedule_limit = 90;
    next.replication.max_replicas = 3;
    assert_eq!(snapshot.get_region_schedule_limit(), 27);
    assert_eq!(next.get_region_schedule_limit(), 90);
    assert_eq!(snapshot.get_max_replicas(), 5);
    next.schedule.max_merge_region_keys = 42;
    assert_eq!(next.get_max_merge_region_keys(), 42);
}
#[test]
fn config_provider_reject_leader_exact_or_matching() {
    let mut snapshot = ProviderSnapshot::<()>::default();
    snapshot.label_properties.insert(
        REJECT_LEADER.into(),
        vec![label("zone", "east"), label("disk", "ssd")],
    );
    for (typ, labels, expected) in [
        (REJECT_LEADER, vec![], false),
        (REJECT_LEADER, vec![label("zone", "east")], true),
        (REJECT_LEADER, vec![label("zone", "East")], false),
        (REJECT_LEADER, vec![label("Zone", "east")], false),
        (
            REJECT_LEADER,
            vec![label("zone", "west"), label("disk", "ssd")],
            true,
        ),
        ("other", vec![label("zone", "east")], false),
    ] {
        assert_eq!(snapshot.check_label_property(typ, &labels), expected);
    }
    snapshot.label_properties.insert("empty".into(), vec![]);
    assert!(!snapshot.check_label_property("empty", &[label("", "")]));
}
#[test]
fn config_provider_registry_and_scheduler_first_match() {
    let mut registry = SchedulerRegistry::default();
    let custom = CheckerSchedulerType::from("custom-scheduler");
    registry.register(custom.clone());
    registry.register(custom.clone());
    registry.register(BALANCE_LEADER_SCHEDULER);
    assert_eq!(registry.registered().count(), 2);
    assert!(registry.contains(&custom));
    assert!(!SchedulerRegistry::default().contains(&custom));
    let mut snapshot = ProviderSnapshot::<()>::default();
    assert!(!snapshot.is_scheduler_disabled(&BALANCE_LEADER_SCHEDULER));
    snapshot.schedule.schedulers = Some(vec![
        SchedulerConfig {
            r#type: "balance-leader".into(),
            disable: true,
            ..Default::default()
        },
        SchedulerConfig {
            r#type: "balance-leader".into(),
            disable: false,
            ..Default::default()
        },
    ]);
    assert!(snapshot.is_scheduler_disabled(&BALANCE_LEADER_SCHEDULER));
    assert!(!snapshot.is_scheduler_disabled(&custom));
    assert!(snapshot.is_trace_region_flow());
    snapshot.flow_round_by_digit = 6;
    assert!(!snapshot.is_trace_region_flow());
}
#[test]
fn config_provider_flags_rates_and_integer_boundaries() {
    use astersql_pd_core_constant::{KeyType, SchedulePolicy};
    let mut snapshot = ProviderSnapshot::<()>::default();
    snapshot.replication.max_replicas = u64::MAX;
    snapshot.schedule.hot_region_cache_hits_threshold = u64::MAX;
    assert_eq!(snapshot.get_max_replicas(), -1);
    assert_eq!(snapshot.get_hot_region_cache_hits_threshold(), -1);
    snapshot.schedule.max_merge_region_size = u64::MAX;
    assert_eq!(
        snapshot.get_max_merge_region_keys(),
        u64::MAX.wrapping_mul(10_000)
    );
    snapshot.schedule.leader_schedule_policy = "size".into();
    snapshot.key_type = KeyType::RAW;
    assert_eq!(
        snapshot.get_leader_schedule_policy(),
        SchedulePolicy::BY_SIZE
    );
    assert_eq!(snapshot.get_key_type(), KeyType::RAW);
    snapshot.schedule.enable_witness = true;
    snapshot.replication.enable_placement_rules = true;
    snapshot.replication.enable_placement_rules_cache = true;
    snapshot.schedule.enable_joint_consensus = true;
    assert!(snapshot.is_witness_allowed());
    assert!(snapshot.is_placement_rules_enabled());
    assert!(snapshot.is_placement_rules_cache_enabled());
    assert!(snapshot.is_use_joint_consensus());
    snapshot.schedule.store_limit = Some(std::collections::BTreeMap::from([(
        9,
        StoreLimitConfig {
            add_peer: 1.0,
            remove_peer: 2.0,
            transfer_leader_in: 3.0,
        },
    )]));
    for (typ, expected) in [
        (Type::ADD_PEER, 1.0),
        (Type::REMOVE_PEER, 2.0),
        (Type::TRANSFER_LEADER_IN, 3.0),
        (Type::SEND_SNAPSHOT, 0.0),
    ] {
        assert_eq!(snapshot.get_store_limit_by_type(9, typ), expected);
    }
    assert_eq!(snapshot.get_stores_limit().unwrap().len(), 1);
    assert!(std::ptr::eq(
        snapshot.get_schedule_config(),
        &snapshot.schedule
    ));
}
