// Copyright 2026 AsterSQL.
use crate::{RegionMetaError, StorageEndpoint};
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionEpoch, RegionMeta};
use astersql_pd_election::Leadership;
use astersql_pd_encryption::{
    DataKey, DictionaryStorage, EncryptionMethod, KeyDictionary, MasterKeyMeta,
};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Consistency, MetaStore, Mutation, Read};
use prost::Message;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn mcs_015_storage_uses_same_cipher_for_point_scan_and_writes() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs015-endpoint-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _dir = Directory(root.clone());
    let file = root.join("master");
    std::fs::write(
        &file,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1502,
        b"storage-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let keys = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(crate::encryption_dictionary::ProtoKeyDictionaryCodec),
    ));
    keys.save(
        &term,
        &MasterKeyMeta::File {
            path: file.to_str().unwrap().into(),
        },
        KeyDictionary {
            current_key_id: 7,
            keys: std::collections::BTreeMap::from([(
                7,
                DataKey {
                    key: vec![7; 24],
                    method: EncryptionMethod::Aes192Ctr,
                    creation_time: 7,
                    was_exposed: false,
                },
            )]),
        },
    )
    .await
    .unwrap();
    keys.load().await.unwrap();
    let mut endpoint = StorageEndpoint::new(store.clone()).with_encryption_keys(Some(keys.clone()));
    let paths = ClusterPaths::new(1502);
    let original = RegionMeta {
        id: 100,
        key_range: KeyRange {
            start_key: b"a non-block-aligned start".to_vec(),
            end_key: vec![],
        },
        epoch: Some(RegionEpoch {
            version: 9,
            conf_ver: 8,
        }),
        peers: vec![Peer {
            id: 1,
            store_id: 2,
            role: PeerRole::Unknown(7),
            is_witness: true,
        }],
        is_in_flashback: true,
        flashback_start_ts: u64::MAX,
        ..Default::default()
    };
    endpoint.save_region(&paths, &original).await.unwrap();
    assert!(original.encryption_meta.is_none());
    let row = store
        .read(Read::range(
            paths.region_path(100).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    let wire = astersql_pd_proto::metapb::Region::decode(row.value.as_slice()).unwrap();
    assert_eq!(wire.encryption_meta.as_ref().unwrap().key_id, 7);
    assert_eq!(wire.encryption_meta.as_ref().unwrap().iv.len(), 16);
    assert_ne!(wire.start_key, original.key_range.start_key);
    assert!(wire.end_key.is_empty());
    assert_eq!(
        endpoint.load_region(&paths, 100).await.unwrap(),
        Some(original.clone())
    );
    let mut recovered = vec![];
    endpoint
        .load_regions(&paths, &CancellationToken::new(), |region| {
            recovered.push(region.meta().clone());
            vec![]
        })
        .await
        .unwrap();
    assert_eq!(recovered, vec![original.clone()]);
    let plain = StorageEndpoint::new(store.clone());
    assert!(matches!(
        plain.load_region(&paths, 100).await.unwrap_err().source,
        RegionMetaError::Encrypted
    ));
    let mut bad = wire;
    bad.id = 200;
    bad.encryption_meta.as_mut().unwrap().key_id = 999;
    store
        .transact(
            vec![],
            vec![Mutation::put(
                paths.region_path(200).as_bytes(),
                &bad.encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    let error = endpoint.load_region(&paths, 200).await.unwrap_err();
    assert!(error.found);
    assert_eq!(
        error.source.to_string(),
        "[PD:encryption:ErrEncryptionKeyNotFound]keyId = 999"
    );
    let mut empty = original.clone();
    empty.id = 300;
    empty.key_range = KeyRange::default();
    endpoint.save_region(&paths, &empty).await.unwrap();
    assert_eq!(
        endpoint.load_region(&paths, 300).await.unwrap(),
        Some(empty)
    );
    term.resign().await.unwrap();
}
