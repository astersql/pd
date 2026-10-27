// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta, StoreState,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::RegionStatistics;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;

fn region(version: u64, count: u64, size: i64) -> RegionInfo {
    let peers: Vec<_> = (1..=count)
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 129,
            epoch: Some(RegionEpoch {
                version,
                conf_ver: version,
            }),
            peers: peers.clone(),
            ..Default::default()
        },
        peers.first().cloned(),
        vec![],
        vec![],
    );
    r.stats_mut().approximate_size = size;
    r.stats_mut().approximate_keys = 400_000;
    r
}
#[test]
fn task_129_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_129.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut bc = BasicCluster::new();
        for id in 1..=4 {
            if id == 3 && case["missing_store"] == true {
                continue;
            }
            let mut store = StoreInfo::new(StoreMeta {
                id,
                ..Default::default()
            });
            if id == 4 || (id == 3 && case["offline"] == true) {
                store.set_store_state(StoreState::Offline, Some(false));
            }
            bc.stores.put_store(Arc::new(store));
        }
        let event = region(1, 1, 0);
        bc.regions
            .check_and_put_root_tree(Arc::new(event.clone()))
            .unwrap();
        bc.regions
            .check_and_put_root_tree(Arc::new(region(2, 3, 40)))
            .unwrap();
        if case["present"] == false {
            bc.regions.remove_region_if_exist(129);
        }
        let mut config = ProviderSnapshot::<()>::default();
        config.replication.max_replicas = 3;
        config.replication.enable_placement_rules = case["placement"] == true;
        config.schedule.max_merge_region_size = 20;
        config.schedule.max_merge_region_keys = 200_000;
        let rules = RuleManager::new(
            Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
            ClusterPaths::new(129),
        );
        let mut stats = RegionStatistics::new();
        let mut checks = 0;
        let result = collect(
            (case["basic"] == true).then_some(&bc),
            &event,
            &mut stats,
            &config,
            &rules,
            100,
            || {
                checks += 1;
                case["cancel"] == true
            },
        );
        let reachable = case["basic"] == true && case["present"] == true;
        assert_eq!(checks, usize::from(reachable), "{case}");
        assert_eq!(
            result.is_err(),
            reachable && case["cancel"] == true,
            "{case}"
        );
        assert_eq!(
            stats.bitmap(129),
            case["bitmap"].as_u64().map(|n| n as u16),
            "{case}"
        );
        // A cancelled collection preserves an already observed real classification.
        if reachable && !config.replication.enable_placement_rules {
            stats.observe(&event, &[], &config, 100).unwrap();
            let before = stats.bitmap(129);
            assert_eq!(
                collect(Some(&bc), &event, &mut stats, &config, &rules, 105, || true),
                Err(CollectCancelled)
            );
            assert_eq!(stats.bitmap(129), before);
            collect(Some(&bc), &event, &mut stats, &config, &rules, 106, || {
                false
            })
            .unwrap();
            assert_eq!(
                stats.bitmap(129),
                Some(
                    if case["offline"] == true && case["missing_store"] == false {
                        16
                    } else {
                        0
                    }
                )
            );
        }
    }
}
