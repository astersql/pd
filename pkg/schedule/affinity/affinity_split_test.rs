// Copyright 2026 AsterSQL.
use crate::{
    group::{Availability, Group},
    manager::Manager,
    ranges::{GroupKeyRanges, make_label_rule},
    split::SplitReason,
};
use astersql_pd_core::{Peer, RegionHeartbeatStats, RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyutil::KeyRange;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;
fn region(size: u64, keys: u64, leader: u64) -> Arc<RegionInfo> {
    let peer = Peer {
        id: leader,
        store_id: leader,
        ..Default::default()
    };
    let mut r = RegionInfo::new(
        RegionMeta {
            id: 1,
            key_range: astersql_pd_core::KeyRange {
                start_key: b"b".to_vec(),
                end_key: b"y".to_vec(),
            },
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    );
    r.set_heartbeat_stats(
        RegionHeartbeatStats {
            approximate_size: size * 1048576,
            approximate_keys: keys,
            ..Default::default()
        },
        0,
    );
    Arc::new(r)
}
#[tokio::test]
async fn affinity_split_thresholds_and_states() {
    let mut labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(39),
        0,
    )
    .await
    .unwrap();
    let mut rule = make_label_rule(&GroupKeyRanges {
        group_id: "g".into(),
        key_ranges: vec![KeyRange::new(b"a".to_vec(), b"z".to_vec())],
    });
    rule.check_and_adjust(0).unwrap();
    labels.set_label_rule(rule, 0).await.unwrap();
    let mut m = Manager::default();
    m.sync_group_from_etcd(
        &Group {
            id: "g".into(),
            create_timestamp: 100,
            leader_store_id: 1,
            voter_store_ids: vec![1],
        },
        &labels,
        100,
    );
    m.restore_affinity_group("g", 100);
    let mut conf = ProviderSnapshot::<String>::default();
    conf.schedule.affinity_schedule_limit = 4;
    conf.schedule.max_affinity_merge_region_size = 10;
    for (size, keys, expected) in [
        (5, 50000, false),
        (79, 799999, false),
        (80, 1, true),
        (1, 800000, true),
        (81, 810000, true),
    ] {
        for reason in [SplitReason::Size, SplitReason::Load, SplitReason::Other(99)] {
            for leader in [1, 2] {
                assert_eq!(
                    m.allow_split(
                        &labels,
                        &conf,
                        Some(&region(size, keys, leader)),
                        reason,
                        100
                    ),
                    expected,
                    "size={size} keys={keys} leader={leader} reason={reason:?}"
                );
            }
        }
    }
    let r = region(5, 50000, 1);
    assert!(m.allow_split(&labels, &conf, None, SplitReason::Size, 100));
    assert!(m.allow_split(&labels, &conf, Some(&r), SplitReason::Admin, 100));
    conf.schedule.affinity_schedule_limit = 0;
    assert!(m.allow_split(&labels, &conf, Some(&r), SplitReason::Size, 100));
    conf.schedule.affinity_schedule_limit = 4;
    conf.schedule.max_affinity_merge_region_size = 0;
    assert!(m.allow_split(&labels, &conf, Some(&r), SplitReason::Size, 100));
    conf.schedule.max_affinity_merge_region_size = 10;
    m.groups
        .get_mut("g")
        .unwrap()
        .info
        .set_availability(Availability::Degraded, 100);
    assert!(!m.allow_split(&labels, &conf, Some(&r), SplitReason::Size, 700));
    assert!(m.allow_split(&labels, &conf, Some(&r), SplitReason::Size, 701));
    m.expire_affinity_group("g", 100);
    assert!(m.allow_split(&labels, &conf, Some(&r), SplitReason::Load, 100));
    m.groups.clear();
    assert!(m.allow_split(&labels, &conf, Some(&r), SplitReason::Size, 100));
}
