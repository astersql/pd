// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{Peer, RegionEpoch, RegionInfo, RegionMeta, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_affinity::{
    group::Group,
    manager::Manager,
    ranges::{GroupKeyRanges, make_label_rule},
};
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_schedule_placement::RuleManager;
use astersql_pd_statistics::{LabelStatistics, RegionStatistics};
use astersql_pd_storage_endpoint::StorageEndpoint;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;

#[tokio::test]
async fn task_128_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_128.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
        let paths = ClusterPaths::new(128);
        let mut labels = RegionLabeler::load(
            StorageEndpoint::new(Arc::new(MemoryStore::default())),
            paths,
            0,
        )
        .await
        .unwrap();
        let mut affinity = Manager::default();
        let mut rule = make_label_rule(&GroupKeyRanges {
            group_id: "g".into(),
            key_ranges: vec![astersql_pd_keyutil::KeyRange::new(vec![1], vec![9])],
        });
        rule.check_and_adjust(0).unwrap();
        affinity.sync_key_ranges_from_etcd(&rule).unwrap();
        affinity.sync_group_from_etcd(
            &Group {
                id: "g".into(),
                leader_store_id: 1,
                voter_store_ids: vec![1],
                ..Default::default()
            },
            &labels,
            100,
        );
        labels.set_label_rule(rule, 0).await.unwrap();
        affinity.restore_affinity_group("g", 100);
        let mut conf = ProviderSnapshot::<()>::default();
        conf.replication.enable_placement_rules_cache = true;
        let stores = vec![Arc::new(StoreInfo::new(StoreMeta {
            id: 1,
            last_heartbeat: 1,
            ..Default::default()
        }))];
        let mut rules = RuleManager::new(storage, paths);
        rules
            .initialize(1, &[], "", false, &conf, &[])
            .await
            .unwrap();
        let mut stats = RegionStatistics::new();
        let mut label_stats = LabelStatistics::new();
        let regions: Vec<_> = (1..=3)
            .map(|id| {
                let peer = Peer {
                    id,
                    store_id: 1,
                    ..Default::default()
                };
                Arc::new(RegionInfo::new(
                    RegionMeta {
                        id,
                        epoch: Some(RegionEpoch::default()),
                        key_range: astersql_pd_core::KeyRange {
                            start_key: vec![id as u8],
                            end_key: vec![id as u8 + 1],
                        },
                        peers: vec![peer.clone()],
                        ..Default::default()
                    },
                    Some(peer),
                    vec![],
                    vec![],
                ))
            })
            .collect();
        for r in &regions {
            stats
                .observe(r, &[stores[0].as_ref().clone()], &conf, 100)
                .unwrap();
            label_stats.observe(r, &[], &[]);
            assert!(
                affinity
                    .get_and_cache_region_affinity_group_state(&labels, Some(r), 100)
                    .1
            );
            let fit = rules.fit_region(&stores, r, &conf, 1);
            rules.set_region_fit_cache(&stores, r, fit, 1);
            for _ in 0..11 {
                rules.fit_region(&stores, r, &conf, 1);
            }
            assert!(rules.is_region_fit_cached(&stores, r, 1));
            assert!(stats.is_observed(r.meta().id));
        }
        assert_eq!(affinity.affinity_region_count(), 3);
        let limit = case["cancel_after"].as_u64().unwrap() as usize;
        let mut checks = 0;
        let mut cancel = || {
            let cancelled = checks >= limit;
            checks += 1;
            cancelled
        };
        let result = handle_overlaps(
            &regions,
            (case["stats"] == true).then_some(&mut stats),
            &mut label_stats,
            &mut rules,
            (case["affinity"] == true).then_some(&mut affinity),
            &mut cancel,
        );
        let processed = case["processed"].as_u64().unwrap() as usize;
        if processed < regions.len() {
            assert_eq!(result, Err(OverlapsCancelled { processed }));
        } else {
            assert_eq!(result, Ok(processed));
        }
        // Label counts are deferred, and every completed ID has all cache effects.
        assert_eq!(label_stats.label_counter()["none"], 3);
        for (index, r) in regions.iter().enumerate() {
            let cleared = index < processed;
            assert_eq!(
                stats.is_observed(r.meta().id),
                !(cleared && case["stats"] == true),
                "{case}"
            );
            assert_eq!(
                rules.is_region_fit_cached(&stores, r, 1),
                !cleared,
                "{case}"
            );
        }
        let expected = if case["affinity"] == true {
            3 - processed
        } else {
            3
        };
        assert_eq!(affinity.affinity_region_count(), expected, "{case}");
        assert_eq!(affinity.groups["g"].info.region_count, expected);
        label_stats.clear_defunct_regions();
        assert_eq!(
            label_stats.label_counter()["none"],
            (3 - processed) as i64,
            "{case}"
        );
        // Repeating cleanup must not underflow affinity/label counts.
        handle_overlaps(
            &regions[..processed],
            None,
            &mut label_stats,
            &mut rules,
            (case["affinity"] == true).then_some(&mut affinity),
            || false,
        )
        .unwrap();
        label_stats.clear_defunct_regions();
        assert_eq!(label_stats.label_counter()["none"], (3 - processed) as i64);
        assert_eq!(affinity.affinity_region_count(), expected);
        assert_eq!(
            handle_overlaps(&[], None, &mut label_stats, &mut rules, None, || true),
            Ok(0)
        );
    }
}
