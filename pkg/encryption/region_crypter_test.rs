// Copyright 2026 AsterSQL.
use crate::etcd_fixture as fixture;
use crate::protocol_adapter::ProtoKeyDictionaryCodec;
use crate::{
    DataKey, DictionaryStorage, EncryptionMethod, KeyDictionary, MasterKeyMeta, decrypt_region,
    encrypt_region,
};
use astersql_pd_core::{KeyRange, Peer, PeerRole, RegionEpoch, RegionMeta};
use astersql_pd_election::Leadership;
use astersql_pd_storage_domain::MetaStore;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn region() -> RegionMeta {
    RegionMeta {
        id: u64::MAX,
        key_range: KeyRange {
            start_key: b"start-key-17-bytes".to_vec(),
            end_key: b"end-after-partial-block".to_vec(),
        },
        epoch: Some(RegionEpoch {
            version: 7,
            conf_ver: 3,
        }),
        peers: vec![Peer {
            id: 2,
            store_id: 3,
            role: PeerRole::Learner,
            is_witness: true,
        }],
        is_in_flashback: true,
        flashback_start_ts: u64::MAX,
        ..Default::default()
    }
}
#[tokio::test]
async fn mcs_015_behavior() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let meta = MasterKeyMeta::File {
        path: file.to_str().unwrap().into(),
    };
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1500,
        b"region-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let storage = DictionaryStorage::new(store, Arc::new(ProtoKeyDictionaryCodec));
    let dictionary = KeyDictionary {
        current_key_id: 2,
        keys: std::collections::BTreeMap::from([
            (
                1,
                DataKey {
                    key: vec![1; 16],
                    method: EncryptionMethod::Aes128Ctr,
                    creation_time: 1599608041,
                    was_exposed: false,
                },
            ),
            (
                2,
                DataKey {
                    key: vec![2; 32],
                    method: EncryptionMethod::Aes256Ctr,
                    creation_time: 1599608042,
                    was_exposed: false,
                },
            ),
        ]),
    };
    storage.save(&term, &meta, dictionary).await.unwrap();
    storage.load().await.unwrap();
    let input = region();
    let original = input.clone();
    let mut encrypted = encrypt_region(Some(&input), Some(&storage))
        .unwrap()
        .into_owned();
    assert!(
        encrypted.encryption_meta.is_some(),
        "enabled current key must encrypt region bounds and attach key identity"
    );
    assert_eq!(encrypted.encryption_meta.as_ref().unwrap().key_id, 2);
    assert_eq!(input, original);
    assert_ne!(encrypted.key_range, input.key_range);
    let mut only_bounds = encrypted.clone();
    only_bounds.key_range = input.key_range.clone();
    only_bounds.encryption_meta = None;
    assert_eq!(only_bounds, input);
    decrypt_region(Some(&mut encrypted), Some(&storage))
        .await
        .unwrap();
    assert_eq!(encrypted, input);
    term.resign().await.unwrap();
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn mcs_015_saved_go_ctr_vectors_span_both_bounds() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/mcs-015-go-ctr.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let key = DataKey {
            key: unhex(case["key"].as_str().unwrap()),
            method: EncryptionMethod::from_value(case["method"].as_i64().unwrap() as i32),
            creation_time: 0,
            was_exposed: false,
        };
        let iv = unhex(case["iv"].as_str().unwrap());
        let input = unhex(case["plaintext"].as_str().unwrap());
        let expected = unhex(case["ciphertext"].as_str().unwrap());
        for split in [0, 1, 15, 16, 17, 63, 64] {
            let mut region = region();
            region.key_range = KeyRange {
                start_key: input[..split].to_vec(),
                end_key: input[split..].to_vec(),
            };
            let original = region.clone();
            crate::region_crypter::process_region_keys(&mut region, &key, &iv).unwrap();
            assert_eq!(
                [
                    region.key_range.start_key.as_slice(),
                    region.key_range.end_key.as_slice()
                ]
                .concat(),
                expected
            );
            crate::region_crypter::process_region_keys(&mut region, &key, &iv).unwrap();
            assert_eq!(region, original);
        }
    }
}
#[tokio::test]
async fn mcs_015_nil_disabled_and_error_order() {
    assert_eq!(
        encrypt_region(None, None).unwrap_err().to_string(),
        "[PD:encryption:ErrEncryptionEncryptRegion]trying to encrypt nil region"
    );
    assert_eq!(
        decrypt_region(None, None).await.unwrap_err().to_string(),
        "[PD:encryption:ErrEncryptionDecryptRegion]trying to decrypt nil region"
    );
    let original = region();
    let plain = encrypt_region(Some(&original), None).unwrap();
    assert!(matches!(plain, std::borrow::Cow::Borrowed(_)));
    assert!(std::ptr::eq(plain.as_ref(), &original));
    let mut encrypted = original.clone();
    encrypted.encryption_meta = Some(astersql_pd_core::RegionEncryptionMeta {
        key_id: 9,
        iv: vec![],
    });
    assert_eq!(
        encrypt_region(Some(&encrypted), None)
            .unwrap_err()
            .to_string(),
        format!(
            "[PD:encryption:ErrEncryptionEncryptRegion]region already encrypted, region id = {}",
            encrypted.id
        )
    );
    let before = encrypted.clone();
    assert_eq!(
        decrypt_region(Some(&mut encrypted), None)
            .await
            .unwrap_err()
            .to_string(),
        "[PD:encryption:ErrEncryptionDecryptRegion]unable to decrypt region without encryption keys"
    );
    assert_eq!(encrypted, before);
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let storage = DictionaryStorage::new(store, Arc::new(ProtoKeyDictionaryCodec));
    assert!(matches!(
        encrypt_region(Some(&original), Some(&storage)).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(
        decrypt_region(Some(&mut encrypted), Some(&storage))
            .await
            .unwrap_err()
            .to_string(),
        "[PD:encryption:ErrEncryptionKeyNotFound]keyId = 9"
    );
    assert_eq!(encrypted, before);
}
#[tokio::test]
async fn mcs_015_historical_key_cache_miss_and_validation() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let meta = MasterKeyMeta::File {
        path: file.to_str().unwrap().into(),
    };
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1501,
        b"region-owner".to_vec(),
        60,
    )
    .await
    .unwrap();
    let writer = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    let reader = DictionaryStorage::new(store, Arc::new(ProtoKeyDictionaryCodec));
    let mut dict = KeyDictionary {
        current_key_id: 1,
        keys: std::collections::BTreeMap::from([(
            1,
            DataKey {
                key: vec![1; 16],
                method: EncryptionMethod::Aes128Ctr,
                creation_time: 1,
                was_exposed: false,
            },
        )]),
    };
    writer.save(&term, &meta, dict.clone()).await.unwrap();
    reader.load().await.unwrap();
    let old = reader.snapshot();
    let input = region();
    let mut old_encrypted = encrypt_region(Some(&input), Some(&reader))
        .unwrap()
        .into_owned();
    // Go chooses AES by material size after checking that the method is supported.
    dict.current_key_id = 2;
    dict.keys.insert(
        2,
        DataKey {
            key: vec![2; 32],
            method: EncryptionMethod::Aes128Ctr,
            creation_time: 2,
            was_exposed: false,
        },
    );
    let receipt = writer.save(&term, &meta, dict.clone()).await.unwrap();
    writer.load().await.unwrap();
    let mut newer = encrypt_region(Some(&input), Some(&writer))
        .unwrap()
        .into_owned();
    decrypt_region(Some(&mut newer), Some(&reader))
        .await
        .unwrap();
    assert_eq!(newer, input);
    assert_eq!(reader.snapshot().revision, receipt.revision);
    assert_eq!(old.keys.as_ref().unwrap().current_key_id, 1);
    dict.current_key_id = 0;
    writer.save(&term, &meta, dict.clone()).await.unwrap();
    reader.load().await.unwrap();
    assert!(matches!(
        encrypt_region(Some(&input), Some(&reader)).unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    decrypt_region(Some(&mut old_encrypted), Some(&reader))
        .await
        .unwrap();
    assert_eq!(old_encrypted, input);
    dict.current_key_id = 3;
    dict.keys.insert(
        3,
        DataKey {
            key: vec![3; 7],
            method: EncryptionMethod::Aes128Ctr,
            creation_time: 3,
            was_exposed: false,
        },
    );
    writer.save(&term, &meta, dict.clone()).await.unwrap();
    reader.load().await.unwrap();
    assert_eq!(
        encrypt_region(Some(&input), Some(&reader))
            .unwrap_err()
            .to_string(),
        "fail to create aes cipher: crypto/aes: invalid key size 7"
    );
    let mut bad = input.clone();
    bad.encryption_meta = Some(astersql_pd_core::RegionEncryptionMeta {
        key_id: 3,
        iv: vec![],
    });
    let original = bad.clone();
    assert!(matches!(
        decrypt_region(Some(&mut bad), Some(&reader))
            .await
            .unwrap_err(),
        crate::RegionCipherError::KeyLength(7)
    ));
    assert_eq!(bad, original);
    bad.encryption_meta.as_mut().unwrap().key_id = 2;
    let original = bad.clone();
    assert!(matches!(
        decrypt_region(Some(&mut bad), Some(&reader))
            .await
            .unwrap_err(),
        crate::RegionCipherError::IvLength(0)
    ));
    assert_eq!(bad, original);
    dict.current_key_id = 4;
    dict.keys.insert(
        4,
        DataKey {
            key: vec![],
            method: EncryptionMethod::Unknown(99),
            creation_time: 4,
            was_exposed: false,
        },
    );
    writer.save(&term, &meta, dict).await.unwrap();
    reader.load().await.unwrap();
    assert!(matches!(
        encrypt_region(Some(&input), Some(&reader)).unwrap_err(),
        crate::RegionCipherError::Typed(_)
    ));
    term.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_015_retiring_term_cancels_pending_key_io() {
    use crate::{EncryptionConfig, KeyRotation, MasterKeyConfig, MasterKeyFileConfig};
    use std::time::Duration;
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let mut config = EncryptionConfig {
        data_encryption_method: "aes128-ctr".into(),
        master_key: MasterKeyConfig {
            key_type: "file".into(),
            file: MasterKeyFileConfig {
                file_path: file.to_str().unwrap().into(),
            },
            ..Default::default()
        },
        ..Default::default()
    };
    config.adjust().unwrap();
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1503,
            b"owner".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let storage = Arc::new(DictionaryStorage::new(
        store,
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let rotation = Arc::new(KeyRotation::new(storage.clone(), &config).unwrap());
    rotation.set_leadership(Some(term.clone())).await.unwrap();
    let before = storage.snapshot();
    let pause = fixture.suspend();
    let child = rotation.clone();
    let pending = tokio::spawn(async move { child.tick().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    tokio::time::timeout(
        Duration::from_millis(500),
        rotation.clear_leadership_if(&term),
    )
    .await
    .expect("term cleanup must cancel pending metadata request before waiting on rotation mutex");
    assert!(pending.await.unwrap().unwrap().is_none());
    assert_eq!(storage.snapshot().revision, before.revision);
    assert!(Arc::ptr_eq(
        storage.snapshot().keys.as_ref().unwrap(),
        before.keys.as_ref().unwrap()
    ));
    drop(pause);
    term.resign().await.unwrap();
}
