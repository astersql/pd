// Copyright 2026 AsterSQL.
use super::{BucketRegionNotFound, bucket_heartbeat::retry_bucket_update, *};
use astersql_pd_core::*;
use astersql_pd_statistics_buckets::*;
use prometheus::Registry;
use std::sync::{Arc, Barrier, Mutex};
use tokio::{
    runtime::Handle,
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;

fn report(version: u64) -> BucketReport {
    BucketReport {
        region_id: 1,
        version,
        keys: vec![b"a".to_vec(), b"d".to_vec()],
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: vec![version * 100_000],
            read_keys: vec![0],
            read_qps: vec![0],
            write_bytes: vec![0],
            write_keys: vec![0],
            write_qps: vec![0],
        },
    }
}
fn region(version: u64) -> RegionInfo {
    let peer = Peer {
        id: 11,
        store_id: 1,
        ..Default::default()
    };
    RegionInfo::new(
        RegionMeta {
            id: 1,
            peers: vec![peer.clone()],
            key_range: KeyRange {
                start_key: b"a".to_vec(),
                end_key: b"d".to_vec(),
            },
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 1,
            }),
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    )
}
fn cluster() -> BasicCluster {
    let mut c = BasicCluster::new();
    c.stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
        id: 1,
        node_state: NodeState::Serving,
        ..Default::default()
    })));
    handle_region_heartbeat(&mut c, region(1), true).unwrap();
    c
}
fn version(c: &BasicCluster) -> u64 {
    c.regions
        .get_region(1)
        .unwrap()
        .report_buckets()
        .unwrap()
        .version
}
fn worker() -> BucketWorker {
    BucketWorker::start(
        &Handle::current(),
        &CancellationToken::new(),
        &Registry::new(),
        BucketRangeIndex::new(),
    )
    .unwrap()
}
async fn collect(client: &BucketWorkerClient) -> BucketStats {
    timeout(
        Duration::from_secs(5),
        client
            .collect(-20, vec![])
            .unwrap()
            .wait(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap()
}
#[tokio::test]
async fn state_task_45_bucket_heartbeat_missing_removed_never_enqueue() {
    let mut worker = worker();
    let client = worker.client();
    let mut c = BasicCluster::new();
    assert_eq!(
        handle_bucket_heartbeat(&mut c, Some(&client), report(1)),
        Err(BucketRegionNotFound(1))
    );
    assert_eq!(BucketRegionNotFound(1).to_string(), "region 1 not found");
    assert!(collect(&client).await.is_empty());
    c = cluster();
    c.stores.patch_store(1, |s| {
        s.edit_meta(|m| {
            m.state = StoreState::Tombstone;
            m.node_state = NodeState::Removed;
        })
    });
    let origin = c.regions.get_region(1).unwrap();
    assert_eq!(
        handle_bucket_heartbeat(&mut c, Some(&client), report(2)),
        Ok(false)
    );
    assert!(Arc::ptr_eq(&origin, &c.regions.get_region(1).unwrap()));
    assert!(origin.report_buckets().is_none());
    assert!(collect(&client).await.is_empty());
    worker.shutdown().await.unwrap();
    assert!(matches!(
        client.collect(0, vec![]),
        Err(BucketSubmitError::Closed)
    ));
}
#[tokio::test]
async fn state_task_45_bucket_heartbeat_versions_and_worker_payload() {
    let mut worker = worker();
    let client = worker.client();
    let mut c = cluster();
    for (incoming, expected) in [(2, 2), (2, 2), (1, 2), (3, 3)] {
        let origin = c.regions.get_region(1).unwrap();
        assert_eq!(
            handle_bucket_heartbeat(&mut c, Some(&client), report(incoming)),
            Ok(true)
        );
        assert_eq!(version(&c), expected);
        if incoming <= origin.report_buckets().map_or(0, |b| b.version) {
            assert!(Arc::ptr_eq(&origin, &c.regions.get_region(1).unwrap()));
        }
        // Even equal/stale reports are admitted by Go's CAS success contract.
        let stats = collect(&client).await;
        assert_eq!(stats[&1][0].loads, [incoming * 100_000, 0, 0, 0, 0, 0]);
        assert_eq!(stats[&1][0].interval, 1000);
        assert_eq!(stats[&1][0].start_key, b"a");
        assert_eq!(stats[&1][0].end_key, b"d");
        let root = c.regions.get_region(1).unwrap();
        let sub = c
            .regions
            .role_tree(RegionRole::Leader, 1)
            .unwrap()
            .search(b"a")
            .unwrap();
        assert!(Arc::ptr_eq(&root, &sub));
        assert_eq!(c.regions.store_stats(1).leader, 1);
        assert_eq!(c.regions.tree_len(), 1);
        assert_eq!(root.get_ref(), 2);
        if incoming > origin.report_buckets().map_or(0, |b| b.version) {
            assert_eq!(origin.get_ref(), 0);
        }
    }
    worker.shutdown().await.unwrap();
}
#[test]
fn state_task_45_bucket_heartbeat_compatibility_and_stale_metadata() {
    let mut c = cluster();
    assert_eq!(handle_bucket_heartbeat(&mut c, None, report(2)), Ok(true));
    handle_region_heartbeat(&mut c, region(2), true).unwrap();
    assert_eq!(version(&c), 2);
    assert_eq!(handle_bucket_heartbeat(&mut c, None, report(3)), Ok(true));
    let mut split = region(3).clone_with([RegionOption::StartKey(b"c".to_vec())]);
    handle_region_heartbeat(&mut c, split.clone(), true).unwrap();
    assert_eq!(
        c.regions.get_region(1).unwrap().buckets().unwrap().keys[0],
        b"a"
    );
    split = split.clone_with([RegionOption::IncVersion]);
    split.set_bucket_meta(Some(RegionBucketMeta {
        version: 4,
        keys: vec![b"c".to_vec(), b"d".to_vec()],
    }));
    handle_region_heartbeat(&mut c, split, true).unwrap();
    assert_eq!(handle_bucket_heartbeat(&mut c, None, report(5)), Ok(true));
    let r = c.regions.get_region(1).unwrap();
    assert_eq!(r.buckets().unwrap().version, 4);
    assert_eq!(
        r.buckets().unwrap().keys,
        vec![b"c".to_vec(), b"d".to_vec()]
    );
    assert_eq!(r.report_buckets().unwrap().version, 5);
    handle_region_heartbeat(&mut c, region(5), false).unwrap();
    assert!(c.regions.get_region(1).unwrap().buckets().is_none());
}
#[test]
fn state_task_45_bucket_heartbeat_competing_versions() {
    for order in [[3, 2], [2, 3]] {
        let c = Arc::new(Mutex::new(cluster()));
        let barrier = Arc::new(Barrier::new(2));
        let first_c = c.clone();
        let first_barrier = barrier.clone();
        let first = std::thread::spawn(move || {
            let result =
                handle_bucket_heartbeat(&mut first_c.lock().unwrap(), None, report(order[0]));
            first_barrier.wait();
            assert_eq!(result, Ok(true));
        });
        let second_c = c.clone();
        let second = std::thread::spawn(move || {
            barrier.wait();
            assert_eq!(
                handle_bucket_heartbeat(&mut second_c.lock().unwrap(), None, report(order[1])),
                Ok(true)
            );
        });
        let first = first.join();
        let second = second.join();
        first.unwrap();
        second.unwrap();
        assert_eq!(version(&c.lock().unwrap()), 3);
    }
}
#[test]
fn state_task_45_bucket_heartbeat_three_attempt_boundary_real_core_cas() {
    for conflicts in 0..=3 {
        let mut r = region(1);
        let initial = RegionBuckets {
            version: 1,
            ..Default::default()
        };
        assert!(r.update_buckets(Some(&initial), None));
        let stale = r.report_buckets().unwrap().clone();
        let concurrent = RegionBuckets {
            version: 2,
            ..Default::default()
        };
        assert!(r.update_buckets(Some(&concurrent), Some(&stale)));
        let current = r.report_buckets().unwrap().clone();
        let candidate = RegionBuckets {
            version: 3,
            ..Default::default()
        };
        let mut attempts = 0;
        let applied = retry_bucket_update(|| {
            attempts += 1;
            let expected = if attempts <= conflicts {
                &stale
            } else {
                &current
            };
            r.update_buckets(Some(&candidate), Some(expected))
        });
        assert_eq!(applied, conflicts < 3);
        assert_eq!(attempts, (conflicts + 1).min(3));
        assert_eq!(
            r.report_buckets().unwrap().version,
            if applied { 3 } else { 2 }
        );
    }
}
#[tokio::test]
async fn state_task_45_bucket_heartbeat_closed_worker_keeps_commit() {
    let mut worker = worker();
    let client = worker.client();
    worker.shutdown().await.unwrap();
    let mut c = cluster();
    assert_eq!(
        handle_bucket_heartbeat(&mut c, Some(&client), report(7)),
        Ok(true)
    );
    assert_eq!(version(&c), 7);
    worker.shutdown().await.unwrap();
}
#[test]
fn state_task_45_bucket_heartbeat_missing_leader_store_allowed() {
    let mut c = BasicCluster::new();
    c.regions.put_region(Arc::new(region(1)));
    assert_eq!(handle_bucket_heartbeat(&mut c, None, report(1)), Ok(true));
    let no_leader = region(2).clone_with([RegionOption::Leader(None)]);
    c.regions.put_region(Arc::new(no_leader));
    assert_eq!(handle_bucket_heartbeat(&mut c, None, report(2)), Ok(true));
    assert_eq!(version(&c), 2);
}
