// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_cache::{TTLUint64, TtlDuration};
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, RegionInfo, RegionMeta, StoreInfo, StoreMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_hbstream::routing::StreamRouter;
use astersql_pd_schedule_scatter::ScatterBatchMetrics;
use astersql_pd_schedule_schedulers::{FactoryContext, JobTime};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn setup(
    config: serde_json::Value,
    backend: Arc<dyn astersql_pd_storage_kv::MetaStore>,
) -> (Coordinator<ProviderSnapshot<()>>, Arc<TTLUint64<()>>) {
    let mut cluster = BasicCluster::new();
    let peers: Vec<_> = (1..=3)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: peers.clone(),
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"b".to_vec(),
            },
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    let (origin, overlaps, changed) = cluster.regions.set_region(region.clone());
    cluster
        .regions
        .update_sub_tree(region, origin.as_deref(), &overlaps, changed);
    let cluster = Arc::new(cluster);
    for id in 1..=3 {
        cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })));
    }
    let (heartbeat, _router) = StreamRouter::new(Arc::new(astersql_pd_core::StoresInfo::new()));
    let pending = Arc::new(TTLUint64::new(
        tokio::runtime::Handle::current(),
        CancellationToken::new(),
        Duration::from_secs(60),
        TtlDuration::from_nanos(180_000_000_000),
    ));
    let c = Coordinator::new(CoordinatorInputs {
        cluster,
        config: Arc::new({
            let mut config = ProviderSnapshot::<()>::default();
            config.schedule.switch_witness_interval = TtlDuration::from_nanos(10);
            config
        }),
        heartbeat,
        pending: pending.clone(),
        factory: FactoryContext::new(
            Arc::new(StorageEndpoint::new(backend)),
            ClusterPaths::new(114),
            JobTime(0),
        ),
        scheduler_configuration: config,
        scatter_metrics: ScatterBatchMetrics::new(&prometheus::Registry::new()).unwrap(),
        start: Duration::ZERO,
        now_unix_nanos: 0,
        split_merge_interval_nanos: 0,
        total_regions: Box::new(|| Ok(0)),
        unix_seconds: Arc::new(|| 0),
    });
    (c, pending)
}
#[tokio::test]
async fn task_117_behavior_cases() {
    use astersql_pd_storage_kv::{Consistency, MetaStore, Read};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_117.json"
    ))
    .unwrap();
    let backend = Arc::new(MemoryStore::default());
    let (mut c, pending) = setup(
        serde_json::json!({"schedule":{"schedulers-v2":[{"type":"balance-leader"}]}}),
        backend.clone(),
    );
    c.init_schedulers(true, &CancellationToken::new())
        .await
        .unwrap();
    let name = "balance-leader-scheduler";
    c.get_schedulers_controller_mut()
        .pause_or_resume(name, 60, 10)
        .unwrap();
    c.record_op_step_with_ttl(77, 0);
    assert!(c.get_rule_checker().is_recently_promoted(77, 1));
    let cluster = c.get_cluster().clone();
    let checker = c.get_checker_controller().clone();
    let operators = c.operator_handle();
    let before = format!(
        "{:?}",
        backend
            .read(Read::prefix(b"", Consistency::Linearizable))
            .await
            .unwrap()
    );
    let config = c.get_schedulers_controller().configuration().clone();
    for row in fixture["cases"].as_array().unwrap() {
        for action in [PluginAction::Load, PluginAction::Unload] {
            let path = std::path::Path::new(row["path"].as_str().unwrap());
            let guard = checker.lock().unwrap();
            let result = match action {
                PluginAction::Load => c.load_plugin(path),
                PluginAction::Unload => c.unload_plugin(path),
            };
            drop(guard);
            let error = result.unwrap_err();
            assert_eq!(error, UnsupportedGoPlugin { action });
            assert_eq!(error.to_string(), fixture["error"].as_str().unwrap());
            let s = c.get_schedulers_controller();
            assert_eq!(s.names(), vec![name]);
            assert_eq!(s.configuration(), &config);
            assert!(s.is_paused(name, 11).unwrap());
            assert!(Arc::ptr_eq(&cluster, c.get_cluster()));
            assert!(Arc::ptr_eq(&checker, c.get_checker_controller()));
            assert!(Arc::ptr_eq(&operators, &c.operator_handle()));
            assert!(c.get_rule_checker().is_recently_promoted(77, 1));
            assert!(c.are_schedulers_initialized());
            assert_eq!(
                format!(
                    "{:?}",
                    backend
                        .read(Read::prefix(b"", Consistency::Linearizable))
                        .await
                        .unwrap()
                ),
                before
            );
        }
    }
    pending.shutdown().await;
}
#[tokio::test]
async fn task_117_existing_file_and_invalid_path() {
    let backend = Arc::new(MemoryStore::default());
    let (c, pending) = setup(serde_json::json!({}), backend);
    // Existing non-plugin file and a NUL path must reach the same boundary error.
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin_interface.go");
    let bytes = std::fs::read(&source).unwrap();
    for path in [source.as_path(), std::path::Path::new("\0invalid.so")] {
        assert_eq!(
            c.load_plugin(path),
            Err(UnsupportedGoPlugin {
                action: PluginAction::Load
            })
        );
        assert_eq!(
            c.unload_plugin(path),
            Err(UnsupportedGoPlugin {
                action: PluginAction::Unload
            })
        );
    }
    assert_eq!(std::fs::read(source).unwrap(), bytes);
    assert!(c.get_schedulers_controller().names().is_empty());
    assert!(!c.are_schedulers_initialized());
    pending.shutdown().await;
}
