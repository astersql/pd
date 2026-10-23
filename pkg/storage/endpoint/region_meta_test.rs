// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionEncryptionMeta, RegionEpoch, RegionMeta};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

fn sample() -> RegionMeta {
    RegionMeta {
        id: 99,
        key_range: KeyRange {
            start_key: vec![0, 1],
            end_key: vec![255],
        },
        ..Default::default()
    }
}
fn complete() -> RegionMeta {
    RegionMeta {
        id: u64::MAX,
        key_range: KeyRange {
            start_key: vec![255, 0, 128],
            end_key: vec![0],
        },
        epoch: Some(RegionEpoch {
            conf_ver: u64::MAX,
            version: u64::MAX,
        }),
        peers: [
            PeerRole::Voter,
            PeerRole::Learner,
            PeerRole::IncomingVoter,
            PeerRole::DemotingVoter,
            PeerRole::Unknown(-1),
            PeerRole::Unknown(i32::MAX),
        ]
        .into_iter()
        .map(|role| Peer {
            id: u64::MAX,
            store_id: 0,
            role,
            is_witness: true,
        })
        .chain([Peer::default(), Peer::default()])
        .collect(),
        is_in_flashback: true,
        flashback_start_ts: u64::MAX,
        ..Default::default()
    }
}
// Extract the raw Region submessage from the EXISTING Go-generated fixture,
// never round-tripping through Rust encoding to construct expected bytes.
fn go_region_bytes() -> Vec<u8> {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("../../proto/wire-fixtures.json")).unwrap();
    let hex = fixtures["region_heartbeat"].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    // Go fixture: field 1 header (16 bytes), then field 2 Region (9 bytes).
    assert_eq!(&bytes[..2], &[0x0a, 16]);
    assert_eq!(&bytes[18..20], &[0x12, 9]);
    bytes[20..29].to_vec()
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    let key = paths.region_path(99);
    assert_eq!(ep.load_region(&paths, 99).await.unwrap(), None);
    base.save(key.as_bytes(), b"").await.unwrap();
    assert_eq!(ep.load_region(&paths, 99).await.unwrap(), None);
    ep.save_region(&paths, &sample()).await.unwrap();
    assert_eq!(base.load(key.as_bytes()).await.unwrap(), go_region_bytes());
    base.save(key.as_bytes(), &go_region_bytes()).await.unwrap();
    assert_eq!(ep.load_region(&paths, 99).await.unwrap(), Some(sample()));
    assert_eq!(
        ep.load_region(&ClusterPaths::new(43), 99).await.unwrap(),
        None
    );
    for meta in [
        complete(),
        RegionMeta {
            id: 99,
            epoch: Some(RegionEpoch::default()),
            ..Default::default()
        },
        RegionMeta {
            id: 99,
            flashback_start_ts: 1,
            ..Default::default()
        },
    ] {
        ep.save_region(&paths, &meta).await.unwrap();
        assert_eq!(ep.load_region(&paths, meta.id).await.unwrap(), Some(meta));
    }
    let mut unknown = go_region_bytes();
    unknown.extend_from_slice(&[0x48, 1]);
    base.save(key.as_bytes(), &unknown).await.unwrap();
    assert_eq!(ep.load_region(&paths, 99).await.unwrap(), Some(sample()));
    for bytes in [
        b"\xff".as_slice(),
        b"\x08",
        b"\x0a\x00",
        b"\x22\x01\x80",
        b"\x2a\x01\x80",
        b"\x32\x01\x80",
    ] {
        base.save(key.as_bytes(), bytes).await.unwrap();
        let e = ep.load_region(&paths, 99).await.unwrap_err();
        assert!(e.found);
        assert!(matches!(
            e.source,
            RegionMetaError::Endpoint(EndpointError::ProtoUnmarshal(_))
        ));
    }
    // Presence of even empty encryption metadata is rejected.
    for encryption in [b"\x32\x00".as_slice(), b"\x32\x05\x08\x07\x12\x01x"] {
        let mut bytes = go_region_bytes();
        bytes.extend_from_slice(encryption);
        base.save(key.as_bytes(), &bytes).await.unwrap();
        let e = ep.load_region(&paths, 99).await.unwrap_err();
        assert!(e.found);
        assert!(matches!(e.source, RegionMetaError::Encrypted));
        let mut encrypted = sample();
        encrypted.encryption_meta = Some(RegionEncryptionMeta::default());
        assert!(matches!(
            ep.save_region(&paths, &encrypted).await,
            Err(RegionMetaError::Encrypted)
        ));
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), bytes);
    }
    ep.save_region(&paths, &sample()).await.unwrap();
    ep.delete_region(&paths, &sample()).await.unwrap();
    ep.delete_region(&paths, &sample()).await.unwrap();
    assert_eq!(ep.load_region(&paths, 99).await.unwrap(), None);
    assert_eq!(
        ep.load_region(&paths, u64::MAX).await.unwrap(),
        Some(complete())
    );
    ep.save_region(&paths, &RegionMeta::default())
        .await
        .unwrap();
    assert_eq!(
        base.load(paths.region_path(0).as_bytes()).await.unwrap(),
        b""
    );
    assert_eq!(ep.load_region(&paths, 0).await.unwrap(), None);
}
#[tokio::test]
async fn region_meta_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn region_meta_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(99);
    StorageEndpoint::new(Arc::new(fixture.store().await))
        .save_region(&paths, &complete())
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        ep.load_region(&paths, u64::MAX).await.unwrap(),
        Some(complete())
    );
    ep.delete_region(&paths, &complete()).await.unwrap();
    assert_eq!(ep.load_region(&paths, u64::MAX).await.unwrap(), None);
    drop(fixture);
    let e = ep.load_region(&paths, 99).await.unwrap_err();
    assert!(!e.found);
    assert!(matches!(
        e.source,
        RegionMetaError::Endpoint(EndpointError::Store(_))
    ));
}
