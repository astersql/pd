// Copyright 2026 AsterSQL.
use crate::harness::Server;
use std::{path::PathBuf, process::Command};

#[test]
fn task_101_core_cold_differential() {
    assert_eq!(
        std::env::var("PD_COMPAT_MODE").unwrap_or_else(|_| "dual".into()),
        "dual"
    );
    let mut observed = Vec::new();
    for kind in ["GO", "RUST"] {
        let path = PathBuf::from(
            std::env::var(format!("PD_COMPAT_{kind}_BIN")).unwrap_or_else(|_| {
                if kind == "GO" {
                    format!("{}/../../bin/pd-server", env!("CARGO_MANIFEST_DIR"))
                } else {
                    format!(
                        "{}/debug/pd-server",
                        std::env::var("CARGO_TARGET_DIR").unwrap()
                    )
                }
            }),
        );
        assert!(path.is_absolute());
        let mut server = if kind == "GO" {
            Server::start(&path).unwrap()
        } else {
            Server::start_rust(&path).unwrap()
        };
        server.ready().unwrap();
        let output = Command::new("curl")
            .args([
                "-fsS",
                "--max-time",
                "5",
                &format!("{}/metrics", server.url),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let body = String::from_utf8(output.stdout).unwrap();
        std::fs::write(
            PathBuf::from(std::env::var("CARGO_TARGET_DIR").unwrap())
                .join(format!("task101-{kind}-cold-{}.prom", std::process::id())),
            &body,
        )
        .unwrap();
        server.stop().unwrap();
        observed.push(body);
    }
    // M-022/M-023: Go preallocates both lock labels at package initialization.
    // No heartbeat, sampling or background tick is required to expose them.
    for sample in [
        "pd_core_acquire_regions_lock_wait_duration_seconds_sum{type=\"WaitRegionsLock\"} 0",
        "pd_core_acquire_regions_lock_wait_duration_seconds_count{name=\"WaitRegionsLock\"} 0",
        "pd_core_acquire_regions_lock_wait_duration_seconds_sum{type=\"WaitSubRegionsLock\"} 0",
        "pd_core_acquire_regions_lock_wait_duration_seconds_count{name=\"WaitSubRegionsLock\"} 0",
    ] {
        assert!(
            observed[0].lines().any(|line| line == sample),
            "Go oracle missing {sample}"
        );
        assert!(
            observed[1].lines().any(|line| line == sample),
            "Rust missing required matrix sample: {sample}"
        );
    }
}

fn scrape(url: &str) -> String {
    let output = Command::new("curl")
        .args(["-fsS", "--max-time", "5", &format!("{url}/metrics")])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn sample(text: &str, key: &str) -> f64 {
    text.lines()
        .find_map(|line| {
            line.strip_prefix(&format!("{key} "))
                .map(|v| v.parse().unwrap())
        })
        .unwrap_or_else(|| panic!("missing sample {key}"))
}
fn schema(text: &str) -> std::collections::BTreeSet<String> {
    text.lines()
        .filter(|line| line.starts_with("# HELP pd_core_") || line.starts_with("# TYPE pd_core_"))
        .map(str::to_owned)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_101_core_query_differential() {
    use astersql_pd_proto::{
        metapb,
        pdpb::{self, pd_client::PdClient},
    };
    let mut observed = Vec::new();
    for kind in ["GO", "RUST"] {
        let path = PathBuf::from(
            std::env::var(format!("PD_COMPAT_{kind}_BIN")).unwrap_or_else(|_| {
                if kind == "GO" {
                    format!("{}/../../bin/pd-server", env!("CARGO_MANIFEST_DIR"))
                } else {
                    format!(
                        "{}/debug/pd-server",
                        std::env::var("CARGO_TARGET_DIR").unwrap()
                    )
                }
            }),
        );
        let mut server = if kind == "GO" {
            Server::start(&path).unwrap()
        } else {
            Server::start_rust(&path).unwrap()
        };
        server.ready().unwrap();
        let mut client = PdClient::connect(server.url.clone()).await.unwrap();
        let id = client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let header = Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        });
        let response = client
            .bootstrap(pdpb::BootstrapRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: 1,
                    address: "127.0.0.1:20160".into(),
                    ..Default::default()
                }),
                region: Some(metapb::Region {
                    id: 2,
                    peers: vec![metapb::Peer {
                        id: 3,
                        store_id: 1,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        let before = scrape(&server.url);
        let mut stream = client
            .query_region(tokio_stream::iter([
                pdpb::QueryRegionRequest {
                    header: header.clone(),
                    keys: vec![b"a".to_vec(), b"a".to_vec()],
                    prev_keys: vec![b"z".to_vec()],
                    ids: vec![2, 0, 999],
                    ..Default::default()
                },
                pdpb::QueryRegionRequest {
                    header,
                    ..Default::default()
                },
            ]))
            .await
            .unwrap()
            .into_inner();
        for _ in 0..2 {
            let response =
                tokio::time::timeout(std::time::Duration::from_secs(10), stream.message())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
            assert!(response.header.unwrap().error.is_none());
        }
        assert!(stream.message().await.unwrap().is_none());
        let after = scrape(&server.url);
        let mut values = Vec::new();
        for (kind, count) in [("keys", 2.), ("prev-keys", 1.), ("ids", 3.)] {
            let key = format!("pd_core_query_region_count{{type=\"{kind}\"}}");
            let delta = sample(&after, &key) - sample(&before, &key);
            assert_eq!(delta, count);
            values.push(delta);
            let key = format!("pd_core_query_region_duration_seconds_count{{type=\"by-{kind}\"}}");
            assert_eq!(
                sample(&after, &key) - sample(&before, &key),
                1.,
                "empty inputs must not record observations"
            );
            let key = format!("pd_core_query_region_duration_seconds_sum{{type=\"by-{kind}\"}}");
            assert!(sample(&after, &key) > sample(&before, &key));
        }
        observed.push((schema(&after), values));
        server.stop().unwrap();
    }
    assert_eq!(
        observed[0], observed[1],
        "core HELP/TYPE and actual query increments differ"
    );
}

#[test]
fn task_101_metrics_lifecycle() {
    use astersql_pd_core::{RegionInfo, RegionMeta, RegionsInfo};
    use astersql_pd_server::service_metrics::ServiceMetrics;
    use std::sync::Arc;
    let registry = Arc::new(prometheus::Registry::new());
    let unrelated = prometheus::IntCounter::new("task101_host_total", "host owned").unwrap();
    registry.register(Box::new(unrelated.clone())).unwrap();
    let scope = ServiceMetrics::register(registry.clone()).unwrap();
    let regions = RegionsInfo::new();
    regions.set_metrics(scope.core.clone());
    regions.query_regions(&[b"a"], &[], &[0, 99], false);
    let families = registry.gather();
    assert_eq!(
        families
            .iter()
            .find(|m| m.name() == "pd_core_query_region_count")
            .unwrap()
            .get_metric()
            .iter()
            .map(|m| m.get_counter().get_value())
            .sum::<f64>(),
        3.
    );
    assert!(ServiceMetrics::register(registry.clone()).is_err());
    assert_eq!(
        registry.gather().len(),
        families.len(),
        "failed registration must roll back its own partial collectors"
    );
    scope.core.set_breakdown_metrics(true);
    let mut regions = regions;
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ));
    regions.check_and_put_root_tree(region).unwrap();
    assert!(registry.gather().iter().find(|m|m.name()=="pd_core_region_heartbeat_breakdown_handle_duration_seconds_count").unwrap().get_metric().iter().any(|m|m.get_counter().get_value()>0.));
    // Real acquisition totals are sampled, not replaced by constant exposition.
    for _ in 0..100 {
        regions.get_region(1);
        regions.store_regions(1, None);
    }
    scope.core.collect_wait_metrics(); // baseline is skipped
    for _ in 0..100 {
        regions.get_region(1);
        regions.store_regions(1, None);
    }
    scope.core.collect_wait_metrics();
    assert!(
        registry
            .gather()
            .iter()
            .find(|m| m.name() == "pd_core_acquire_regions_lock_wait_duration_seconds_count")
            .unwrap()
            .get_metric()
            .iter()
            .all(|m| m.get_counter().get_value() > 0.)
    );
    drop(scope); // domain handles do not keep the registry registrations alive
    let names: Vec<_> = registry
        .gather()
        .iter()
        .map(|m| m.name().to_owned())
        .collect();
    assert_eq!(names, vec!["task101_host_total"]);
    assert!(ServiceMetrics::register(registry).is_ok());
}

#[test]
fn task_101_behavior() {
    super::http_regression::run(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
}
#[test]
fn task_101_inventory_negative_controls() {
    use super::http_regression::{catalog, inventory, validate};
    let rows = inventory();
    let catalog = catalog();
    validate(&rows, &catalog).unwrap();
    for field in ["id", "owner", "row"] {
        let mut changed = catalog.clone();
        changed[0][field] = serde_json::json!("unknown");
        assert!(
            validate(&rows, &changed).is_err(),
            "{field} mutation swallowed"
        );
    }
    let mut changed = catalog.clone();
    changed.pop();
    assert!(validate(&rows, &changed).is_err());
    let mut changed = rows.clone();
    changed.insert("HTTP-999".into(), "| GET | /unowned |".into());
    assert!(validate(&changed, &catalog).is_err());
    let mut broadened = catalog.clone();
    broadened[0]["label_dependencies"] =
        serde_json::json!([{"label":"type","values":["anything"],"dependency":"ignore"}]);
    assert!(validate(&rows, &broadened).is_err());
    let mut changed = catalog;
    changed[0]["replays"] = serde_json::json!([]);
    changed[0]["dependency"] = serde_json::json!("");
    assert!(validate(&rows, &changed).is_err());
}
// A real-process control for non-core classic telemetry. No constructor-only zeros.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_101_classic_metrics_differential() {
    use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
    let mut observed = Vec::new();
    for kind in ["GO", "RUST"] {
        let binary = PathBuf::from(
            std::env::var(format!("PD_COMPAT_{kind}_BIN")).unwrap_or_else(|_| {
                if kind == "GO" {
                    format!("{}/../../bin/pd-server", env!("CARGO_MANIFEST_DIR"))
                } else {
                    format!(
                        "{}/debug/pd-server",
                        std::env::var("CARGO_TARGET_DIR").unwrap()
                    )
                }
            }),
        );
        let mut server = if kind == "GO" {
            Server::start(&binary).unwrap()
        } else {
            Server::start_rust(&binary).unwrap()
        };
        server.ready().unwrap();
        let mut client = PdClient::connect(server.url.clone()).await.unwrap();
        let id = client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let response = client
            .alloc_id(pdpb::AllocIdRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: id,
                    ..Default::default()
                }),
                count: 1001,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        assert!(response.id > 0);
        let body = scrape(&server.url);
        use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
        let store = EtcdMetaStore::connect(
            &[server.metadata_url().to_owned()],
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap();
        let persisted = store
            .read(Read::range(
                format!("/pd/{id}/alloc_id").as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let persisted =
            u64::from_be_bytes(persisted.entries[0].value.as_slice().try_into().unwrap());
        assert!(persisted >= response.id);
        assert_eq!(
            sample(&body, "pd_cluster_id{type=\"idalloc\"}"),
            persisted as f64,
            "gauge must be the actually persisted ID window"
        );
        for family in [
            "pd_cluster_id",
            "pd_tso_events",
            "pd_cluster_tso",
            "pd_cluster_tso_gap_millionseconds",
            "pd_cluster_tso_operation_duration_seconds",
            "pd_tso_role",
        ] {
            assert!(
                body.lines()
                    .any(|line| line.starts_with(&format!("# HELP {family} "))),
                "missing {family}"
            );
        }
        observed.push(body);
        server.stop().unwrap();
    }
    let event_keys = |text: &str| {
        text.lines()
            .filter(|line| {
                line.starts_with("pd_tso_events{")
                    || line.starts_with("pd_schedule_scatter_operators_count{")
            })
            .map(|line| line.rsplit_once(' ').unwrap().0.to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        event_keys(&observed[0]),
        event_keys(&observed[1]),
        "TSO/scatter eager label vocabulary differs"
    );
    for family in [
        "pd_cluster_id",
        "pd_tso_events",
        "pd_cluster_tso",
        "pd_cluster_tso_gap_millionseconds",
        "pd_cluster_tso_operation_duration_seconds",
        "pd_tso_role",
    ] {
        let descriptors = |text: &str| {
            text.lines()
                .filter(|line| {
                    line.starts_with(&format!("# HELP {family} "))
                        || line.starts_with(&format!("# TYPE {family} "))
                })
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            descriptors(&observed[0]),
            descriptors(&observed[1]),
            "{family} schema differs"
        );
    }
}

#[test]
fn task_101_all_metric_descriptors() {
    use astersql_pd_server::service_metrics::ServiceMetrics;
    let scope = ServiceMetrics::register(std::sync::Arc::new(prometheus::Registry::new())).unwrap();
    let descriptors = scope.metric_descriptors();
    for row in super::http_regression::catalog()
        .iter()
        .filter(|row| row.get("metric_contract").is_some())
    {
        let contract = &row["metric_contract"];
        let name = contract["name"].as_str().unwrap();
        if ["M-075", "M-076", "M-077", "M-132", "M-156"].contains(&row["id"].as_str().unwrap()) {
            continue;
        }
        let desc = descriptors
            .iter()
            .find(|desc| desc.fq_name == name)
            .unwrap_or_else(|| panic!("{} unregistered collector {name}", row["id"]));
        assert_eq!(
            desc.help,
            contract["help"].as_str().unwrap(),
            "{} HELP",
            row["id"]
        );
        let mut labels = desc.variable_labels.clone();
        labels.sort();
        assert_eq!(
            serde_json::json!(labels),
            contract["labels"],
            "{} labels",
            row["id"]
        );
    }
}

#[test]
fn task_101_deleted_store_cannot_recreate_series() {
    use astersql_pd_core::{StoreInfo, StoreMeta, StoresInfo};
    use astersql_pd_server::service_metrics::ServiceMetrics;
    use std::sync::Arc;
    let registry = Arc::new(prometheus::Registry::new());
    let scope = ServiceMetrics::register(registry.clone()).unwrap();
    let stores = StoresInfo::new();
    stores.set_filter_metrics(scope.store_metric_sink());
    stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 101,
        ..Default::default()
    })));
    let stale = stores.get_store(101).unwrap();
    stale.record_filter_metrics("balance-region-scheduler", "store-state-filter", false);
    stale.record_scatter_metrics(true, "tikv");
    let present = || {
        registry
            .gather()
            .iter()
            .filter(|family| {
                [
                    "pd_schedule_target_filter",
                    "pd_schedule_scatter_distribution",
                ]
                .contains(&family.name())
            })
            .flat_map(|family| family.get_metric())
            .any(|metric| {
                metric.get_label().iter().any(|label| {
                    ["store", "target"].contains(&label.name()) && label.value() == "101"
                })
            })
    };
    assert!(present());
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let worker = {
        let stale = stale.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..1000 {
                stale.record_filter_metrics(
                    "balance-region-scheduler",
                    "store-state-filter",
                    false,
                );
                stale.record_scatter_metrics(true, "tikv");
            }
        })
    };
    barrier.wait();
    stores.delete_store(101);
    worker.join().unwrap();
    stale.record_filter_metrics("balance-region-scheduler", "store-state-filter", false);
    stale.record_scatter_metrics(true, "tikv");
    assert!(
        !present(),
        "in-flight immutable store snapshots recreated deleted series"
    );
}

#[tokio::test]
async fn task_101_worker_owned_metric_contracts() {
    use astersql_pd_core::{NodeState, StoreInfo, StoreMeta};
    use astersql_pd_statistics_buckets::{BucketRangeIndex, BucketWorker};
    use std::{
        sync::Arc,
        time::{Duration, SystemTime},
    };
    let registry = prometheus::Registry::new();
    let manager = astersql_pd_progress::Manager::new(
        Arc::new(|| Duration::ZERO),
        Duration::from_secs(10),
        &registry,
    )
    .unwrap();
    let store = StoreInfo::new(StoreMeta {
        id: 101,
        node_state: NodeState::Preparing,
        ..Default::default()
    });
    let now = SystemTime::now();
    manager.update_progress_at(Some(&store), 1., 100., now);
    manager.update_progress_at(Some(&store), 2., 100., now + Duration::from_secs(10));
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut worker = BucketWorker::start(
        &tokio::runtime::Handle::current(),
        &cancel,
        &registry,
        BucketRangeIndex::default(),
    )
    .unwrap();
    worker
        .client()
        .collect(0, vec![])
        .unwrap()
        .wait(&cancel)
        .await
        .unwrap();
    let families = registry.gather();
    for row in super::http_regression::catalog()
        .iter()
        .filter(|row| ["M-075", "M-076", "M-077", "M-132"].contains(&row["id"].as_str().unwrap()))
    {
        let contract = &row["metric_contract"];
        let family = families
            .iter()
            .find(|family| family.name() == contract["name"].as_str().unwrap())
            .unwrap();
        assert_eq!(family.help(), contract["help"].as_str().unwrap());
        for sample in family.get_metric() {
            let mut labels = sample
                .get_label()
                .iter()
                .map(|label| label.name().to_owned())
                .collect::<Vec<_>>();
            labels.sort();
            assert_eq!(serde_json::json!(labels), contract["labels"]);
        }
    }
    worker.shutdown().await.unwrap();
    drop(manager);
    assert!(registry.gather().is_empty());
}

#[test]
fn task_101_metric_schema_negative_controls() {
    use super::http_regression::{catalog, validate_live_metrics};
    use serde_json::json;
    let row = catalog()
        .into_iter()
        .find(|row| row["id"] == "M-024")
        .unwrap();
    let contract = &row["metric_contract"];
    let name = contract["name"].as_str().unwrap();
    let text = format!(
        "# HELP {name} {}\n# TYPE {name} {}\n",
        contract["help"].as_str().unwrap(),
        contract["kind"].as_str().unwrap()
    );
    let snapshot = |kind| json!({"implementation":kind,"exposition":text,"sample_schemas":{name:{"labels":[contract["labels"]],"buckets":[0.0005,0.001],"enum_labels":{"type":["by-keys"]},"scope_dependencies":[]}}});
    let report =
        json!({"observations":[{"schemas":[snapshot("GO")]},{"schemas":[snapshot("RUST")]}]});
    validate_live_metrics(std::slice::from_ref(&row), std::slice::from_ref(&report)).unwrap();
    for field in [
        "missing",
        "help",
        "type",
        "labels",
        "buckets",
        "label-values",
    ] {
        let mut changed = report.clone();
        let rust = &mut changed["observations"][1]["schemas"][0];
        match field {
            "missing" => rust["exposition"] = json!(""),
            "help" => rust["exposition"] = json!(text.replace("# HELP", "# CHANGED")),
            "type" => rust["exposition"] = json!(text.replace("histogram", "gauge")),
            "labels" => rust["sample_schemas"][name]["labels"] = json!([["unknown"]]),
            "label-values" => {
                rust["sample_schemas"][name]["enum_labels"] =
                    serde_json::json!({"type":["unknown"]})
            }
            "buckets" => rust["sample_schemas"][name]["buckets"] = json!([0.001]),
            _ => unreachable!(),
        }
        assert!(
            validate_live_metrics(std::slice::from_ref(&row), &[changed]).is_err(),
            "{field} mutation swallowed"
        );
    }
}

#[test]
fn task_101_real_balance_debug_metrics() {
    use astersql_pd_core::{StoreInfo, StoreMeta, StoresInfo};
    use astersql_pd_core_constant::{ResourceKind, ScheduleKind, SchedulePolicy};
    use astersql_pd_schedule_config::ScheduleConfig;
    use astersql_pd_schedule_operator::OpInfluence;
    use astersql_pd_schedule_plan::BalanceSchedulerPlan;
    use astersql_pd_schedule_schedulers::BalanceSolver;
    use astersql_pd_server::service_metrics::ServiceMetrics;
    use std::sync::Arc;
    let registry = Arc::new(prometheus::Registry::new());
    let scope = ServiceMetrics::register(registry.clone()).unwrap();
    let stores = StoresInfo::new();
    stores.set_filter_metrics(scope.store_metric_sink());
    for (id, count) in [(101, 100), (102, 10)] {
        let mut store = StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        });
        store.set_leader_count(count);
        stores.put_store(Arc::new(store));
    }
    let plan = BalanceSchedulerPlan {
        source: stores.get_store(101),
        target: stores.get_store(102),
        ..Default::default()
    };
    let influence = OpInfluence::new();
    influence.update_store(101, |store| store.leader_count = 3);
    influence.update_store(102, |store| store.leader_count = -4);
    let kind = ScheduleKind::new(ResourceKind::LEADER, SchedulePolicy::BY_COUNT);
    let mut config = ScheduleConfig::default();
    let evaluate = |config: &ScheduleConfig| {
        BalanceSolver::new(kind, config, 0, 0)
            .with_metric_name("balance-leader-scheduler")
            .evaluate(&plan, &influence)
            .unwrap()
    };
    assert!(evaluate(&config).should_balance());
    assert!(
        !registry
            .gather()
            .iter()
            .any(|f| f.name() == "pd_scheduler_op_influence")
    );
    config.enable_debug_metrics = true;
    assert!(evaluate(&config).should_balance());
    let value = |family: &str, store: &str| {
        registry
            .gather()
            .iter()
            .find(|f| f.name() == family)
            .unwrap()
            .get_metric()
            .iter()
            .find(|metric| {
                store.is_empty()
                    || metric
                        .get_label()
                        .iter()
                        .any(|label| label.name() == "store" && label.value() == store)
            })
            .unwrap()
            .get_gauge()
            .get_value()
    };
    assert_eq!(value("pd_scheduler_op_influence", "101"), -3.);
    assert_eq!(value("pd_scheduler_op_influence", "102"), 4.);
    assert_eq!(value("pd_scheduler_tolerant_resource", ""), 5.);
    stores.delete_store(101);
    evaluate(&config); // a retained candidate cannot recreate the deleted store
    assert!(
        !registry
            .gather()
            .iter()
            .filter(|f| f.name() == "pd_scheduler_op_influence")
            .flat_map(|f| f.get_metric())
            .any(|m| m
                .get_label()
                .iter()
                .any(|label| label.name() == "store" && label.value() == "101"))
    );
}

#[test]
fn task_101_hot_summary_uses_real_population() {
    use astersql_pd_core::{StoreInfo, StoreMeta};
    use astersql_pd_core_constant::ResourceKind;
    use astersql_pd_server::service_metrics::ServiceMetrics;
    use astersql_pd_statistics::{StoreKindLoads, summary_store_infos, summary_stores_load};
    use astersql_pd_statistics_utils::{RWType, StoreLoadKind};
    use std::{collections::HashMap, sync::Arc, time::Duration};
    let registry = Arc::new(prometheus::Registry::new());
    let scope = ServiceMetrics::register(registry.clone()).unwrap();
    let stores = [1, 2].map(|id| {
        StoreInfo::new(StoreMeta {
            id,
            ..Default::default()
        })
    });
    let infos = summary_store_infos(&stores);
    let collect = |second| {
        let loads: HashMap<_, _> = [(1, 10.), (2, second)]
            .into_iter()
            .map(|(id, value)| {
                let mut loads: StoreKindLoads = Default::default();
                loads[StoreLoadKind::READ_BYTES.as_i32() as usize] = value;
                (id, loads)
            })
            .collect();
        let peers = [1, 2]
            .into_iter()
            .map(|id| {
                (
                    id,
                    vec![astersql_pd_statistics::HotPeerStat {
                        store_id: id,
                        region_id: id,
                        ..Default::default()
                    }],
                )
            })
            .collect();
        let details = summary_stores_load(
            &infos,
            &loads,
            None,
            &peers,
            true,
            RWType::READ,
            ResourceKind::REGION,
            Duration::ZERO,
        )
        .into_values()
        .collect::<Vec<_>>();
        scope
            .statistics
            .collect_hot_load_summary(&details, RWType::READ, ResourceKind::REGION);
    };
    let value = |typ: &str| {
        registry
            .gather()
            .iter()
            .find(|f| f.name() == "pd_scheduler_hot_peers_summary")
            .unwrap()
            .get_metric()
            .iter()
            .find(|sample| {
                sample
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "type" && label.value() == typ)
            })
            .unwrap()
            .get_gauge()
            .get_value()
    };
    collect(30.);
    assert_eq!(value("exp-byte-rate-read-region"), 20.);
    assert_eq!(value("stddev-byte-rate-read-region"), 0.5);
    collect(50.);
    assert_eq!(value("exp-byte-rate-read-region"), 30.);
    assert!((value("stddev-byte-rate-read-region") - 2. / 3.).abs() < 1e-12);
}

#[test]
fn task_101_hotspot_collection_and_store_deletion() {
    use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
    use astersql_pd_schedule::HotObservationInputs;
    use astersql_pd_server::service_metrics::ServiceMetrics;
    use astersql_pd_statistics::HotPeerStat;
    use std::{
        collections::HashMap,
        sync::{Arc, Barrier, mpsc},
        time::Duration,
    };
    let registry = Arc::new(prometheus::Registry::new());
    let scope = ServiceMetrics::register(registry.clone()).unwrap();
    let cluster = Arc::new(BasicCluster::new());
    cluster.stores.set_filter_metrics(scope.store_metric_sink());
    cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 101,
        address: "127.0.0.1:20160".into(),
        ..Default::default()
    })));
    let snapshots = cluster
        .stores
        .get_stores()
        .iter()
        .map(|store| store.as_ref().clone())
        .collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(3));
    let (done, wait) = mpsc::channel();
    let collector = {
        let scope = scope.clone();
        let cluster = cluster.clone();
        let barrier = barrier.clone();
        let done = done.clone();
        std::thread::spawn(move || {
            let read = HashMap::from([(
                101,
                vec![HotPeerStat {
                    store_id: 101,
                    region_id: 11,
                    loads: vec![1., 2., 3., 4.],
                    ..Default::default()
                }],
            )]);
            let empty = HashMap::new();
            let loads = HashMap::new();
            barrier.wait();
            for _ in 0..2000 {
                scope.hotspot.collect_hot_spot_metrics(
                    &cluster,
                    &snapshots,
                    &HotObservationInputs {
                        loads: &loads,
                        read: &read,
                        write: &empty,
                    },
                );
            }
            done.send(()).unwrap();
        })
    };
    let deletion = {
        let cluster = cluster.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..2000 {
                cluster.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
                    id: 101,
                    address: "127.0.0.1:20160".into(),
                    ..Default::default()
                })));
                cluster.stores.delete_store(101);
            }
            done.send(()).unwrap();
        })
    };
    barrier.wait();
    for _ in 0..2 {
        wait.recv_timeout(Duration::from_secs(10))
            .expect("membership deletion and metric collection must not invert locks");
    }
    collector.join().unwrap();
    deletion.join().unwrap();
    assert!(
        !registry
            .gather()
            .iter()
            .filter(|family| family.name() == "pd_hotspot_status")
            .flat_map(|family| family.get_metric())
            .any(|metric| metric
                .get_label()
                .iter()
                .any(|label| label.name() == "store" && label.value() == "101"))
    );
}

#[tokio::test]
async fn task_101_scheduler_empty_region_events() {
    use astersql_pd_core::{BasicCluster, StoreInfo, StoreMeta};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::AffinityFilter;
    use astersql_pd_schedule_operator::OpInfluence;
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_schedule_schedulers::{BalanceLeaderScheduler, LeaderBalanceContext};
    use astersql_pd_storage_domain::{MemoryStore, StorageEndpoint};
    use std::sync::Arc;
    let scope = astersql_pd_server::service_metrics::ServiceMetrics::register(Arc::new(
        prometheus::Registry::new(),
    ))
    .unwrap();
    let core = BasicCluster::new();
    core.regions.set_metrics(scope.core.clone());
    let mut store = StoreInfo::new(StoreMeta {
        id: 1,
        ..Default::default()
    });
    store.set_last_heartbeat_unix_nanos(1_000_000_000_000);
    core.stores.put_store(Arc::new(store));
    let endpoint = StorageEndpoint::new(Arc::new(MemoryStore::default()));
    let mut rules = RuleManager::new(Arc::new(endpoint.clone()), ClusterPaths::new(101));
    let labels =
        astersql_pd_schedule_labeler::RegionLabeler::load(endpoint, ClusterPaths::new(101), 0)
            .await
            .unwrap();
    let mut config = ProviderSnapshot::<()>::default();
    config.schedule.leader_schedule_policy = "count".into();
    let stats = astersql_pd_statistics::StatisticsSnapshot::default();
    let ranges = astersql_pd_schedule_keyrange::Manager::new();
    let influence = OpInfluence::new();
    let mut index = |_: usize| 0;
    let mut shuffle = |_: &mut [usize]| {};
    let mut ctx = LeaderBalanceContext {
        cluster: &core,
        config: &config,
        rules: &mut rules,
        affinity: AffinityFilter {
            manager: None,
            labels: &labels,
            now: 1000,
        },
        statistics: &stats,
        hot_degree: 1,
        key_ranges: &ranges,
        influence: &influence,
        now_unix_nanos: 1_000_000_000_000,
        random_index: &mut index,
        shuffle: &mut shuffle,
    };
    let out = BalanceLeaderScheduler::default()
        .schedule(&mut ctx, false)
        .await;
    assert!(out.operators.is_empty());
    for event in ["no-leader-region", "no-follower-region"] {
        assert!(
            scope
                .schedulers
                .event_metrics("balance-leader-scheduler", event)
                .unwrap()
                .get()
                > 0.,
            "real empty-region decision did not publish {event}"
        );
    }
}
