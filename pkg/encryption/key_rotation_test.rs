// Copyright 2026 AsterSQL.
use crate::protocol_adapter::ProtoKeyDictionaryCodec;
use crate::{
    DictionaryStorage, EncryptionConfig, KeyRotation, MasterKeyConfig, MasterKeyFileConfig,
};
use astersql_pd_election::Leadership;
use astersql_pd_storage_domain::MetaStore;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;
fn config(path: &str, method: &str) -> EncryptionConfig {
    let mut config = EncryptionConfig {
        data_encryption_method: method.into(),
        master_key: MasterKeyConfig {
            key_type: "file".into(),
            file: MasterKeyFileConfig {
                file_path: path.into(),
            },
            ..Default::default()
        },
        ..Default::default()
    };
    config.adjust().unwrap();
    config
}
#[tokio::test]
async fn mcs_013_behavior() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1300,
            b"key-owner".to_vec(),
            30,
        )
        .await
        .unwrap(),
    );
    let storage = Arc::new(DictionaryStorage::new(
        store,
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let rotation = KeyRotation::new(
        storage.clone(),
        &config(file.to_str().unwrap(), "aes128-ctr"),
    )
    .unwrap();
    assert!(
        rotation.tick().await.unwrap().is_none(),
        "follower must not create data keys"
    );
    let receipt = rotation
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .expect("valid leader must create and commit an initial data key");
    let snapshot = rotation.snapshot();
    assert_eq!(snapshot.revision, receipt.revision);
    let keys = snapshot.keys.unwrap();
    assert_eq!(keys.keys.len(), 1);
    assert_eq!(keys.keys[&keys.current_key_id].key.len(), 16);
    assert!(!keys.keys[&keys.current_key_id].was_exposed);
    let before = rotation.snapshot().revision;
    assert!(rotation.tick().await.unwrap().is_none());
    assert_eq!(rotation.snapshot().revision, before);
    term.resign().await.unwrap();
    assert!(rotation.tick().await.unwrap().is_none());
    assert_eq!(rotation.snapshot().revision, before);
}

fn seeded_keys() -> crate::KeyDictionary {
    crate::KeyDictionary {
        current_key_id: 123,
        keys: std::collections::BTreeMap::from([(
            123,
            crate::DataKey {
                key: vec![1; 16],
                method: crate::EncryptionMethod::Aes128Ctr,
                creation_time: 1601679533,
                was_exposed: false,
            },
        )]),
    }
}
#[tokio::test]
async fn mcs_013_deadline_method_exposure_and_disable() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1301,
            b"key-owner".to_vec(),
            30,
        )
        .await
        .unwrap(),
    );
    let storage = Arc::new(DictionaryStorage::new(
        store,
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let mut config = config(file.to_str().unwrap(), "aes128-ctr");
    config.data_key_rotation_period = astersql_pd_typeutil::Duration::from_nanos(100_000_000_000);
    storage
        .save(&term, &config.master_key_meta().unwrap(), seeded_keys())
        .await
        .unwrap();
    let now = Arc::new(AtomicI64::new(1601679633_000_000_000));
    let value = now.clone();
    let clock: crate::KeyClock = Arc::new(move || i128::from(value.load(Ordering::SeqCst)));
    let rotation =
        KeyRotation::new_with_resources(storage.clone(), &config, clock.clone(), rand::rngs::OsRng)
            .unwrap();
    rotation
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        rotation.snapshot().keys.as_ref().unwrap().current_key_id,
        123
    );
    assert!(
        rotation.tick().await.unwrap().is_none(),
        "exact expiry is not Before(now)"
    );
    now.fetch_add(1, Ordering::SeqCst);
    rotation.tick().await.unwrap().unwrap();
    let current = rotation.snapshot();
    let keys = current.keys.unwrap();
    assert_ne!(keys.current_key_id, 123);
    assert_eq!(keys.keys.len(), 2);
    assert_eq!(keys.keys[&123], seeded_keys().keys[&123]);
    assert_eq!(keys.keys[&keys.current_key_id].creation_time, 1601679633);
    config.data_encryption_method = "aes256-ctr".into();
    let changed =
        KeyRotation::new_with_resources(storage.clone(), &config, clock.clone(), rand::rngs::OsRng)
            .unwrap();
    changed
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    let keys = changed.snapshot().keys.unwrap();
    assert_eq!(keys.keys.len(), 3);
    assert_eq!(keys.keys[&keys.current_key_id].key.len(), 32);
    assert!(!keys.keys[&keys.current_key_id].was_exposed);
    let mut exposed = config.clone();
    exposed.master_key.key_type = "plaintext".into();
    let expose = KeyRotation::new_with_resources(
        storage.clone(),
        &exposed,
        clock.clone(),
        rand::rngs::OsRng,
    )
    .unwrap();
    expose
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    let exposed_keys = expose.snapshot().keys.unwrap();
    assert_eq!(exposed_keys.current_key_id, keys.current_key_id);
    assert!(exposed_keys.keys.values().all(|k| k.was_exposed));
    let recover =
        KeyRotation::new_with_resources(storage.clone(), &config, clock.clone(), rand::rngs::OsRng)
            .unwrap();
    recover
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    let keys = recover.snapshot().keys.unwrap();
    assert_eq!(keys.keys.len(), 4);
    assert!(!keys.keys[&keys.current_key_id].was_exposed);
    for id in exposed_keys.keys.keys() {
        assert!(keys.keys[id].was_exposed);
    }
    config.data_encryption_method = "plaintext".into();
    let disable =
        KeyRotation::new_with_resources(storage.clone(), &config, clock, rand::rngs::OsRng)
            .unwrap();
    disable
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    let final_keys = disable.snapshot().keys.unwrap();
    assert_eq!(final_keys.current_key_id, 0);
    assert_eq!(final_keys.keys, keys.keys);
    let revision = disable.snapshot().revision;
    assert!(
        disable
            .set_leadership(Some(term.clone()))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(disable.snapshot().revision, revision);
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_013_real_crypto_collisions_and_retry_limit() {
    use crate::{EncryptionMethod, KeyDictionary, new_data_key_with_rng};
    use rand::SeedableRng;
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    for occupied in [3, 10] {
        let fixture = fixture::Fixture::start();
        let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
        let term = Arc::new(
            Leadership::campaign(
                store.clone(),
                CancellationToken::new(),
                1302,
                b"key-owner".to_vec(),
                30,
            )
            .await
            .unwrap(),
        );
        let storage = Arc::new(DictionaryStorage::new(
            store,
            Arc::new(ProtoKeyDictionaryCodec),
        ));
        let config = config(file.to_str().unwrap(), "aes128-ctr");
        let entropy = rand::rngs::StdRng::seed_from_u64(72);
        let mut preview = entropy.clone();
        let mut keys = KeyDictionary::default();
        for _ in 0..occupied {
            let (id, key) =
                new_data_key_with_rng(EncryptionMethod::Aes128Ctr, 1000, &mut preview).unwrap();
            keys.keys.insert(id, key);
        }
        let (expected_id, expected_key) =
            new_data_key_with_rng(EncryptionMethod::Aes128Ctr, 1001, &mut preview).unwrap();
        storage
            .save(&term, &config.master_key_meta().unwrap(), keys.clone())
            .await
            .unwrap();
        let before = storage.load().await.unwrap();
        let rotation = KeyRotation::new_with_resources(
            storage.clone(),
            &config,
            Arc::new(|| 1001_000_000_000),
            entropy,
        )
        .unwrap();
        let result = rotation.set_leadership(Some(term.clone())).await;
        if occupied == 10 {
            assert_eq!(
                result.unwrap_err().to_string(),
                "[PD:encryption:ErrEncryptionRotateDataKey]maximum attempts reached"
            );
            assert_eq!(rotation.snapshot().revision, before.revision);
            assert!(Arc::ptr_eq(
                rotation.snapshot().keys.as_ref().unwrap(),
                before.keys.as_ref().unwrap()
            ));
            rotation.tick().await.unwrap().unwrap();
        } else {
            result.unwrap().unwrap();
        }
        let after = rotation.snapshot().keys.unwrap();
        assert_eq!(after.current_key_id, expected_id);
        assert_eq!(after.keys[&expected_id], expected_key);
        assert_eq!(after.keys.len(), occupied + 1);
        for (id, key) in keys.keys {
            assert_eq!(after.keys[&id], key);
        }
        term.resign().await.unwrap();
    }
}
#[tokio::test]
async fn mcs_013_late_old_term_cannot_publish_candidate() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let old = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1303,
            b"old-owner".to_vec(),
            30,
        )
        .await
        .unwrap(),
    );
    let storage = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let policy = config(file.to_str().unwrap(), "aes256-ctr");
    storage
        .save(&old, &policy.master_key_meta().unwrap(), seeded_keys())
        .await
        .unwrap();
    let before = storage.load().await.unwrap();
    store.revoke(old.lease_id()).await.unwrap();
    assert!(
        !old.is_expired(),
        "captured local TTL can outlive backend ownership"
    );
    let new = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1303,
            b"new-owner".to_vec(),
            30,
        )
        .await
        .unwrap(),
    );
    let rotation = KeyRotation::new_with_resources(
        storage.clone(),
        &policy,
        Arc::new(|| 1601679533_000_000_000),
        rand::rngs::OsRng,
    )
    .unwrap();
    assert_eq!(
        rotation
            .set_leadership(Some(old.clone()))
            .await
            .unwrap_err()
            .to_string(),
        "[PD:encryption:ErrEncryptionSaveDataKeys]leader expired"
    );
    assert_eq!(rotation.snapshot().revision, before.revision);
    assert!(Arc::ptr_eq(
        rotation.snapshot().keys.as_ref().unwrap(),
        before.keys.as_ref().unwrap()
    ));
    let durable = storage.load().await.unwrap();
    assert_eq!(durable.revision, before.revision);
    let mut next_policy = policy.clone();
    next_policy.data_encryption_method = "aes192-ctr".into();
    let replacement = KeyRotation::new_with_resources(
        storage.clone(),
        &next_policy,
        Arc::new(|| 1601679533_000_000_000),
        rand::rngs::OsRng,
    )
    .unwrap();
    replacement
        .set_leadership(Some(new.clone()))
        .await
        .unwrap()
        .unwrap();
    let valid = replacement.snapshot();
    assert_eq!(
        valid.keys.as_ref().unwrap().keys[&valid.keys.as_ref().unwrap().current_key_id]
            .key
            .len(),
        24
    );
    assert!(rotation.tick().await.is_err());
    assert_eq!(rotation.snapshot().revision, valid.revision);
    assert!(Arc::ptr_eq(
        rotation.snapshot().keys.as_ref().unwrap(),
        valid.keys.as_ref().unwrap()
    ));
    old.resign().await.unwrap();
    new.resign().await.unwrap();
}

async fn go_dictionary(
    go: &mut astersql_pd_compat_tests::harness::Server,
) -> (Vec<u8>, crate::KeyDictionary, i64) {
    use crate::KeyDictionaryCodec;
    use astersql_pd_storage_domain::{Consistency, Read};
    let (store, _) = crate::master_key_file_test::go_content(go).await;
    let row = store
        .read(Read::range(
            crate::ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    let codec = ProtoKeyDictionaryCodec;
    let envelope = codec.decode_envelope(&row.value).unwrap();
    let master =
        crate::MasterKey::from_config(envelope.master_key.as_ref(), &envelope.ciphertext_key)
            .await
            .unwrap();
    let keys = codec
        .decode_keys(&master.decrypt(&envelope.content, &envelope.iv).unwrap())
        .unwrap();
    (row.value, keys, row.mod_revision)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_013_live_go_restart_rotation_and_master_rewrap() {
    use astersql_pd_compat_tests::readonly_oracle::pinned_go_binary;
    use astersql_pd_storage_domain::Mutation;
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let file2 = directory.0.join("master2");
    std::fs::write(
        &file2,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .unwrap();
    let mut go = crate::master_key_file_test::start_go(&directory, "file", file.to_str().unwrap());
    let (initial_bytes, initial, initial_revision) = go_dictionary(&mut go).await;
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1304,
            b"rust-owner".to_vec(),
            120,
        )
        .await
        .unwrap(),
    );
    let storage = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    store
        .transact(
            vec![],
            vec![Mutation::put(crate::ENCRYPTION_KEYS_PATH, &initial_bytes)],
        )
        .await
        .unwrap();
    storage.load().await.unwrap();
    let toml = directory.0.join("pd.toml");
    let write_config = |method: &str, path: &str| {
        std::fs::write(&toml,format!("[security.encryption]\ndata-encryption-method='{method}'\n[security.encryption.master-key]\ntype='file'\npath={}\n[log]\nformat='json'\n",serde_json::to_string(path).unwrap())).unwrap();
    };
    write_config("aes128-ctr", file.to_str().unwrap());
    go.restart_with_args(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", toml.display())],
    )
    .unwrap();
    let (rotated_bytes, rotated, rotated_revision) = go_dictionary(&mut go).await;
    assert!(rotated_revision > initial_revision);
    assert_eq!(rotated.keys.len(), initial.keys.len() + 1);
    for (id, key) in &initial.keys {
        assert_eq!(&rotated.keys[id], key);
    }
    let new_key = &rotated.keys[&rotated.current_key_id];
    assert_eq!(new_key.method, crate::EncryptionMethod::Aes128Ctr);
    assert_eq!(new_key.key.len(), 16);
    assert!(!new_key.was_exposed);
    let now = i128::from(new_key.creation_time) * 1_000_000_000;
    let rotation = KeyRotation::new_with_resources(
        storage.clone(),
        &config(file.to_str().unwrap(), "aes128-ctr"),
        Arc::new(move || now),
        rand::rngs::OsRng,
    )
    .unwrap();
    let receipt = rotation
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    let rust = rotation.snapshot().keys.unwrap();
    assert_eq!(rotation.snapshot().revision, receipt.revision);
    assert_eq!(rust.keys.len(), rotated.keys.len());
    for (id, key) in &initial.keys {
        assert_eq!(&rust.keys[id], key);
    }
    let rust_key = &rust.keys[&rust.current_key_id];
    assert_eq!(rust_key.method, new_key.method);
    assert_eq!(rust_key.creation_time, new_key.creation_time);
    assert_eq!(rust_key.was_exposed, new_key.was_exposed);
    assert_eq!(rust_key.key.len(), new_key.key.len());
    println!(
        "Go rotation revisions {initial_revision}->{rotated_revision}; Rust actual commit revision {}; old key IDs/bytes unchanged, new random ID/key compared by metadata",
        receipt.revision
    );
    // Feed the same Go dictionary to both master-key rewrap paths: no random data-key normalization.
    store
        .transact(
            vec![],
            vec![Mutation::put(crate::ENCRYPTION_KEYS_PATH, &rotated_bytes)],
        )
        .await
        .unwrap();
    storage.load().await.unwrap();
    write_config("aes128-ctr", file2.to_str().unwrap());
    go.restart_with_args(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", toml.display())],
    )
    .unwrap();
    let (rewrapped_bytes, rewrapped, rewrapped_revision) = go_dictionary(&mut go).await;
    assert_eq!(rewrapped, rotated);
    assert!(rewrapped_revision > rotated_revision);
    let rewrap = KeyRotation::new_with_resources(
        storage.clone(),
        &config(file2.to_str().unwrap(), "aes128-ctr"),
        Arc::new(move || now),
        rand::rngs::OsRng,
    )
    .unwrap();
    rewrap
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rewrap.snapshot().keys.as_deref(), Some(&rewrapped));
    // A successful disable preserves all historical keys in both implementations.
    store
        .transact(
            vec![],
            vec![Mutation::put(crate::ENCRYPTION_KEYS_PATH, &rewrapped_bytes)],
        )
        .await
        .unwrap();
    storage.load().await.unwrap();
    write_config("plaintext", file2.to_str().unwrap());
    go.restart_with_args(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", toml.display())],
    )
    .unwrap();
    let (_, disabled, disabled_revision) = go_dictionary(&mut go).await;
    assert_eq!(disabled.current_key_id, 0);
    assert_eq!(disabled.keys, rotated.keys);
    assert!(disabled_revision > rewrapped_revision);
    let disable = KeyRotation::new_with_resources(
        storage.clone(),
        &config(file2.to_str().unwrap(), "plaintext"),
        Arc::new(move || now),
        rand::rngs::OsRng,
    )
    .unwrap();
    disable
        .set_leadership(Some(term.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(disable.snapshot().keys.as_deref(), Some(&disabled));
    println!(
        "Go master rewrap revision {rewrapped_revision}, disable revision {disabled_revision}; Rust dictionary metadata/current/history byte-exact after decryption"
    );
    go.stop().unwrap();
    term.resign().await.unwrap();
}
