// Copyright 2026 AsterSQL.
use crate::protocol_adapter::ProtoKeyDictionaryCodec;
use crate::{
    DataKey, DictionaryStorage, ENCRYPTION_KEYS_PATH, EncryptionMethod, KeyDictionary,
    MasterKeyMeta,
};
use astersql_pd_election::Leadership;
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;
fn dictionary() -> KeyDictionary {
    KeyDictionary {
        current_key_id: 200,
        keys: BTreeMap::from([
            (
                100,
                DataKey {
                    key: vec![1; 16],
                    method: EncryptionMethod::Aes128Ctr,
                    creation_time: 1000,
                    was_exposed: false,
                },
            ),
            (
                200,
                DataKey {
                    key: vec![2; 32],
                    method: EncryptionMethod::Aes256Ctr,
                    creation_time: 2000,
                    was_exposed: false,
                },
            ),
        ]),
    }
}
#[tokio::test]
async fn mcs_012_behavior() {
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1200,
        b"dictionary-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let storage = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    store
        .transact(
            vec![],
            vec![astersql_pd_storage_domain::Mutation::put(
                b"encryption_keys_backup",
                b"not an envelope",
            )],
        )
        .await
        .unwrap();
    assert!(storage.load().await.unwrap().keys.is_none());
    let receipt = storage
        .save(&term, &MasterKeyMeta::Plaintext, dictionary())
        .await
        .unwrap();
    assert!(
        receipt.revision > 0,
        "a successful save must commit the real encrypted record"
    );
    assert!(
        storage.snapshot().keys.is_none(),
        "save leaves publication to the loader"
    );
    let loaded = storage.load().await.unwrap();
    assert_eq!(loaded.revision, receipt.revision);
    let keys = loaded.keys.unwrap();
    assert_eq!(keys.current_key_id, 200);
    assert_eq!(keys.keys.len(), 2);
    assert!(keys.keys.values().all(|k| k.was_exposed));
    assert_eq!(keys.keys[&100].key, vec![1; 16]);
    let rows = store
        .read(Read::range(
            ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(rows.entries.len(), 1);
    term.resign().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_012_load_live_go_dictionary() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let mut go = crate::master_key_file_test::start_go(&directory, "file", file.to_str().unwrap());
    let (store, _) = crate::master_key_file_test::go_content(&mut go).await;
    let storage = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    let loaded = storage.load().await.unwrap();
    let keys = loaded.keys.expect("Go saved a readable dictionary");
    assert!(loaded.revision > 0);
    assert_eq!(keys.keys[&keys.current_key_id].key.len(), 32);
    assert!(!keys.keys[&keys.current_key_id].was_exposed);
    println!(
        "Loaded actual Go encryption dictionary at revision {}",
        loaded.revision
    );
    use crate::{KeyDictionaryCodec, KeyEnvelope, MasterKey};
    use astersql_pd_storage_domain::Mutation;
    use std::time::Duration;
    let mut next = (*keys).clone();
    let new_id = if next.current_key_id == 300 { 301 } else { 300 };
    let mut current = next.keys[&next.current_key_id].clone();
    current.creation_time += 1;
    next.keys.insert(new_id, current);
    next.current_key_id = new_id;
    let meta = MasterKeyMeta::File {
        path: file.to_str().unwrap().into(),
    };
    let master = MasterKey::from_config(Some(&meta), &[]).await.unwrap();
    let codec = ProtoKeyDictionaryCodec;
    let (content, iv) = master.encrypt(&codec.encode_keys(&next).unwrap()).unwrap();
    let encoded = codec
        .encode_envelope(&KeyEnvelope {
            content,
            iv,
            master_key: Some(meta),
            ciphertext_key: vec![],
        })
        .unwrap();
    let receipt = store
        .transact(vec![], vec![Mutation::put(ENCRYPTION_KEYS_PATH, &encoded)])
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if go
                .logs()
                .lines()
                .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .any(|v| {
                    v["message"] == "reloaded encryption keys"
                        && v["revision"].as_i64() == Some(receipt.revision)
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let after = storage.load().await.unwrap();
    assert_eq!(after.revision, receipt.revision);
    assert_eq!(after.keys.as_deref(), Some(&next));
    assert_eq!(
        keys.keys.len(),
        1,
        "existing readable snapshot remains immutable"
    );
    println!(
        "Go accepted canonical Rust dictionary with current/history keys at revision {}",
        receipt.revision
    );
    go.stop().unwrap();
}

#[tokio::test]
async fn mcs_012_corrupt_stale_and_deleted_keep_snapshot() {
    use crate::{KeyDictionaryCodec, KeyEnvelope};
    use astersql_pd_storage_domain::{Entry, Mutation};
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1201,
        b"dictionary-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let storage = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    storage
        .save(&term, &MasterKeyMeta::Plaintext, dictionary())
        .await
        .unwrap();
    let before = storage.load().await.unwrap();
    let old = store
        .read(Read::range(
            ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    let stale = Entry {
        value: vec![0xff],
        ..old.clone()
    };
    let same = storage.load_entry(&stale).await.unwrap();
    assert_eq!(same.revision, before.revision);
    assert!(Arc::ptr_eq(
        same.keys.as_ref().unwrap(),
        before.keys.as_ref().unwrap()
    ));
    let codec = ProtoKeyDictionaryCodec;
    let missing_meta = codec
        .encode_envelope(&KeyEnvelope {
            content: vec![],
            master_key: None,
            iv: vec![],
            ciphertext_key: vec![],
        })
        .unwrap();
    let corrupt_keys = codec
        .encode_envelope(&KeyEnvelope {
            content: vec![0xff],
            master_key: Some(MasterKeyMeta::Plaintext),
            iv: vec![],
            ciphertext_key: vec![],
        })
        .unwrap();
    for (value, code) in [
        (vec![0xff], "PD:proto:ErrProtoUnmarshal"),
        (missing_meta, "PD:encryption:ErrEncryptionLoadKeys"),
        (corrupt_keys, "PD:proto:ErrProtoUnmarshal"),
    ] {
        store
            .transact(vec![], vec![Mutation::put(ENCRYPTION_KEYS_PATH, &value)])
            .await
            .unwrap();
        let error = storage.load().await.unwrap_err();
        assert_eq!(error.code(), code);
        let after = storage.snapshot();
        assert_eq!(after.revision, before.revision);
        assert!(Arc::ptr_eq(
            after.keys.as_ref().unwrap(),
            before.keys.as_ref().unwrap()
        ));
    }
    let receipt = store
        .transact(
            vec![],
            vec![Mutation::put(ENCRYPTION_KEYS_PATH, &old.value)],
        )
        .await
        .unwrap();
    let recovered = storage.load().await.unwrap();
    assert_eq!(recovered.revision, receipt.revision);
    assert_eq!(recovered.keys, before.keys);
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: ENCRYPTION_KEYS_PATH.to_vec(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        storage.load().await.unwrap_err().to_string(),
        "[PD:encryption:ErrEncryptionLoadKeys]encryption keys is deleted unexpectedly"
    );
    assert_eq!(storage.snapshot().revision, recovered.revision);
    assert_eq!(storage.snapshot().keys, recovered.keys);
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_012_term_and_real_network_failure_keep_snapshot() {
    use std::time::Duration;
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1202,
        b"dictionary-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let storage = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    storage
        .save(&term, &MasterKeyMeta::Plaintext, dictionary())
        .await
        .unwrap();
    let before = storage.load().await.unwrap();
    let pause = fixture.suspend();
    let result = tokio::time::timeout(
        Duration::from_secs(6),
        storage.save(&term, &MasterKeyMeta::Plaintext, dictionary()),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code(), "PD:etcd:ErrEtcdTxnInternal");
    assert_eq!(storage.snapshot().revision, before.revision);
    assert!(Arc::ptr_eq(
        storage.snapshot().keys.as_ref().unwrap(),
        before.keys.as_ref().unwrap()
    ));
    assert!(
        tokio::time::timeout(Duration::from_secs(6), storage.load())
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(storage.snapshot().revision, before.revision);
    drop(pause);
    // A timed-out transport write has unknown persistence outcome; do not assume
    // absence. Reload actual state after the native process resumes.
    let after = storage.load().await.unwrap();
    assert_eq!(after.keys, before.keys);
    term.resign().await.unwrap();
    let current = storage.snapshot();
    let rows = store
        .read(Read::range(
            ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let revision = rows.entries[0].mod_revision;
    let error = storage
        .save(&term, &MasterKeyMeta::Plaintext, dictionary())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "[PD:encryption:ErrEncryptionSaveDataKeys]leader expired"
    );
    assert_eq!(storage.snapshot().revision, current.revision);
    assert_eq!(
        store
            .read(Read::range(
                ENCRYPTION_KEYS_PATH,
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries[0]
            .mod_revision,
        revision
    );
}
#[tokio::test]
async fn mcs_012_file_encryption_and_corrupt_authentication() {
    use crate::{KeyDictionaryCodec, MasterKey};
    use astersql_pd_storage_domain::Mutation;
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
        1203,
        b"dictionary-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let storage = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    let codec = ProtoKeyDictionaryCodec;
    let expected = dictionary();
    let receipt = storage.save(&term, &meta, expected.clone()).await.unwrap();
    let loaded = storage.load().await.unwrap();
    assert_eq!(loaded.revision, receipt.revision);
    assert_eq!(loaded.keys.as_deref(), Some(&expected));
    let row = store
        .read(Read::range(
            ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    let mut envelope = codec.decode_envelope(&row.value).unwrap();
    assert_eq!(envelope.master_key, Some(meta.clone()));
    assert_eq!(envelope.iv.len(), 12);
    assert!(envelope.ciphertext_key.is_empty());
    let master = MasterKey::from_config(Some(&meta), &[]).await.unwrap();
    assert_eq!(
        codec
            .decode_keys(&master.decrypt(&envelope.content, &envelope.iv).unwrap())
            .unwrap(),
        expected
    );
    envelope.content[0] ^= 1;
    store
        .transact(
            vec![],
            vec![Mutation::put(
                ENCRYPTION_KEYS_PATH,
                &codec.encode_envelope(&envelope).unwrap(),
            )],
        )
        .await
        .unwrap();
    assert_eq!(
        storage.load().await.unwrap_err().code(),
        "PD:encryption:ErrEncryptionGCMDecrypt"
    );
    assert_eq!(storage.snapshot().revision, loaded.revision);
    assert!(Arc::ptr_eq(
        storage.snapshot().keys.as_ref().unwrap(),
        loaded.keys.as_ref().unwrap()
    ));
    term.resign().await.unwrap();
}

#[test]
fn mcs_012_protocol_metadata_and_future_method() {
    use crate::{KeyDictionaryCodec, KeyEnvelope};
    let codec = ProtoKeyDictionaryCodec;
    let mut keys = dictionary();
    keys.keys.get_mut(&100).unwrap().method = EncryptionMethod::Unknown(-17);
    assert_eq!(
        codec
            .decode_keys(&codec.encode_keys(&keys).unwrap())
            .unwrap(),
        keys
    );
    for meta in [
        MasterKeyMeta::Plaintext,
        MasterKeyMeta::File {
            path: "主密钥".into(),
        },
        MasterKeyMeta::Kms {
            vendor: format!("future-{}", 42),
            key_id: "id".into(),
            region: "region".into(),
            endpoint: "endpoint".into(),
        },
    ] {
        let expected = KeyEnvelope {
            content: vec![1, 2, 3],
            master_key: Some(meta.clone()),
            iv: vec![4],
            ciphertext_key: vec![5],
        };
        let got = codec
            .decode_envelope(&codec.encode_envelope(&expected).unwrap())
            .unwrap();
        assert_eq!(got.master_key, Some(meta));
        assert_eq!(got.content, expected.content);
        assert_eq!(got.iv, expected.iv);
        assert_eq!(got.ciphertext_key, expected.ciphertext_key);
    }
}
