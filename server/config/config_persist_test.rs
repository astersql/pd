// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_configutil::ConfigMetaData;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ScheduleDefaults;
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::{path::Path, sync::Arc};
fn context() -> ConfigDefaultsContext<'static> {
    ConfigDefaultsContext {
        hostname: "host",
        cwd: Path::new("/tmp"),
    }
}
fn configured() -> PersistedConfig {
    let mut c = PersistedConfig::default();
    c.config
        .adjust(
            &ConfigMetaData::default(),
            context(),
            false,
            &mut ScheduleDefaults::default(),
        )
        .unwrap();
    c.store.adjust();
    c
}
async fn reload_contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(12);
    let mut active = configured();
    active.config.scalars.name = "local-host".into();
    assert!(
        !active
            .reload(
                &endpoint,
                &paths,
                context(),
                &mut ScheduleDefaults::default()
            )
            .await
            .unwrap()
    );
    endpoint.save_config(&paths, &serde_json::json!({"schedule":{"max-snapshot-count":10,"schedulers-v2":[]},"replication":{"max-replicas":5}})).await.unwrap();
    assert!(
        active
            .reload(
                &endpoint,
                &paths,
                context(),
                &mut ScheduleDefaults::default()
            )
            .await
            .unwrap()
    );
    assert_eq!(active.config.schedule.max_snapshot_count, 10);
    assert_eq!(active.config.replication.max_replicas, 5);
    assert_eq!(active.config.scalars.name, "local-host");
    assert_eq!(active.config.schedule.schedulers.as_ref().unwrap().len(), 5);
    assert_eq!(active.config.schedule.region_score_formula_version, "");
    assert_eq!(active.config.schedule.max_movable_hot_peer_size, 512);
    let before = active.clone();
    for data in [
        b"{".as_slice(),
        br#"{"pd-server":{"gc-state-cache-warmup-concurrency":0}}"#,
        b"[]",
    ] {
        Base::new(store.as_ref())
            .save(paths.config_path().as_bytes(), data)
            .await
            .unwrap();
        assert!(
            active
                .reload(
                    &endpoint,
                    &paths,
                    context(),
                    &mut ScheduleDefaults::default()
                )
                .await
                .is_err()
        );
        assert_eq!(active, before);
    }
}
#[tokio::test]
async fn config_persist_memory_reload_upgrade() {
    reload_contract(Arc::new(MemoryStore::default())).await;
}

#[allow(dead_code)]
#[path = "../../pkg/storage/kv/etcd_fixture_test.rs"]
pub(super) mod fixture;

async fn roundtrip(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(121);
    let mut original = configured();
    original.config.schedule.max_snapshot_count = 10;
    original.config.replication.max_replicas = 5;
    original.store.coprocessor.region_max_size = "256MiB".into();
    original.config.label_property = Some(std::collections::BTreeMap::from([(
        "reject-leader".into(),
        Some(vec![StoreLabel {
            key: "zone".into(),
            value: "east".into(),
        }]),
    )]));
    original.persist(&endpoint, &paths).await.unwrap();
    let wire: serde_json::Value = endpoint.load_config(&paths).await.unwrap().unwrap();
    assert!(wire.get("Config").is_none());
    assert_eq!(wire["name"], "");
    assert_eq!(wire["schedule"]["max-snapshot-count"], 10);
    assert_eq!(wire["replication"]["enable-placement-rules"], "true");
    assert_eq!(wire["store"]["coprocessor"]["region-max-size"], "256MiB");
    let mut restarted = configured();
    restarted
        .reload(
            &endpoint,
            &paths,
            context(),
            &mut ScheduleDefaults::default(),
        )
        .await
        .unwrap();
    original.store.adjust();
    assert_eq!(restarted, original);
    assert_eq!(restarted.store.region_max_size_mb, 256);
    // Serialization fails before dispatch; the previous durable value survives.
    original.config.schedule.default_store_limit.add_peer = f64::NAN;
    assert!(original.persist(&endpoint, &paths).await.is_err());
    assert_eq!(
        endpoint
            .load_config::<serde_json::Value>(&paths)
            .await
            .unwrap()
            .unwrap(),
        wire
    );
}
#[tokio::test]
async fn config_persist_memory_roundtrip() {
    roundtrip(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn config_persist_etcd_reload_and_roundtrip() {
    let fixture = fixture::Fixture::start();
    reload_contract(Arc::new(fixture.store().await)).await;
    roundtrip(Arc::new(fixture.store().await)).await;
}
#[tokio::test]
async fn config_persist_etcd_restart_and_backend_errors() {
    let mut fixture = fixture::Fixture::start();
    let paths = ClusterPaths::new(122);
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    let mut original = configured();
    original.config.schedule.max_snapshot_count = 10;
    original.persist(&endpoint, &paths).await.unwrap();
    fixture.command("stop", "stopped");
    let mut active = configured();
    let before = active.clone();
    assert!(original.persist(&endpoint, &paths).await.is_err());
    assert!(
        active
            .reload(
                &endpoint,
                &paths,
                context(),
                &mut ScheduleDefaults::default()
            )
            .await
            .is_err()
    );
    assert_eq!(active, before);
    drop(endpoint);
    fixture.command("start", "restarted");
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    active
        .reload(
            &endpoint,
            &paths,
            context(),
            &mut ScheduleDefaults::default(),
        )
        .await
        .unwrap();
    assert_eq!(active, original);
}
#[tokio::test]
async fn config_persist_legacy_limits_null_and_flags() {
    let store = Arc::new(MemoryStore::default());
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(123);
    let mut active = configured();
    let raw = serde_json::json!({"schedule":{"store-balance-rate":60,"default-store-limit":{"add-peer":0},"store-limit":{"1":{"add-peer":10,"remove-peer":20},"2":null},"disable-remove-down-replica":"true","enable-remove-down-replica":"true"}});
    endpoint.save_config(&paths, &raw).await.unwrap();
    let mut defaults = ScheduleDefaults::default();
    active
        .reload(&endpoint, &paths, context(), &mut defaults)
        .await
        .unwrap();
    let c = &active.config.schedule;
    assert_eq!(c.default_store_limit.add_peer, 0.0);
    assert_eq!(c.default_store_limit.remove_peer, 60.0);
    assert_eq!(c.default_store_limit.transfer_leader_in, 100_000_000.0);
    assert_eq!(
        c.store_limit.as_ref().unwrap()[&1].transfer_leader_in,
        100_000_000.0
    );
    assert_eq!(c.store_limit.as_ref().unwrap()[&2].add_peer, 0.0);
    assert_eq!(c.store_balance_rate, 0.0);
    assert!(!c.enable_remove_down_replica);
    assert!(!c.disable_remove_down_replica);
    assert_eq!(defaults.store_limit, c.default_store_limit);
    for data in [
        b"".as_slice(),
        b"null",
        b"{}",
        br#"{"schedule":{"max-snapshot-count":null}}"#,
    ] {
        Base::new(store.as_ref())
            .save(paths.config_path().as_bytes(), data)
            .await
            .unwrap();
        let changed = active
            .reload(
                &endpoint,
                &paths,
                context(),
                &mut ScheduleDefaults::default(),
            )
            .await
            .unwrap();
        assert_eq!(changed, !data.is_empty());
        if changed {
            assert_eq!(active.config.schedule.max_snapshot_count, 64);
        }
    }
}

#[tokio::test]
async fn config_persist_go_snapshot_differential() {
    let go: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule/config/task12/go-persisted.json"
    ))
    .unwrap();
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(124);
    endpoint.save_config(&paths, &go).await.unwrap();
    let mut active = configured();
    active
        .reload(
            &endpoint,
            &paths,
            context(),
            &mut ScheduleDefaults::default(),
        )
        .await
        .unwrap();
    assert_eq!(active.config.schedule.max_snapshot_count, 10);
    assert_eq!(active.config.replication.max_replicas, 3);
    assert_eq!(active.config.schedule.max_movable_hot_peer_size, 512);
    active.persist(&endpoint, &paths).await.unwrap();
    let rust: serde_json::Value = endpoint.load_config(&paths).await.unwrap().unwrap();
    for key in [
        "schedule",
        "replication",
        "pd-server",
        "replication-mode",
        "label-property",
        "keyspace",
        "micro-service",
        "cluster-version",
        "store",
    ] {
        assert_eq!(rust[key], go[key], "section {key}");
    }
}

#[tokio::test]
async fn config_persist_rejects_malformed_migration_fields() {
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let paths = ClusterPaths::new(125);
    let mut active = configured();
    let before = active.clone();
    let mut defaults = ScheduleDefaults::default();
    for raw in [
        serde_json::json!({"schedule":{"store-balance-rate":"invalid"}}),
        serde_json::json!({"schedule":{"disable-remove-down-replica":true}}),
        serde_json::json!({"schedule":{"store-limit":{"1":[]}}}),
        serde_json::json!({"schedule":[]}),
        serde_json::json!({"store":[]}),
    ] {
        endpoint.save_config(&paths, &raw).await.unwrap();
        assert!(
            active
                .reload(&endpoint, &paths, context(), &mut defaults)
                .await
                .is_err(),
            "{raw}"
        );
        assert_eq!(active, before);
        assert_eq!(defaults.store_limit.add_peer, 15.0);
    }
}
