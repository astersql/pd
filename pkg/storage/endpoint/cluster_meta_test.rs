// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

// Observed through etcd /v3/kv/range while the existing Go
// tests/server/cluster TestLoadClusterInfo saved metapb.Cluster{Id:123}.
const GO_CLUSTER_123: &[u8] = &[0x08, 0x7b];

async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let key = paths.cluster_path();
    assert_eq!(endpoint.load_meta(&paths).await.unwrap(), None);
    base.save(key.as_bytes(), b"").await.unwrap();
    assert_eq!(endpoint.load_meta(&paths).await.unwrap(), None);
    let meta = Cluster {
        id: 123,
        max_peer_count: 0,
    };
    endpoint.save_meta(&paths, &meta).await.unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_CLUSTER_123);
    assert_eq!(
        endpoint.load_meta(&paths).await.unwrap(),
        Some(meta.clone())
    );
    // Read actual Go wire independently of the Rust encoder.
    base.save(key.as_bytes(), GO_CLUSTER_123).await.unwrap();
    assert_eq!(endpoint.load_meta(&paths).await.unwrap(), Some(meta));
    assert_eq!(
        endpoint.load_meta(&ClusterPaths::new(43)).await.unwrap(),
        None
    );
    for meta in [
        Cluster {
            id: 0,
            max_peer_count: 3,
        },
        Cluster {
            id: u64::MAX,
            max_peer_count: u32::MAX,
        },
    ] {
        endpoint.save_meta(&paths, &meta).await.unwrap();
        assert_eq!(endpoint.load_meta(&paths).await.unwrap(), Some(meta));
    }
    // Unknown fields are skipped as in Go metapb.Cluster.Unmarshal.
    base.save(key.as_bytes(), &[0x08, 0x7b, 0x18, 0x01])
        .await
        .unwrap();
    assert_eq!(
        endpoint.load_meta(&paths).await.unwrap(),
        Some(Cluster {
            id: 123,
            max_peer_count: 0
        })
    );
    for bytes in [b"\xff".as_slice(), b"\x08", b"\x0a\x00", b"\x10\x80"] {
        base.save(key.as_bytes(), bytes).await.unwrap();
        assert!(matches!(
            endpoint.load_meta(&paths).await,
            Err(EndpointError::ProtoUnmarshal(_))
        ));
    }
    endpoint
        .save_meta(&paths, &Cluster::default())
        .await
        .unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"");
    assert_eq!(endpoint.load_meta(&paths).await.unwrap(), None);
}

#[tokio::test]
async fn cluster_meta_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn cluster_meta_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = ClusterPaths::new(99);
    let meta = Cluster {
        id: 123,
        max_peer_count: 0,
    };
    endpoint.save_meta(&paths, &meta).await.unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let store = Arc::new(fixture.store().await);
    let endpoint = StorageEndpoint::new(store.clone());
    assert_eq!(endpoint.load_meta(&paths).await.unwrap(), Some(meta));
    assert_eq!(
        Base::new(store.as_ref())
            .load(paths.cluster_path().as_bytes())
            .await
            .unwrap(),
        GO_CLUSTER_123
    );
}
