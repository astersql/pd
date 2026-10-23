// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{NodeState, StoreLabel, StoreMeta, StoreState, TxnProtocolVersionRange};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

// Actual Go TestLoadClusterInfo etcd value for metapb.Store{Id:1}.
const GO_STORE_1: &[u8] = &[0x08, 0x01];

fn complete() -> StoreMeta {
    StoreMeta {
        id: u64::MAX,
        address: "client:20160".into(),
        peer_address: "peer:20160".into(),
        status_address: "status:20180".into(),
        git_hash: "abc123".into(),
        version: "9.0.0".into(),
        deploy_path: "/data/节点".into(),
        start_timestamp: i64::MIN,
        last_heartbeat: i64::MAX,
        physically_destroyed: true,
        state: StoreState::Offline,
        node_state: NodeState::Preparing,
        labels: vec![
            StoreLabel {
                key: "zone".into(),
                value: "a".into(),
            },
            StoreLabel {
                key: "zone".into(),
                value: "b".into(),
            },
            StoreLabel::default(),
        ],
        txn_protocol_version_range: Some(TxnProtocolVersionRange {
            min: u32::MAX,
            max: 0,
        }),
    }
}
async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let key = paths.store_path(1);
    let one = StoreMeta {
        id: 1,
        ..Default::default()
    };
    assert_eq!(endpoint.load_store_meta(&paths, 1).await.unwrap(), None);
    base.save(key.as_bytes(), b"").await.unwrap();
    assert_eq!(endpoint.load_store_meta(&paths, 1).await.unwrap(), None);
    endpoint.save_store_meta(&paths, &one).await.unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_STORE_1);
    assert_eq!(
        endpoint.load_store_meta(&paths, 1).await.unwrap(),
        Some(one.clone())
    );
    base.save(key.as_bytes(), GO_STORE_1).await.unwrap();
    assert_eq!(
        endpoint.load_store_meta(&paths, 1).await.unwrap(),
        Some(one.clone())
    );
    assert_eq!(
        endpoint
            .load_store_meta(&ClusterPaths::new(43), 1)
            .await
            .unwrap(),
        None
    );

    for meta in [
        complete(),
        StoreMeta {
            id: 0,
            txn_protocol_version_range: Some(TxnProtocolVersionRange::default()),
            ..Default::default()
        },
        StoreMeta {
            id: 1,
            state: StoreState::Unknown(-1),
            node_state: NodeState::Unknown(i32::MAX),
            ..Default::default()
        },
        StoreMeta {
            id: 1,
            state: StoreState::Tombstone,
            node_state: NodeState::Serving,
            ..Default::default()
        },
        StoreMeta {
            id: 1,
            node_state: NodeState::Removing,
            ..Default::default()
        },
        StoreMeta {
            id: 1,
            node_state: NodeState::Removed,
            ..Default::default()
        },
    ] {
        endpoint.save_store_meta(&paths, &meta).await.unwrap();
        assert_eq!(
            endpoint.load_store_meta(&paths, meta.id).await.unwrap(),
            Some(meta)
        );
    }
    // Go point reads do not apply the LoadStores Offline/Tombstone mapping.
    // Unknown fields are skipped by Go metapb.Store.Unmarshal.
    base.save(key.as_bytes(), &[0x08, 0x01, 0x78, 0x01])
        .await
        .unwrap();
    assert_eq!(
        endpoint.load_store_meta(&paths, 1).await.unwrap(),
        Some(one.clone())
    );
    for bytes in [
        b"\xff".as_slice(),
        b"\x08",
        b"\x0a\x00",
        b"\x22\x02\x0a",
        b"\x72\x01\x80",
    ] {
        base.save(key.as_bytes(), bytes).await.unwrap();
        assert!(matches!(
            endpoint.load_store_meta(&paths, 1).await,
            Err(EndpointError::ProtoUnmarshal(_))
        ));
    }
    for weight in [
        paths.store_leader_weight_path(1),
        paths.store_region_weight_path(1),
    ] {
        base.save(weight.as_bytes(), b"2.5").await.unwrap();
    }
    endpoint.save_store_meta(&paths, &one).await.unwrap();
    endpoint.delete_store_meta(&paths, &one).await.unwrap();
    endpoint.delete_store_meta(&paths, &one).await.unwrap();
    assert_eq!(endpoint.load_store_meta(&paths, 1).await.unwrap(), None);
    for weight in [
        paths.store_leader_weight_path(1),
        paths.store_region_weight_path(1),
    ] {
        assert_eq!(base.load(weight.as_bytes()).await.unwrap(), b"2.5");
    }
    assert_eq!(
        endpoint.load_store_meta(&paths, u64::MAX).await.unwrap(),
        Some(complete())
    );
    endpoint
        .save_store_meta(&paths, &StoreMeta::default())
        .await
        .unwrap();
    assert_eq!(
        base.load(paths.store_path(0).as_bytes()).await.unwrap(),
        b""
    );
    assert_eq!(endpoint.load_store_meta(&paths, 0).await.unwrap(), None);
}
#[tokio::test]
async fn store_meta_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn store_meta_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(99);
    let meta = complete();
    StorageEndpoint::new(Arc::new(fixture.store().await))
        .save_store_meta(&paths, &meta)
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        endpoint.load_store_meta(&paths, meta.id).await.unwrap(),
        Some(meta.clone())
    );
    endpoint.delete_store_meta(&paths, &meta).await.unwrap();
    assert_eq!(
        endpoint.load_store_meta(&paths, meta.id).await.unwrap(),
        None
    );
}
