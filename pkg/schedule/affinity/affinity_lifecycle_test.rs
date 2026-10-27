// Copyright 2026 AsterSQL.
use crate::{
    availability_loop::AvailabilityLoop,
    group::{Availability, Group},
    manager::Manager,
    ranges::GroupKeyRanges,
};
use astersql_pd_core::{NodeState, StoreInfo, StoreMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_schedule_config::ProviderSnapshot;
use astersql_pd_schedule_labeler::RegionLabeler;
use astersql_pd_storage_domain::MemoryStore;
use astersql_pd_storage_domain::StorageEndpoint;
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
async fn manager() -> Manager {
    let memory = Arc::new(MemoryStore::default());
    let ep = StorageEndpoint::new(memory.clone());
    let paths = ClusterPaths::new(40);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(memory), paths.clone(), 0)
        .await
        .unwrap();
    let mut m = Manager::initialize(&ep, &paths, &labels, 0).await.unwrap();
    m.create_affinity_groups(
        &ep,
        &paths,
        &mut labels,
        &[GroupKeyRanges {
            group_id: "g".into(),
            key_ranges: vec![],
        }],
        0,
    )
    .await
    .unwrap();
    m.groups.get_mut("g").unwrap().info.group = Group {
        id: "g".into(),
        leader_store_id: 1,
        voter_store_ids: vec![1, 2],
        ..Default::default()
    };
    m.restore_affinity_group("g", 0);
    m
}
fn snapshot() -> (Vec<StoreInfo>, ProviderSnapshot<String>, i128, u64) {
    (
        vec![StoreInfo::new(StoreMeta {
            id: 2,
            node_state: NodeState::Removed,
            ..Default::default()
        })],
        ProviderSnapshot::default(),
        0,
        0,
    )
}
fn gauge(registry: &Registry, name: &str) -> f64 {
    let families = registry.gather();
    let family = families
        .iter()
        .find(|f| f.name() == "pd_affinity_status")
        .unwrap();
    assert_eq!(family.help(), "Status of the affinity manager.");
    assert_eq!(family.get_metric().len(), 4);
    family
        .get_metric()
        .iter()
        .find(|m| m.get_label()[0].value() == name)
        .unwrap()
        .get_gauge()
        .get_value()
}
#[tokio::test(start_paused = true)]
async fn affinity_lifecycle_periodic_metrics_and_two_shutdowns() {
    let registry = Registry::new();
    for _ in 0..2 {
        let token = CancellationToken::new();
        let mut task = AvailabilityLoop::start(
            &tokio::runtime::Handle::current(),
            &token,
            registry.clone(),
            Duration::from_secs(10),
            manager().await,
            snapshot,
        )
        .unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(9)).await;
        assert_eq!(
            task.manager.lock().await.groups["g"].info.availability(0),
            Availability::Available
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            task.manager.lock().await.groups["g"].info.availability(0),
            Availability::Expired
        );
        assert_eq!(gauge(&registry, "group-count"), 1.0);
        assert_eq!(gauge(&registry, "region-count"), 0.0);
        assert_eq!(gauge(&registry, "affinity-region-count"), 0.0);
        assert_eq!(gauge(&registry, "split-deny-count"), 0.0);
        token.cancel();
        task.shutdown().await.unwrap();
        task.shutdown().await.unwrap();
        assert!(registry.gather().is_empty());
    }
}
#[tokio::test(start_paused = true)]
async fn affinity_lifecycle_cancel_waiting_lock_and_registration_failure() {
    let registry = Registry::new();
    let token = CancellationToken::new();
    let mut task = AvailabilityLoop::start(
        &tokio::runtime::Handle::current(),
        &token,
        registry.clone(),
        Duration::from_secs(10),
        manager().await,
        snapshot,
    )
    .unwrap();
    assert!(
        AvailabilityLoop::start(
            &tokio::runtime::Handle::current(),
            &token,
            registry.clone(),
            Duration::from_secs(10),
            manager().await,
            snapshot
        )
        .is_err()
    );
    let shared = task.manager.clone();
    let guard = shared.lock().await;
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    token.cancel();
    task.shutdown().await.unwrap();
    drop(guard);
    assert!(registry.gather().is_empty());
    assert!(
        AvailabilityLoop::start(
            &tokio::runtime::Handle::current(),
            &token,
            registry.clone(),
            Duration::ZERO,
            manager().await,
            snapshot
        )
        .is_err()
    );
    assert!(registry.gather().is_empty());
}
#[tokio::test(start_paused = true)]
async fn affinity_lifecycle_split_deny_metric() {
    use crate::{ranges::make_label_rule, split::SplitReason};
    use astersql_pd_core::{RegionInfo, RegionMeta};
    use astersql_pd_keyutil::KeyRange;
    let mut labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(401),
        0,
    )
    .await
    .unwrap();
    labels
        .set_label_rule(
            make_label_rule(&GroupKeyRanges {
                group_id: "g".into(),
                key_ranges: vec![KeyRange::new(vec![], vec![])],
            }),
            0,
        )
        .await
        .unwrap();
    let registry = Registry::new();
    let token = CancellationToken::new();
    let mut task = AvailabilityLoop::start(
        &tokio::runtime::Handle::current(),
        &token,
        registry.clone(),
        Duration::from_secs(10),
        manager().await,
        snapshot,
    )
    .unwrap();
    let mut conf = ProviderSnapshot::<String>::default();
    conf.schedule.affinity_schedule_limit = 4;
    conf.schedule.max_affinity_merge_region_size = 10;
    let region = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 1,
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    ));
    let m = task.manager.lock().await;
    assert!(!m.allow_split(&labels, &conf, Some(&region), SplitReason::Size, 0));
    assert!(m.allow_split(&labels, &conf, Some(&region), SplitReason::Admin, 0));
    assert_eq!(gauge(&registry, "split-deny-count"), 1.0);
    drop(m);
    task.shutdown().await.unwrap();
}
#[tokio::test(start_paused = true)]
async fn affinity_lifecycle_initialization_failure_and_drop_cleanup() {
    use crate::ranges::make_label_rule;
    use astersql_pd_keyutil::KeyRange;
    let memory = Arc::new(MemoryStore::default());
    let ep = StorageEndpoint::new(memory.clone());
    let paths = ClusterPaths::new(402);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(memory), paths.clone(), 0)
        .await
        .unwrap();
    ep.run_in_txn(async |txn| {
        for id in ["a", "b"] {
            ep.save_affinity_group(
                txn,
                &paths,
                id,
                &Group {
                    id: id.into(),
                    ..Default::default()
                },
            )?;
        }
        Ok::<_, astersql_pd_storage_domain::EndpointError>(())
    })
    .await
    .unwrap();
    for id in ["a", "b"] {
        labels
            .set_label_rule(
                make_label_rule(&GroupKeyRanges {
                    group_id: id.into(),
                    key_ranges: vec![KeyRange::new(vec![1], vec![3])],
                }),
                0,
            )
            .await
            .unwrap();
    }
    let registry = Registry::new();
    let token = CancellationToken::new();
    let result = AvailabilityLoop::initialize(
        &tokio::runtime::Handle::current(),
        &token,
        registry.clone(),
        Duration::from_secs(10),
        Manager::initialize(&ep, &paths, &labels, 0),
        snapshot,
    )
    .await;
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("key range overlaps")
    );
    assert!(registry.gather().is_empty());
    let task = AvailabilityLoop::start(
        &tokio::runtime::Handle::current(),
        &token,
        registry.clone(),
        Duration::from_secs(10),
        manager().await,
        snapshot,
    )
    .unwrap();
    drop(task);
    tokio::task::yield_now().await;
    assert!(registry.gather().is_empty());
}

#[tokio::test(start_paused = true)]
async fn affinity_lifecycle_nonzero_cache_metrics_and_deletion() {
    use crate::ranges::make_label_rule;
    use astersql_pd_core::{Peer, RegionInfo, RegionMeta};
    use astersql_pd_keyutil::KeyRange;
    let mut labels = RegionLabeler::load(
        StorageEndpoint::new(Arc::new(MemoryStore::default())),
        ClusterPaths::new(403),
        0,
    )
    .await
    .unwrap();
    labels
        .set_label_rule(
            make_label_rule(&GroupKeyRanges {
                group_id: "g".into(),
                key_ranges: vec![KeyRange::new(vec![], vec![])],
            }),
            0,
        )
        .await
        .unwrap();
    let registry = Registry::new();
    let token = CancellationToken::new();
    let mut task = AvailabilityLoop::start(
        &tokio::runtime::Handle::current(),
        &token,
        registry.clone(),
        Duration::from_secs(10),
        manager().await,
        || (vec![], ProviderSnapshot::<String>::default(), 0, 0),
    )
    .unwrap();
    let peers: Vec<_> = [1, 2]
        .into_iter()
        .map(|id| Peer {
            id,
            store_id: id,
            ..Default::default()
        })
        .collect();
    let r = Arc::new(RegionInfo::new(
        RegionMeta {
            id: 9,
            peers: peers.clone(),
            ..Default::default()
        },
        Some(peers[0].clone()),
        vec![],
        vec![],
    ));
    assert!(
        task.manager
            .lock()
            .await
            .get_and_cache_region_affinity_group_state(&labels, Some(&r), 0)
            .1
    );
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(gauge(&registry, "region-count"), 1.0);
    assert_eq!(gauge(&registry, "affinity-region-count"), 1.0);
    task.manager.lock().await.invalid_cache(9);
    tokio::time::advance(Duration::from_secs(10)).await;
    tokio::task::yield_now().await;
    assert_eq!(gauge(&registry, "region-count"), 0.0);
    assert_eq!(gauge(&registry, "affinity-region-count"), 0.0);
    task.shutdown().await.unwrap();
}
