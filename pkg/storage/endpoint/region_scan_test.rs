// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{RegionInfo, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore, Mutation};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn contract(store: Arc<dyn MetaStore>) {
    let paths = ClusterPaths::new(827);
    let base = Base::new(store.as_ref());
    // Existing Go bytes for Region{id:99,start:[0,1],end:[255]}.
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../proto/wire-fixtures.json")).unwrap();
    let hex = fixtures["region_heartbeat"].as_str().unwrap();
    let bytes: Vec<u8> = (40..58)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    base.save(paths.region_path(99).as_bytes(), &bytes)
        .await
        .unwrap();
    let mut ep = StorageEndpoint::new(store.clone());
    let cancel = CancellationToken::new();
    let mut seen = vec![];
    ep.load_regions(&paths, &cancel, |r| {
        seen.push(r.meta().clone());
        vec![]
    })
    .await
    .unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].id, 99);
    assert_eq!(seen[0].key_range.start_key, [0, 1]);
    assert_eq!(ep.next_region_id(), 100);
    ep.delete_region(&paths, &seen[0]).await.unwrap();
    // Empty bytes decode as a default Region in the range API (point load is absent).
    base.save(paths.region_path(0).as_bytes(), b"")
        .await
        .unwrap();
    let mut ep = StorageEndpoint::new(store.clone());
    seen.clear();
    ep.load_regions(&paths, &cancel, |r| {
        seen.push(r.meta().clone());
        vec![]
    })
    .await
    .unwrap();
    assert_eq!(seen, [RegionMeta::default()]);
    ep.delete_region(&paths, &RegionMeta::default())
        .await
        .unwrap();
    // Cross both 100 and the actual 10000 initial page boundary.
    for first in (1..=10001u64).step_by(100) {
        let mut writes = vec![];
        for id in first..=(first + 99).min(10001) {
            // Known protobuf uint64 wire layout, independent of the implementation codec.
            let mut value = vec![8];
            let mut n = id;
            while n >= 128 {
                value.push((n as u8 & 127) | 128);
                n >>= 7;
            }
            value.push(n as u8);
            writes.push(Mutation::put(paths.region_path(id).as_bytes(), &value));
        }
        store.transact(vec![], writes).await.unwrap();
    }
    ep.save_region(
        &paths,
        &RegionMeta {
            id: u64::MAX,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut ep = StorageEndpoint::new(store.clone());
    let stop = CancellationToken::new();
    let mut ids = vec![];
    let error = ep
        .load_regions(&paths, &stop, |r| {
            ids.push(r.meta().id);
            if ids.len() == 1 {
                stop.cancel();
            }
            if r.meta().id == 2 {
                vec![RegionInfo::new(
                    RegionMeta {
                        id: 1,
                        ..Default::default()
                    },
                    None,
                    vec![],
                    vec![],
                )]
            } else {
                vec![]
            }
        })
        .await
        .unwrap_err();
    assert!(matches!(error, RegionScanError::Cancelled));
    assert_eq!(ids, (1..=10000).collect::<Vec<_>>()); // Cancellation checked after range read, not per region.
    assert_eq!(ep.next_region_id(), 10001);
    assert!(ep.load_region(&paths, 1).await.unwrap().is_none());
    ep.load_regions(&paths, &cancel, |r| {
        ids.push(r.meta().id);
        vec![]
    })
    .await
    .unwrap();
    assert_eq!(ids, (1..=10001).collect::<Vec<_>>());
    assert_eq!(ep.next_region_id(), 10002);
    assert!(ep.load_region(&paths, u64::MAX).await.unwrap().is_some());
    // Bad wire fails before cursor advance; repairing it resumes the same ID.
    base.save(paths.region_path(10002).as_bytes(), b"\xff")
        .await
        .unwrap();
    assert!(matches!(
        ep.load_regions(&paths, &cancel, |_| vec![]).await,
        Err(RegionScanError::Region(RegionMetaError::Endpoint(
            EndpointError::ProtoUnmarshal(_)
        )))
    ));
    assert_eq!(ep.next_region_id(), 10002);
    ep.save_region(
        &paths,
        &RegionMeta {
            id: 10002,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut resumed = vec![];
    ep.load_regions(&paths, &cancel, |r| {
        resumed.push(r.meta().id);
        vec![]
    })
    .await
    .unwrap();
    assert_eq!(resumed, [10002]);
    let pre_cancel = CancellationToken::new();
    pre_cancel.cancel();
    assert!(matches!(
        ep.load_regions(&paths, &pre_cancel, |_| panic!("cancelled callback"))
            .await,
        Err(RegionScanError::Cancelled)
    ));
    assert_eq!(ep.next_region_id(), 10003);
}
#[tokio::test]
async fn region_scan_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn region_scan_etcd() {
    let fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
}

#[tokio::test]
async fn region_scan_etcd_adaptive_and_io_failure() {
    use astersql_pd_core::KeyRange;
    use astersql_pd_storage_kv::{Consistency, Read};
    let fixture = endpoint_codec_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let paths = ClusterPaths::new(828);
    let mut ep = StorageEndpoint::new(store.clone());
    // A real response exceeding tonic's 4 MiB receive limit, no fake storage results.
    for id in 1..=220 {
        ep.save_region(
            &paths,
            &RegionMeta {
                id,
                key_range: KeyRange {
                    start_key: vec![42; 24000],
                    end_key: vec![],
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    let mut request = Read::range(
        paths.region_path(0).as_bytes(),
        paths.region_path(u64::MAX).as_bytes(),
        Consistency::Linearizable,
    );
    request.limit = 10000;
    assert!(
        store.read(request).await.is_err(),
        "oversized real response must fail"
    );
    let cancel = CancellationToken::new();
    let mut ids = vec![];
    ep.load_regions(&paths, &cancel, |r| {
        assert_eq!(r.meta().key_range.start_key.len(), 24000);
        ids.push(r.meta().id);
        vec![]
    })
    .await
    .unwrap();
    assert_eq!(ids, (1..=220).collect::<Vec<_>>());
    assert_eq!(ep.next_region_id(), 221);
    // Stop the actual server inside a delivered callback, before overlap deletion.
    let paths = ClusterPaths::new(829);
    ep.save_region(
        &paths,
        &RegionMeta {
            id: 1,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut ep = StorageEndpoint::new(store);
    let mut fixture = Some(fixture);
    let error = ep
        .load_regions(&paths, &cancel, |r| {
            drop(fixture.take());
            vec![r]
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RegionScanError::Region(RegionMetaError::Endpoint(EndpointError::Store(_)))
    ));
    assert_eq!(ep.next_region_id(), 2);
    // Read errors are retried down through 156; all fail on the closed server.
    let error = ep
        .load_regions(&paths, &cancel, |_| panic!("unavailable backend callback"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        RegionScanError::Region(RegionMetaError::Endpoint(EndpointError::Store(_)))
    ));
    assert_eq!(ep.next_region_id(), 2);
}
